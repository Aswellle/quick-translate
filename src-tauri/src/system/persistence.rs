// src-tauri/src/system/persistence.rs
// 落盘 worker：把历史与缓存的写入从翻译主链路上彻底摘下来（计划第 24 节）。
//
// 重构前的写法是「每翻译成功一次就 spawn 一个任务」：
//
//     tauri::async_runtime::spawn(async move {
//         history.insert(...).await;
//         history.enforce_limit(...).await;
//     });
//
// 每次翻译一个游离任务，没有任何统一的生命周期 —— 长期驻留时它们会各自
// 持有 DB 连接锁、各自决定何时结束。计划第 41 节明确禁止这种形态
// （「禁止每个事件 spawn 一个长期 task」「所有 channel 必须有界」）。
//
// 现在：一条**有界**队列 + **唯一**一个常驻 worker。
//
// 关键性质：翻译结果在入队之前就已经 emit 给用户了，因此数据库再慢也
// 影响不到用户看到的译文；队列满时丢弃并告警，而不是把主链路堵住。

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::domain::cache::{CacheEntry, TranslationCache};
use crate::domain::history::HistoryRepository;
use crate::types::TranslationRecord;

/// 队列容量。
///
/// 写盘远快于翻译（一次翻译至少几百毫秒往返，一次 insert 是本地毫秒级），
/// 32 足够吸收任何合理突发。超出说明数据库卡住了 —— 此时丢弃比无限堆积好，
/// 计划第 41 节要求所有 channel 有界。
pub const QUEUE_CAPACITY: usize = 32;

/// 缓存条目的拥有值形式。
///
/// `CacheEntry<'a>` 借用 `&str`，无法跨任务传递到 worker 里去，所以这里
/// 先拷成拥有值。`provider` 与 `source_lang` 都是短字符串，拷贝代价可忽略。
#[derive(Debug, Clone)]
pub struct OwnedCacheEntry {
    pub source_text: String,
    pub target_lang: String,
    pub translated_text: String,
    pub source_lang: String,
    pub provider: String,
}

impl OwnedCacheEntry {
    fn as_entry(&self) -> CacheEntry<'_> {
        CacheEntry {
            source_text: &self.source_text,
            target_lang: &self.target_lang,
            translated_text: &self.translated_text,
            source_lang: &self.source_lang,
            provider: &self.provider,
        }
    }
}

/// 一次成功的翻译要落盘的全部内容。
///
/// 历史与缓存总是一起写，所以是一个任务而不是两个 —— 两个队列只会让
/// 二者之间产生无意义的时序差。
#[derive(Debug, Clone)]
pub struct PersistJob {
    pub record: TranslationRecord,
    pub history_limit: i64,
    pub cache_entry: OwnedCacheEntry,
}

pub struct PersistenceWriter {
    tx: mpsc::Sender<PersistJob>,
}

impl PersistenceWriter {
    pub fn spawn(history: Arc<HistoryRepository>, cache: Arc<TranslationCache>) -> Self {
        Self::with_capacity(history, cache, QUEUE_CAPACITY)
    }

    /// 可指定容量的构造，供测试构造「队列满」的情形。
    pub fn with_capacity(
        history: Arc<HistoryRepository>,
        cache: Arc<TranslationCache>,
        capacity: usize,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<PersistJob>(capacity);

        tauri::async_runtime::spawn(async move {
            info!("[persistence] worker 已启动");
            while let Some(job) = rx.recv().await {
                process(&history, &cache, job).await;
            }
            // 所有 sender 都 drop 了（进程退出路径）
            info!("[persistence] 队列已关闭，worker 退出");
        });

        Self { tx }
    }

    /// 入队。返回 false 表示队列已满或 worker 已停止，本次写入被丢弃。
    ///
    /// 用 `try_send` 而不是 `send().await`：调用方处在翻译结果的回传路径上，
    /// 绝不能因为数据库慢而被堵住。丢弃一次历史/缓存写入，比让用户多等
    /// 或者让浮窗卡住划算得多。
    pub fn enqueue(&self, job: PersistJob) -> bool {
        match self.tx.try_send(job) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => false,
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// 测试用：只建通道、不启动 worker。
    ///
    /// 故意泄漏 receiver —— 通道保持打开但无人消费，「队列满」因此是确定状态，
    /// 不必和 worker 的消费速度赛跑。
    #[cfg(test)]
    fn channel_only(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        std::mem::forget(rx);
        Self { tx }
    }
}

/// 处理一个任务。
///
/// **刻意不做重试**：连接已设 `busy_timeout = 5000`，SQLite 的锁竞争由它
/// 兜住；再叠一层重试只会把同样的等待做两遍。剩下的失败是真实错误
/// （磁盘满、库损坏），重试也修不好，如实记录即可。
async fn process(history: &HistoryRepository, cache: &TranslationCache, job: PersistJob) {
    let PersistJob {
        record,
        history_limit,
        cache_entry,
    } = job;

    match history.insert(&record).await {
        Ok(()) => {
            if let Err(e) = history.enforce_limit(history_limit).await {
                error!(event = "history_limit_failed", "历史清理失败: {}", e);
            }
        }
        Err(e) => {
            error!(event = "history_write_failed", "历史记录写入失败: {}", e);
        }
    }

    match cache.put(cache_entry.as_entry()).await {
        Ok(()) => {
            if let Err(e) = cache.enforce_limits().await {
                warn!(event = "cache_limit_failed", "本地缓存清理失败: {}", e);
            }
        }
        Err(e) => {
            // 缓存写失败对用户完全无感：下次联网时照常翻译
            warn!(event = "cache_write_failed", "本地缓存写入失败: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rusqlite::Connection;
    use tokio::sync::Mutex;

    use super::*;
    use crate::types::HistoryQuery;

    /// 跑完整迁移的内存库。
    ///
    /// 顺带验证了「全套迁移能在一个全新库上干净跑完」—— 新增 v6 之后
    /// 这条最容易坏。
    fn migrated_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        crate::infra::database::run_migrations(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn job(text: &str) -> PersistJob {
        PersistJob {
            record: TranslationRecord {
                id: format!("id-{}", text),
                source_text: text.to_string(),
                translated_text: "译文".to_string(),
                source_lang: "en".to_string(),
                target_lang: "zh".to_string(),
                provider: "deepl".to_string(),
                created_at: crate::types::now_unix_ms(),
                duration_ms: Some(1),
                is_starred: false,
            },
            history_limit: 200,
            cache_entry: OwnedCacheEntry {
                source_text: text.to_string(),
                target_lang: "zh".to_string(),
                translated_text: "译文".to_string(),
                source_lang: "en".to_string(),
                provider: "deepl".to_string(),
            },
        }
    }

    #[test]
    fn migrations_bring_a_fresh_database_to_the_latest_version() {
        let db = migrated_db();
        let conn = db.blocking_lock();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 6, "全新库应跑到最新迁移版本");

        let cache_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='translation_cache'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cache_table, 1, "v6 应建出 translation_cache 表");
    }

    #[tokio::test]
    async fn writer_persists_to_history_and_cache() {
        let db = migrated_db();
        let history = Arc::new(HistoryRepository::new(db.clone()));
        let cache = Arc::new(TranslationCache::new(db.clone()));
        let writer = PersistenceWriter::spawn(history.clone(), cache.clone());

        assert!(writer.enqueue(job("hello")), "队列应有空位");

        // 轮询等待落盘，不用固定 sleep（那会让测试要么慢要么偶发失败）
        let mut history_count = 0;
        for _ in 0..200 {
            history_count = history.query(&HistoryQuery::default()).await.unwrap().len();
            if history_count == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(history_count, 1, "worker 应在期限内写入历史");

        assert!(
            cache.get("hello", "zh").await.unwrap().is_some(),
            "worker 应同时写入本地缓存"
        );
    }

    #[tokio::test]
    async fn writer_processes_a_burst_in_order() {
        let db = migrated_db();
        let history = Arc::new(HistoryRepository::new(db.clone()));
        let cache = Arc::new(TranslationCache::new(db.clone()));
        let writer = PersistenceWriter::spawn(history.clone(), cache.clone());

        for i in 0..10 {
            assert!(writer.enqueue(job(&format!("text-{}", i))));
        }

        let mut count = 0;
        for _ in 0..200 {
            count = history.query(&HistoryQuery::default()).await.unwrap().len();
            if count == 10 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(count, 10, "一个 worker 应把整批任务都处理完，不丢件");
    }

    /// 队列满时必须**如实返回 false**，而不是阻塞调用方 ——
    /// 调用方正处在翻译结果的回传路径上。
    #[tokio::test]
    async fn enqueue_reports_failure_once_the_queue_is_full() {
        let writer = PersistenceWriter::channel_only(1);

        assert!(writer.enqueue(job("a")), "第一个任务应入队成功");
        assert!(
            !writer.enqueue(job("b")),
            "队列满时应返回 false，绝不能把翻译回传路径堵住"
        );
    }

    #[test]
    fn queue_capacity_is_bounded() {
        assert!(
            QUEUE_CAPACITY > 0 && QUEUE_CAPACITY <= 256,
            "计划第 41 节要求所有 channel 有界且不夸张"
        );
    }
}
