// src-tauri/src/domain/cache.rs
// 翻译结果的精确缓存（计划第 16 节）。
//
// 目的只有一个：**网络不可用时，之前译过的内容仍能显示**。
//
// 计划第 15 节明确划了产品边界：本项目依赖云端翻译源，因此「离线也能翻译
// 任何新文本」是做不到的承诺，不该假装能做到。该做的是：
//
//     命中缓存   → 立刻显示，并标明来自本地缓存
//     未命中     → 明确告知暂时连不上，等网络恢复后自动继续
//
// 只在所有翻译源都用不了时才查缓存（见下方 `should_fall_back_to_cache`，
// coordinator 的剪贴板路径与 translate_text 的手动路径共用），因此在线时
// 结果永远是新鲜的 —— 缓存不会让用户看到过期译文。

use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::types::now_unix_ms;

/// 缓存条数上限（计划第 16 节建议从 1000~5000 起步）。
///
/// 超出后按最近命中时间淘汰 —— 常翻的内容留下，一次性的走掉。
pub const MAX_ENTRIES: i64 = 2_000;

/// 缓存存活时长。
///
/// 同一段文本的译文本身不会「过期」，设 TTL 是为了给磁盘占用兜底，
/// 同时让翻译源改善后有机会产出更好的结果。
pub const TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// 超过这个长度的文本不进缓存：长文命中率低，却最占空间。
pub const MAX_CACHED_TEXT_CHARS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedTranslation {
    pub translated_text: String,
    pub provider: String,
    pub source_lang: String,
}

/// 写入缓存的一条内容。
///
/// 用具名结构而不是五个位置参数：`provider` 与 `source_lang` 都是短字符串，
/// 位置传错编译器不会吭声。
pub struct CacheEntry<'a> {
    pub source_text: &'a str,
    pub target_lang: &'a str,
    pub translated_text: &'a str,
    pub source_lang: &'a str,
    pub provider: &'a str,
    /// 本次请求的作用域令牌（空串 = 无作用域判别），进入缓存键，
    /// 见 [`scope_token`]。
    pub scope_token: &'a str,
}

pub struct TranslationCache {
    db: crate::infra::database::Db,
    max_entries: i64,
    ttl: Duration,
    /// 读取/写入开关（配置键 `cache_enabled`，默认开启）。
    ///
    /// 关闭 ≠ 删除：已有条目原样保留，用户仍可通过「清空缓存」显式删除。
    /// 关闭期间不读不写，重新打开后旧条目（未过期者）恢复可用 ——
    /// 把开关做成「暂停」而不是「销毁」，避免一次误点丢掉所有离线译文。
    enabled: std::sync::atomic::AtomicBool,
}

impl TranslationCache {
    pub fn new(db: crate::infra::database::Db) -> Self {
        Self {
            db,
            max_entries: MAX_ENTRIES,
            ttl: TTL,
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// 测试用：可调上限与 TTL（否则验证淘汰与过期得等 30 天）
    #[cfg(test)]
    pub fn with_limits(db: crate::infra::database::Db, max_entries: i64, ttl: Duration) -> Self {
        Self {
            db,
            max_entries,
            ttl,
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// 运行时切换读写开关。原子操作：翻译与落盘 worker 分别在不同任务里
    /// 读它，不需要额外锁。
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 存储不可用（计划第 26 节的降级路径）：缓存是纯增益能力，
    /// 读视为无命中、写视为成功静默丢弃，绝不向主链路抛错。
    fn conn(&self) -> Option<Arc<Mutex<Connection>>> {
        self.db.try_conn()
    }

    /// 缓存键：归一化文本 + 目标语言 + 作用域令牌。
    ///
    /// 用 `\u{1}` 分隔而不是直接拼接：否则 "ab"+"c" 与 "a"+"bc" 会撞成同一个键。
    /// 分隔符取控制字符，正常文本里不可能出现。令牌为空（无显式源语言、
    /// 无术语表、控制台术语库全关）时与引入作用域之前的键完全一致，
    /// 既有缓存不失效。
    fn key(text: &str, target_lang: &str, scope_token: &str) -> String {
        if scope_token.is_empty() {
            format!("{}\u{1}{}", text, target_lang)
        } else {
            format!("{}\u{1}{}\u{1}{}", text, target_lang, scope_token)
        }
    }

    /// 该文本是否值得缓存
    pub fn is_cacheable(text: &str) -> bool {
        let n = text.chars().count();
        n > 0 && n <= MAX_CACHED_TEXT_CHARS
    }

    /// 精确命中。过期的条目不返回（但留到 enforce_limits 再清理，
    /// 免得每次读都写一次库）。存储不可用 → 无命中（Ok(None)）。
    pub async fn get(
        &self,
        text: &str,
        target_lang: &str,
        scope_token: &str,
    ) -> Result<Option<CachedTranslation>, AppError> {
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(None);
        }
        if !Self::is_cacheable(text) {
            return Ok(None);
        }

        let Some(conn) = self.conn() else {
            return Ok(None);
        };

        let key = Self::key(text, target_lang, scope_token);
        let cutoff = now_unix_ms() - self.ttl.as_millis() as i64;
        let conn = conn.lock().await;

        let hit = conn
            .query_row(
                "SELECT translated_text, provider, source_lang
                   FROM translation_cache
                  WHERE cache_key = ?1 AND created_at >= ?2",
                rusqlite::params![key, cutoff],
                |row| {
                    Ok(CachedTranslation {
                        translated_text: row.get(0)?,
                        provider: row.get(1)?,
                        source_lang: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        if hit.is_some() {
            // LRU 记账：命中时间决定未来谁被淘汰
            conn.execute(
                "UPDATE translation_cache
                    SET last_hit_at = ?2, hit_count = hit_count + 1
                  WHERE cache_key = ?1",
                rusqlite::params![key, now_unix_ms()],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        }

        Ok(hit)
    }

    /// 写入缓存。同一段文本重复翻译时更新译文，
    /// 但**保留** last_hit_at 与 hit_count —— 那是这段内容的热度记录。
    /// 存储不可用 → 静默成功（写入本来就不影响主链路）。
    pub async fn put(&self, entry: CacheEntry<'_>) -> Result<(), AppError> {
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        if !Self::is_cacheable(entry.source_text) {
            return Ok(());
        }

        let Some(conn) = self.conn() else {
            return Ok(());
        };

        let now = now_unix_ms();
        let conn = conn.lock().await;
        conn.execute(
            "INSERT INTO translation_cache
                 (cache_key, source_text, translated_text, source_lang, provider,
                  created_at, last_hit_at, hit_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0)
             ON CONFLICT(cache_key) DO UPDATE SET
                 translated_text = excluded.translated_text,
                 source_lang     = excluded.source_lang,
                 provider        = excluded.provider,
                 created_at      = excluded.created_at",
            rusqlite::params![
                Self::key(entry.source_text, entry.target_lang, entry.scope_token),
                entry.source_text,
                entry.translated_text,
                entry.source_lang,
                entry.provider,
                now
            ],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 清理：先删过期条目，再按 LRU 淘汰到上限以内。
    ///
    /// 由调用方在写缓存后择机触发（与历史记录的 `enforce_limit` 同构）。
    /// 存储不可用 → 无事可做，静默成功。
    pub async fn enforce_limits(&self) -> Result<(), AppError> {
        let Some(conn) = self.conn() else {
            return Ok(());
        };
        let conn = conn.lock().await;

        let cutoff = now_unix_ms() - self.ttl.as_millis() as i64;
        conn.execute(
            "DELETE FROM translation_cache WHERE created_at < ?1",
            rusqlite::params![cutoff],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM translation_cache", [], |row| {
                row.get(0)
            })
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        if count > self.max_entries {
            conn.execute(
                "DELETE FROM translation_cache WHERE cache_key IN (
                     SELECT cache_key FROM translation_cache
                      ORDER BY last_hit_at ASC
                      LIMIT ?1)",
                rusqlite::params![count - self.max_entries],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        }

        Ok(())
    }

    /// 清空全部缓存条目，返回删除的条数（供前端反馈「清除了 N 条」）。
    ///
    /// 隐私语义（计划 3.3）：缓存条目含原文与译文，属于本地敏感数据，
    /// 用户必须能单独清掉它而不动历史。存储不可用 → 无可清，按 0 处理。
    pub async fn clear_and_count(&self) -> Result<i64, AppError> {
        let Some(conn) = self.conn() else {
            return Ok(0);
        };
        let conn = conn.lock().await;
        let deleted = conn
            .execute("DELETE FROM translation_cache", [])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(deleted as i64)
    }

    /// 测试用：当前条目数
    #[cfg(test)]
    pub async fn len(&self) -> i64 {
        let conn = self.conn().expect("测试要求存储可用");
        let conn = conn.lock().await;
        conn.query_row("SELECT COUNT(*) FROM translation_cache", [], |row| {
            row.get(0)
        })
        .unwrap()
    }
}

// ── 离线回退（计划第 15 节；coordinator 与手动翻译路径共用）─────────────

/// 缓存来源的 provider 标识。
///
/// 前端 `PROVIDER_LABELS` 里对应「本地缓存」。复用 provider 字段而不是给
/// `TranslationResult` 加一个 `from_cache` 标志：既有的 SameLanguage 路径
/// 已经在用非翻译商取值（"none"），说明这个字段本就是「来源」而非
/// 「翻译商」，加字段会连带改动前端类型与所有展示组件。
pub const CACHE_PROVIDER: &str = "cache";

/// 是否该回退到本地缓存。
///
/// 只在「所有翻译源都用不了」时回退 —— 那才是离线信号的形状。Permanent
/// 不回退：换谁都一样，缓存里也不会有更对的东西；SameLanguage 根本不是
/// 失败。这两种情况把真实原因告诉用户，比递一个来源不明的旧译文有用。
pub fn should_fall_back_to_cache(err: &AppError) -> bool {
    matches!(err, AppError::AllProvidersFailed { .. })
}

/// 构造缓存键的作用域令牌：把「会改变同一段文本 + 目标语言之译文含义」
/// 的请求上下文全部编进去。
///
/// 此前键里只有 DeepL 术语表指纹 —— 用户切换显式源语言、开关百度术语库、
/// 改有道词汇表/腾讯术语库之后，旧上下文的缓存译文仍会被命中，用户配置
/// 的术语上下文被静默忽略。现在这些维度全部进键：
///
/// - `src=<源语言>`：仅显式源语言进入。自动检测对同一段文本是确定性的，
///   编入只会白白分裂缓存，不编。
/// - `gl=<指纹>`：DeepL 术语表指纹（同步成功才会出现）。
/// - `bd=1` / `yd=<词汇表ID>` / `tc=<术语库ID列表>`：控制台术语库引用。
///
/// 所有维度都不存在时令牌为空串，键退回「文本 + 目标语言」的旧形状 ——
/// 既有缓存条目不失效。写入与查询必须用同一把钥匙（同一请求上下文
/// 构造同一个令牌），这是跨上下文不串译的关键。
pub fn scope_token(
    source_lang: Option<&str>,
    glossary_fingerprint: &str,
    baidu_use_term_base: bool,
    youdao_vocab_id: Option<&str>,
    tencent_term_repo_ids: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(src) = source_lang {
        parts.push(format!("src={}", src));
    }
    if !glossary_fingerprint.is_empty() {
        parts.push(format!("gl={}", glossary_fingerprint));
    }
    if baidu_use_term_base {
        parts.push("bd=1".to_string());
    }
    if let Some(vocab) = youdao_vocab_id {
        parts.push(format!("yd={}", vocab));
    }
    if !tencent_term_repo_ids.is_empty() {
        parts.push(format!("tc={}", tencent_term_repo_ids.join(";")));
    }
    parts.join(",")
}

/// 查缓存并组装成一次正常的翻译结果。
///
/// 回声条目过滤：缓存里可能存有回声守卫落地前的存量 `(A → A)` 条目，
/// 离线回退把它们当译文端出去就是「翻译结果是原文」。读取侧统一拒掉，
/// 存量数据无需迁移即失效。
///
/// 缓存读失败返回 None（记 warn）：缓存是最后一根稻草，不该把主链路
/// 带下水 —— 回退路径上没有别的兜底了。
pub async fn lookup_cached_result(
    cache: &TranslationCache,
    text: &str,
    target_lang: &str,
    scope_token: &str,
    truncated: bool,
) -> Option<crate::types::TranslationResult> {
    match cache.get(text, target_lang, scope_token).await {
        Ok(Some(hit)) => {
            if crate::domain::translator::is_echo_text(text, &hit.translated_text) {
                tracing::info!(
                    event = "cache_entry_rejected_as_echo",
                    "[cache] 缓存命中了与原文相同的内容，按未命中处理"
                );
                return None;
            }
            Some(crate::types::TranslationResult {
                source_text: text.to_string(),
                translated_text: hit.translated_text,
                detected_source_lang: hit.source_lang,
                target_lang: target_lang.to_string(),
                provider: CACHE_PROVIDER.to_string(),
                duration_ms: 0,
                truncated,
            })
        }
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("[cache] 读取本地缓存失败: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::database::CACHE_TABLE_SQL;

    /// 用与真实迁移同一份 DDL 建表 —— 测试里的表结构不可能与线上漂移
    fn test_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CACHE_TABLE_SQL).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn fast() -> TranslationCache {
        TranslationCache::with_limits(
            crate::infra::database::Db::available(test_db()),
            3,
            Duration::from_secs(60),
        )
    }

    /// 直接写一行并可指定 `last_hit_at`。
    ///
    /// LRU 用例必须用它：`now_unix_ms()` 只有毫秒精度，测试里连续几次调用会
    /// 撞在同一毫秒，`ORDER BY last_hit_at` 的次序就成了偶然。
    /// （真实使用中操作间隔是秒级，不存在这个问题。）
    async fn insert_raw(db: &Arc<Mutex<Connection>>, text: &str, last_hit_at: i64) {
        let conn = db.lock().await;
        conn.execute(
            "INSERT INTO translation_cache
                 (cache_key, source_text, translated_text, source_lang, provider,
                  created_at, last_hit_at, hit_count)
             VALUES (?1, ?2, '译文', 'en', 'deepl', ?3, ?4, 0)",
            rusqlite::params![format!("{}\u{1}zh", text), text, now_unix_ms(), last_hit_at],
        )
        .unwrap();
    }

    /// 写入便捷函数：多数用例只关心文本/目标语言/译文三件事，
    /// 源语言固定成 "en" 免得每处都写一遍。
    async fn put(c: &TranslationCache, text: &str, target: &str, translated: &str, provider: &str) {
        c.put(CacheEntry {
            source_text: text,
            target_lang: target,
            translated_text: translated,
            source_lang: "en",
            provider,
            scope_token: "",
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn put_then_get_roundtrip() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;

        let hit = c.get("hello", "zh", "").await.unwrap().expect("应命中");
        assert_eq!(hit.translated_text, "你好");
        assert_eq!(hit.provider, "deepl");
    }

    /// 清空只删缓存：条目归零、计数如实返回，且之后再写仍正常（表未被破坏）。
    #[tokio::test]
    async fn clear_and_count_empties_only_the_cache() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;
        put(&c, "world", "zh", "世界", "deepl").await;

        assert_eq!(c.clear_and_count().await.unwrap(), 2);
        assert_eq!(c.len().await, 0, "清空后条目必须归零");
        assert_eq!(c.clear_and_count().await.unwrap(), 0, "重复清空返回 0");

        // 表仍然可用：清空后的写入/读取照常
        put(&c, "hello", "zh", "你好", "deepl").await;
        assert!(c.get("hello", "zh", "").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let c = fast();
        assert!(c.get("never seen", "zh", "").await.unwrap().is_none());
    }

    /// 同一个源文本、不同目标语言必须是两条不同的缓存
    #[tokio::test]
    async fn key_includes_the_target_language() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;
        put(&c, "hello", "ja", "こんにちは", "deepl").await;

        assert_eq!(
            c.get("hello", "zh", "")
                .await
                .unwrap()
                .unwrap()
                .translated_text,
            "你好"
        );
        assert_eq!(
            c.get("hello", "ja", "")
                .await
                .unwrap()
                .unwrap()
                .translated_text,
            "こんにちは"
        );
        assert!(c.get("hello", "fr", "").await.unwrap().is_none());
    }

    /// 拼接歧义防护：("ab","c") 与 ("a","bc") 不能撞成同一个键
    #[tokio::test]
    async fn key_separator_prevents_collisions() {
        let c = fast();
        put(&c, "ab", "c", "first", "deepl").await;
        put(&c, "a", "bc", "second", "deepl").await;

        assert_eq!(
            c.get("ab", "c", "").await.unwrap().unwrap().translated_text,
            "first"
        );
        assert_eq!(
            c.get("a", "bc", "").await.unwrap().unwrap().translated_text,
            "second"
        );
    }

    // ── 大小限制 ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn oversized_text_is_never_cached() {
        let c = fast();
        let long = "a".repeat(MAX_CACHED_TEXT_CHARS + 1);

        put(&c, &long, "zh", "译文", "deepl").await;

        assert!(c.get(&long, "zh", "").await.unwrap().is_none());
        assert_eq!(c.len().await, 0, "超长文本不该落盘");
    }

    #[tokio::test]
    async fn text_at_the_limit_is_cached() {
        let c = fast();
        let exact = "a".repeat(MAX_CACHED_TEXT_CHARS);
        assert!(TranslationCache::is_cacheable(&exact));
        put(&c, &exact, "zh", "译文", "deepl").await;
        assert!(c.get(&exact, "zh", "").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn empty_text_is_never_cached() {
        let c = fast();
        put(&c, "", "zh", "译文", "deepl").await;
        assert_eq!(c.len().await, 0);
        assert!(c.get("", "zh", "").await.unwrap().is_none());
    }

    // ── 过期 ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn expired_entries_are_not_served_and_get_cleaned_up() {
        let db = test_db();
        let c = TranslationCache::with_limits(
            crate::infra::database::Db::available(db.clone()),
            100,
            Duration::from_secs(60),
        );

        // 直接写一条 2 分钟前的记录（超过 60s 的 TTL）
        {
            let conn = db.lock().await;
            conn.execute(
                "INSERT INTO translation_cache
                     (cache_key, source_text, translated_text, provider,
                      created_at, last_hit_at, hit_count)
                 VALUES ('stale\u{1}zh', 'stale', '过期的译文', 'deepl', ?1, ?1, 0)",
                rusqlite::params![now_unix_ms() - 120_000],
            )
            .unwrap();
        }

        assert!(
            c.get("stale", "zh", "").await.unwrap().is_none(),
            "过期条目不该被返回"
        );

        c.enforce_limits().await.unwrap();
        assert_eq!(c.len().await, 0, "enforce_limits 应清掉过期条目");
    }

    // ── LRU 淘汰 ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn lru_eviction_drops_the_least_recently_hit() {
        let db = test_db();
        let c = TranslationCache::with_limits(
            crate::infra::database::Db::available(db.clone()),
            3,
            TTL,
        ); // 上限 3 条
        let base = now_unix_ms();

        insert_raw(&db, "coldest", base).await;
        insert_raw(&db, "middle", base + 10).await;
        insert_raw(&db, "hottest", base + 20).await;

        // 第 4 条 → 超出上限，触发 LRU 淘汰
        insert_raw(&db, "extra", base + 30).await;
        c.enforce_limits().await.unwrap();

        assert_eq!(c.len().await, 3);
        assert!(
            c.get("coldest", "zh", "").await.unwrap().is_none(),
            "最久未命中的条目应被淘汰"
        );
        for t in ["middle", "hottest", "extra"] {
            assert!(
                c.get(t, "zh", "").await.unwrap().is_some(),
                "{} 应被保留",
                t
            );
        }
    }

    /// 命中会刷新热度，从而把条目从淘汰边缘拉回来
    #[tokio::test]
    async fn a_hit_refreshes_recency_and_hit_count() {
        let db = test_db();
        let c = TranslationCache::with_limits(
            crate::infra::database::Db::available(db.clone()),
            100,
            TTL,
        );
        let base = now_unix_ms();
        insert_raw(&db, "x", base).await;

        assert!(c.get("x", "zh", "").await.unwrap().is_some());

        let (last_hit, count): (i64, i64) = {
            let conn = db.lock().await;
            conn.query_row(
                "SELECT last_hit_at, hit_count FROM translation_cache WHERE cache_key = ?1",
                rusqlite::params!["x\u{1}zh"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert!(
            last_hit >= base,
            "命中必须刷新 last_hit_at，否则它会被当成冷门淘汰"
        );
        assert_eq!(count, 1, "命中必须累计 hit_count");
    }

    #[tokio::test]
    async fn enforce_limits_is_a_noop_below_the_cap() {
        let c = fast();
        put(&c, "a", "zh", "A", "deepl").await;
        put(&c, "b", "zh", "B", "deepl").await;
        c.enforce_limits().await.unwrap();
        assert_eq!(c.len().await, 2);
    }

    // ── 重复写入 ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn rewriting_updates_the_translation_but_keeps_the_hit_count() {
        let db = test_db();
        let c = TranslationCache::with_limits(
            crate::infra::database::Db::available(db.clone()),
            100,
            TTL,
        );

        put(&c, "hello", "zh", "旧译文", "deepl").await;
        assert!(c.get("hello", "zh", "").await.unwrap().is_some()); // hit_count → 1

        put(&c, "hello", "zh", "新译文", "tencent").await;

        let hit = c.get("hello", "zh", "").await.unwrap().unwrap(); // hit_count → 2
        assert_eq!(hit.translated_text, "新译文", "译文应被更新");
        assert_eq!(hit.provider, "tencent");

        let count: i64 = {
            let conn = db.lock().await;
            conn.query_row(
                "SELECT hit_count FROM translation_cache WHERE cache_key = 'hello\u{1}zh'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        // 两次 get 累计到 2。若 put 把 hit_count 重置为 0，这里会读到 1。
        assert_eq!(
            count, 2,
            "重复写入不该重置热度记录 —— 否则常翻的内容会因为被重新翻译过而被当成冷门淘汰"
        );
    }

    // ── 离线回退组装（coordinator 与手动翻译路径共用）────────────────────

    #[tokio::test]
    async fn cached_hit_is_assembled_as_a_result_from_the_cache_provider() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;

        let r = lookup_cached_result(&c, "hello", "zh", "", false)
            .await
            .expect("有缓存条目必须组装出结果");
        assert_eq!(r.provider, CACHE_PROVIDER);
        assert_eq!(r.translated_text, "你好");
        assert_eq!(r.source_text, "hello");
        assert_eq!(r.target_lang, "zh");
        assert_eq!(r.duration_ms, 0, "缓存返回没有网络耗时");
        assert!(!r.truncated);
    }

    #[tokio::test]
    async fn truncated_flag_is_passed_through_to_the_cached_result() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;
        assert!(
            lookup_cached_result(&c, "hello", "zh", "", true)
                .await
                .unwrap()
                .truncated
        );
    }

    /// 存量回声条目（回声守卫落地前写入的 A → A）在回退读取侧被拒收。
    #[tokio::test]
    async fn an_echo_entry_in_the_cache_is_rejected_on_fallback() {
        let c = fast();
        put(&c, "password", "zh", "password", "deepl").await;
        assert!(
            lookup_cached_result(&c, "password", "zh", "", false)
                .await
                .is_none(),
            "译文与原文相同的缓存条目端出去就是「翻译结果是原文」"
        );
    }

    #[tokio::test]
    async fn cache_miss_yields_none_on_fallback() {
        let c = fast();
        assert!(lookup_cached_result(&c, "never seen", "zh", "", false)
            .await
            .is_none());
    }

    /// 作用域令牌进入缓存键：术语表/术语库/源语言任一维度变化后，
    /// 旧上下文的译文不会被新上下文命中；空令牌与引入作用域之前的键一致
    /// （既有缓存不失效）。
    #[tokio::test]
    async fn scope_token_separates_cache_entries() {
        let c = fast();
        c.put(CacheEntry {
            source_text: "Apple",
            target_lang: "zh",
            translated_text: "苹果",
            source_lang: "en",
            provider: "deepl",
            scope_token: "gl=fp1",
        })
        .await
        .unwrap();

        assert!(c.get("Apple", "zh", "gl=fp1").await.unwrap().is_some());
        assert!(
            c.get("Apple", "zh", "gl=fp2").await.unwrap().is_none(),
            "术语表变化后不得命中旧译文"
        );
        assert!(
            c.get("Apple", "zh", "").await.unwrap().is_none(),
            "未应用术语表的请求不得命中术语表译文"
        );

        // 无令牌条目照旧可用
        put(&c, "hello", "zh", "你好", "deepl").await;
        assert!(c.get("hello", "zh", "").await.unwrap().is_some());
    }

    /// 令牌构造：各维度按固定顺序拼接，全部缺省时为空串（退回旧键形状）。
    #[test]
    fn scope_token_orders_all_context_dimensions() {
        assert_eq!(scope_token(None, "", false, None, &[]), "");
        assert_eq!(
            scope_token(
                Some("en"),
                "fp1",
                true,
                Some("v1"),
                &["r1".to_string(), "r2".to_string()]
            ),
            "src=en,gl=fp1,bd=1,yd=v1,tc=r1;r2"
        );
    }

    /// 评审整改回归：控制台术语库上下文进键后，开关切换不会串用旧上下文
    /// 的缓存译文。
    #[tokio::test]
    async fn term_context_change_never_serves_old_scope() {
        let c = fast();
        let with_terms = scope_token(None, "", true, None, &[]);
        c.put(CacheEntry {
            source_text: "Apple",
            target_lang: "zh",
            translated_text: "苹果",
            source_lang: "en",
            provider: "baidu",
            scope_token: &with_terms,
        })
        .await
        .unwrap();

        assert!(
            c.get("Apple", "zh", "").await.unwrap().is_none(),
            "术语库关闭后的请求不得命中带术语上下文的条目"
        );
        assert!(
            c.get("Apple", "zh", &with_terms).await.unwrap().is_some(),
            "同一上下文内照常命中"
        );
    }

    // ── 读写开关（配置键 cache_enabled）──────────────────────────────────

    /// 关闭是「暂停」不是「销毁」：不读不写，但已有条目原样保留，
    /// 重新打开后仍然可用。
    #[tokio::test]
    async fn disabling_pauses_reads_and_writes_without_deleting_entries() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;

        c.set_enabled(false);
        assert!(!c.is_enabled());
        assert!(
            c.get("hello", "zh", "").await.unwrap().is_none(),
            "关闭后不得读取缓存"
        );
        put(&c, "world", "zh", "世界", "deepl").await;
        assert_eq!(c.len().await, 1, "关闭后不得写入新条目");

        c.set_enabled(true);
        assert!(
            c.get("hello", "zh", "").await.unwrap().is_some(),
            "重新打开后既有条目必须恢复可用"
        );
    }

    /// 关闭期间「清空缓存」仍然有效 —— 隐私操作不能因为开关状态被挡住。
    #[tokio::test]
    async fn clearing_works_while_disabled() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;
        c.set_enabled(false);
        assert_eq!(c.clear_and_count().await.unwrap(), 1);
        assert_eq!(c.len().await, 0);
    }

    /// 关闭时离线回退不得把缓存端出去。
    #[tokio::test]
    async fn fallback_lookup_respects_the_disable_switch() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;
        c.set_enabled(false);
        assert!(lookup_cached_result(&c, "hello", "zh", "", false)
            .await
            .is_none());
    }
}
