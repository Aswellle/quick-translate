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

use tauri::Emitter;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::domain::cache::{CacheEntry, TranslationCache};
use crate::domain::history::HistoryRepository;
use crate::runtime::{ComponentState, RuntimeStatus};
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
    pub fn spawn(
        app: tauri::AppHandle,
        history: Arc<HistoryRepository>,
        cache: Arc<TranslationCache>,
        runtime: Arc<RuntimeStatus>,
    ) -> Self {
        Self::with_capacity(app, history, cache, runtime, QUEUE_CAPACITY)
    }

    /// 可指定容量的构造，供测试构造「队列满」的情形。
    pub fn with_capacity(
        app: tauri::AppHandle,
        history: Arc<HistoryRepository>,
        cache: Arc<TranslationCache>,
        runtime: Arc<RuntimeStatus>,
        capacity: usize,
    ) -> Self {
        Self::run(Some(app), history, cache, runtime, capacity)
    }

    /// 测试专用：没有 Tauri 应用上下文，不广播 history-changed。
    #[cfg(test)]
    fn spawn_without_events(
        history: Arc<HistoryRepository>,
        cache: Arc<TranslationCache>,
        runtime: Arc<RuntimeStatus>,
    ) -> Self {
        Self::run(None, history, cache, runtime, QUEUE_CAPACITY)
    }

    fn run(
        app: Option<tauri::AppHandle>,
        history: Arc<HistoryRepository>,
        cache: Arc<TranslationCache>,
        runtime: Arc<RuntimeStatus>,
        capacity: usize,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<PersistJob>(capacity);

        tauri::async_runtime::spawn(async move {
            info!("[persistence] worker 已启动");
            while let Some(job) = rx.recv().await {
                let history_written = process(&history, &cache, &runtime, job).await;
                if history_written {
                    // 历史窗口 / 设置仪表盘监听这条事件后各自刷新 ——
                    // 这两个窗口都是 hide 而非销毁，组件树永不重挂载，
                    // 没有这条事件它们会一直展示打开时的旧快照
                    if let Some(app) = app.as_ref() {
                        let _ = app.emit("history-changed", ());
                    }
                }
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
/// 历史写入带**一次**重试（计划第 24 节：retry → 仍失败 → 记录 → 丢弃）。
/// 首次尝试里的 SQLite 锁竞争由 `busy_timeout = 5000` 兜住，因此重试针对的
/// 是另一种短暂故障 —— 杀软扫描、磁盘瞬时卡顿这类；重试仍失败就是真实
/// 错误（磁盘满、库损坏），按计划记录 error 后丢弃，绝不反噬主链路。
///
/// 健康语义（QT-008）：写入成功但**保留清理失败**不是健康 —— 清理失效
/// 意味着保留上限形同虚设，history/cache 会无界膨胀。清理失败以稳定
/// 错误码（`HISTORY_RETENTION_FAILED` / `CACHE_RETENTION_FAILED`）上报
/// Degraded；随后的成功任务会自然把状态带回 Healthy，一次暂态失败
/// 不会永久标红。
async fn process(
    history: &HistoryRepository,
    cache: &TranslationCache,
    runtime: &RuntimeStatus,
    job: PersistJob,
) -> bool {
    let PersistJob {
        record,
        history_limit,
        cache_entry,
    } = job;

    let mut storage_error: Option<&'static str> = None;
    let mut history_written = false;

    match history.insert(&record).await {
        Ok(()) => {
            history_written = true;
            if let Err(e) = history.enforce_limit(history_limit).await {
                error!(
                    event = "history_limit_failed",
                    code = "HISTORY_RETENTION_FAILED",
                    "历史清理失败（保留上限失效）: {}",
                    e
                );
                storage_error.get_or_insert("HISTORY_RETENTION_FAILED");
            }
        }
        Err(first) => {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            match history.insert(&record).await {
                Ok(()) => {
                    history_written = true;
                    info!(
                        event = "history_write_retried_ok",
                        "历史写入首次失败后重试成功: {}", first
                    );
                    if let Err(e) = history.enforce_limit(history_limit).await {
                        error!(
                            event = "history_limit_failed",
                            code = "HISTORY_RETENTION_FAILED",
                            "历史清理失败（保留上限失效）: {}",
                            e
                        );
                        storage_error.get_or_insert("HISTORY_RETENTION_FAILED");
                    }
                }
                Err(second) => {
                    error!(
                        event = "history_write_failed",
                        "历史记录写入失败（含一次重试）: 首次 {} / 重试 {}", first, second
                    );
                    storage_error = Some("HISTORY_WRITE_FAILED");
                }
            }
        }
    }

    match cache.put(cache_entry.as_entry()).await {
        Ok(()) => {
            if let Err(e) = cache.enforce_limits().await {
                warn!(
                    event = "cache_limit_failed",
                    code = "CACHE_RETENTION_FAILED",
                    "本地缓存清理失败（保留上限失效）: {}",
                    e
                );
                storage_error.get_or_insert("CACHE_RETENTION_FAILED");
            }
        }
        Err(e) => {
            // 缓存写失败对用户完全无感：下次联网时照常翻译
            warn!(event = "cache_write_failed", "本地缓存写入失败: {}", e);
            storage_error.get_or_insert("CACHE_WRITE_FAILED");
        }
    }

    // 存储健康上报给运行时层：界面不该从「历史里没这条」去推断数据库坏了
    match storage_error {
        Some(code) => runtime.report_storage(ComponentState::Degraded, Some(code)),
        None => runtime.report_storage(ComponentState::Healthy, None),
    }

    history_written
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

    /// 运行时层不依赖 Tauri，因此测试里可以正常构造
    fn test_runtime() -> Arc<RuntimeStatus> {
        RuntimeStatus::new(
            Arc::new(crate::system::clipboard::MonitorController::new()),
            crate::runtime::status::no_notifier(),
        )
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
        let history = Arc::new(HistoryRepository::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let cache = Arc::new(TranslationCache::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let runtime = test_runtime();
        let writer = PersistenceWriter::spawn_without_events(
            history.clone(),
            cache.clone(),
            runtime.clone(),
        );

        assert!(writer.enqueue(job("hello")), "队列应有空位");

        // 轮询等待落盘，不用固定 sleep（那会让测试要么慢要么偶发失败）。
        //
        // 等待条件必须是「历史与缓存都已写入」二者同时成立，而不是先等
        // 历史、再裸断言缓存：worker 跑在 tauri 的全局运行时线程上，
        // `cache.put` 是 `history.insert` 之后的下一个 await —— 高负载下
        // 测试线程完全可能在两步之间观察到历史行已提交而缓存还没写，
        // 随后的缓存断言就会偶发失败（这正是本测试曾经的 flake）。
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut history_count = 0;
        while std::time::Instant::now() < deadline {
            history_count = history.query(&HistoryQuery::default()).await.unwrap().len();
            if history_count == 1 && cache.get("hello", "zh").await.unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
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
        let history = Arc::new(HistoryRepository::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let cache = Arc::new(TranslationCache::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let runtime = test_runtime();
        let writer = PersistenceWriter::spawn_without_events(
            history.clone(),
            cache.clone(),
            runtime.clone(),
        );

        for i in 0..10 {
            assert!(writer.enqueue(job(&format!("text-{}", i))));
        }

        // 同上：轮询到全部写入完成，deadline 制（10s）而不是固定次数。
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut count = 0;
        while std::time::Instant::now() < deadline {
            count = history.query(&HistoryQuery::default()).await.unwrap().len();
            if count == 10 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
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

    // ── QT-008：保留清理失败必须反映到存储健康 ──────────────────────────

    /// 故障注入：给表挂一个 BEFORE DELETE ... RAISE(ABORT) 触发器。
    /// 这样 INSERT 依旧成功而 DELETE 必然失败 —— 与真实的清理故障
    /// （磁盘满 / 库损坏 / 权限）走同一条 rusqlite 错误路径，且完全确定。
    fn inject_delete_failure(conn: &Connection, table: &str) {
        conn.execute(
            &format!(
                "CREATE TRIGGER fail_delete_{table} BEFORE DELETE ON {table}
                 BEGIN SELECT RAISE(ABORT, 'injected retention failure'); END"
            ),
            [],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn history_retention_failure_is_reported_as_degraded() {
        let db = migrated_db();
        let history = Arc::new(HistoryRepository::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let cache = Arc::new(TranslationCache::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let runtime = test_runtime();

        inject_delete_failure(&db.try_lock().unwrap(), "translation_records");

        // 预置超出限额的旧行：enforce_limit 必须真的执行删除，
        // BEFORE DELETE 触发器才会命中（表内无需删除时 RAISE 不触发）
        {
            let conn = db.try_lock().unwrap();
            for i in 0..3 {
                conn.execute(
                    "INSERT INTO translation_records
                     (id, source_text, translated_text, source_lang, target_lang,
                      provider, created_at, duration_ms, is_starred)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 0)",
                    rusqlite::params![
                        format!("old-{}", i),
                        "old",
                        "旧",
                        "en",
                        "zh",
                        "deepl",
                        1000 + i
                    ],
                )
                .unwrap();
            }
        }

        let mut oversized = job("retention-fails");
        oversized.history_limit = 2;

        process(&history, &cache, &runtime, oversized).await;

        let storage = runtime.snapshot().storage;
        assert_eq!(
            storage.state,
            ComponentState::Degraded,
            "清理失败不得被报告为健康 —— 保留上限失效会让历史无界膨胀"
        );
        assert_eq!(
            storage.last_error_code.as_deref(),
            Some("HISTORY_RETENTION_FAILED")
        );
    }

    #[tokio::test]
    async fn cache_retention_failure_is_reported_as_degraded() {
        let db = migrated_db();
        let history = Arc::new(HistoryRepository::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let cache = Arc::new(TranslationCache::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let runtime = test_runtime();

        inject_delete_failure(&db.try_lock().unwrap(), "translation_cache");

        // 预置一条已过期（created_at 远早于 TTL）的缓存行：
        // enforce_limits 的第一条 DELETE 就会命中它 → 触发注入的故障
        {
            let conn = db.try_lock().unwrap();
            conn.execute(
                "INSERT INTO translation_cache
                 (cache_key, source_text, translated_text, source_lang, provider,
                  created_at, last_hit_at, hit_count)
                 VALUES ('expired-key', 'old', '旧', 'en', 'deepl', 1000, 1000, 0)",
                [],
            )
            .unwrap();
        }

        process(&history, &cache, &runtime, job("cache-fails")).await;

        let storage = runtime.snapshot().storage;
        assert_eq!(storage.state, ComponentState::Degraded);
        assert_eq!(
            storage.last_error_code.as_deref(),
            Some("CACHE_RETENTION_FAILED")
        );
    }

    /// 一次暂态的清理失败不能永久标红：故障消除后的下一个成功任务
    /// 必须把存储状态带回 Healthy。
    #[tokio::test]
    async fn storage_health_recovers_after_retention_failure() {
        let db = migrated_db();
        let history = Arc::new(HistoryRepository::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let cache = Arc::new(TranslationCache::new(
            crate::infra::database::Db::available(db.clone()),
        ));
        let runtime = test_runtime();

        inject_delete_failure(&db.try_lock().unwrap(), "translation_records");
        {
            let conn = db.try_lock().unwrap();
            for i in 0..3 {
                conn.execute(
                    "INSERT INTO translation_records
                     (id, source_text, translated_text, source_lang, target_lang,
                      provider, created_at, duration_ms, is_starred)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 0)",
                    rusqlite::params![
                        format!("old-{}", i),
                        "old",
                        "旧",
                        "en",
                        "zh",
                        "deepl",
                        1000 + i
                    ],
                )
                .unwrap();
            }
        }
        let mut oversized = job("bad");
        oversized.history_limit = 2;
        process(&history, &cache, &runtime, oversized).await;
        assert_eq!(runtime.snapshot().storage.state, ComponentState::Degraded);

        // 故障消除
        db.try_lock()
            .unwrap()
            .execute("DROP TRIGGER fail_delete_translation_records", [])
            .unwrap();
        process(&history, &cache, &runtime, job("good")).await;

        let storage = runtime.snapshot().storage;
        assert_eq!(storage.state, ComponentState::Healthy, "恢复后必须回到健康");
        // last_error_code 是刻意保留的诊断历史（最后一次错误的稳定码），
        // 恢复这一事实由 last_recovery_at_ms 表达 —— 状态与历史是两个字段
        assert!(
            storage.last_recovery_at_ms.is_some(),
            "从故障恢复必须记录恢复时刻"
        );
    }
}
