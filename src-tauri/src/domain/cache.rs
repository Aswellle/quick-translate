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
// 只在所有翻译源都用不了时才查缓存（见 coordinator），因此在线时结果永远
// 是新鲜的 —— 缓存不会让用户看到过期译文。

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
}

pub struct TranslationCache {
    db: Arc<Mutex<Connection>>,
    max_entries: i64,
    ttl: Duration,
}

impl TranslationCache {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self {
            db,
            max_entries: MAX_ENTRIES,
            ttl: TTL,
        }
    }

    /// 测试用：可调上限与 TTL（否则验证淘汰与过期得等 30 天）
    #[cfg(test)]
    pub fn with_limits(db: Arc<Mutex<Connection>>, max_entries: i64, ttl: Duration) -> Self {
        Self {
            db,
            max_entries,
            ttl,
        }
    }

    /// 缓存键：归一化文本 + 目标语言。
    ///
    /// 用 `\u{1}` 分隔而不是直接拼接：否则 "ab"+"c" 与 "a"+"bc" 会撞成同一个键。
    /// 分隔符取控制字符，正常文本里不可能出现。
    fn key(text: &str, target_lang: &str) -> String {
        format!("{}\u{1}{}", text, target_lang)
    }

    /// 该文本是否值得缓存
    pub fn is_cacheable(text: &str) -> bool {
        let n = text.chars().count();
        n > 0 && n <= MAX_CACHED_TEXT_CHARS
    }

    /// 精确命中。过期的条目不返回（但留到 enforce_limits 再清理，
    /// 免得每次读都写一次库）。
    pub async fn get(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<Option<CachedTranslation>, AppError> {
        if !Self::is_cacheable(text) {
            return Ok(None);
        }

        let key = Self::key(text, target_lang);
        let cutoff = now_unix_ms() - self.ttl.as_millis() as i64;
        let conn = self.db.lock().await;

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
    pub async fn put(&self, entry: CacheEntry<'_>) -> Result<(), AppError> {
        if !Self::is_cacheable(entry.source_text) {
            return Ok(());
        }

        let now = now_unix_ms();
        let conn = self.db.lock().await;
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
                Self::key(entry.source_text, entry.target_lang),
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
    pub async fn enforce_limits(&self) -> Result<(), AppError> {
        let conn = self.db.lock().await;

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

    /// 测试用：当前条目数
    #[cfg(test)]
    pub async fn len(&self) -> i64 {
        let conn = self.db.lock().await;
        conn.query_row("SELECT COUNT(*) FROM translation_cache", [], |row| {
            row.get(0)
        })
        .unwrap()
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
        TranslationCache::with_limits(test_db(), 3, Duration::from_secs(60))
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
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn put_then_get_roundtrip() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;

        let hit = c.get("hello", "zh").await.unwrap().expect("应命中");
        assert_eq!(hit.translated_text, "你好");
        assert_eq!(hit.provider, "deepl");
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let c = fast();
        assert!(c.get("never seen", "zh").await.unwrap().is_none());
    }

    /// 同一个源文本、不同目标语言必须是两条不同的缓存
    #[tokio::test]
    async fn key_includes_the_target_language() {
        let c = fast();
        put(&c, "hello", "zh", "你好", "deepl").await;
        put(&c, "hello", "ja", "こんにちは", "deepl").await;

        assert_eq!(
            c.get("hello", "zh").await.unwrap().unwrap().translated_text,
            "你好"
        );
        assert_eq!(
            c.get("hello", "ja").await.unwrap().unwrap().translated_text,
            "こんにちは"
        );
        assert!(c.get("hello", "fr").await.unwrap().is_none());
    }

    /// 拼接歧义防护：("ab","c") 与 ("a","bc") 不能撞成同一个键
    #[tokio::test]
    async fn key_separator_prevents_collisions() {
        let c = fast();
        put(&c, "ab", "c", "first", "deepl").await;
        put(&c, "a", "bc", "second", "deepl").await;

        assert_eq!(
            c.get("ab", "c").await.unwrap().unwrap().translated_text,
            "first"
        );
        assert_eq!(
            c.get("a", "bc").await.unwrap().unwrap().translated_text,
            "second"
        );
    }

    // ── 大小限制 ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn oversized_text_is_never_cached() {
        let c = fast();
        let long = "a".repeat(MAX_CACHED_TEXT_CHARS + 1);

        put(&c, &long, "zh", "译文", "deepl").await;

        assert!(c.get(&long, "zh").await.unwrap().is_none());
        assert_eq!(c.len().await, 0, "超长文本不该落盘");
    }

    #[tokio::test]
    async fn text_at_the_limit_is_cached() {
        let c = fast();
        let exact = "a".repeat(MAX_CACHED_TEXT_CHARS);
        assert!(TranslationCache::is_cacheable(&exact));
        put(&c, &exact, "zh", "译文", "deepl").await;
        assert!(c.get(&exact, "zh").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn empty_text_is_never_cached() {
        let c = fast();
        put(&c, "", "zh", "译文", "deepl").await;
        assert_eq!(c.len().await, 0);
        assert!(c.get("", "zh").await.unwrap().is_none());
    }

    // ── 过期 ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn expired_entries_are_not_served_and_get_cleaned_up() {
        let db = test_db();
        let c = TranslationCache::with_limits(db.clone(), 100, Duration::from_secs(60));

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
            c.get("stale", "zh").await.unwrap().is_none(),
            "过期条目不该被返回"
        );

        c.enforce_limits().await.unwrap();
        assert_eq!(c.len().await, 0, "enforce_limits 应清掉过期条目");
    }

    // ── LRU 淘汰 ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn lru_eviction_drops_the_least_recently_hit() {
        let db = test_db();
        let c = TranslationCache::with_limits(db.clone(), 3, TTL); // 上限 3 条
        let base = now_unix_ms();

        insert_raw(&db, "coldest", base).await;
        insert_raw(&db, "middle", base + 10).await;
        insert_raw(&db, "hottest", base + 20).await;

        // 第 4 条 → 超出上限，触发 LRU 淘汰
        insert_raw(&db, "extra", base + 30).await;
        c.enforce_limits().await.unwrap();

        assert_eq!(c.len().await, 3);
        assert!(
            c.get("coldest", "zh").await.unwrap().is_none(),
            "最久未命中的条目应被淘汰"
        );
        for t in ["middle", "hottest", "extra"] {
            assert!(c.get(t, "zh").await.unwrap().is_some(), "{} 应被保留", t);
        }
    }

    /// 命中会刷新热度，从而把条目从淘汰边缘拉回来
    #[tokio::test]
    async fn a_hit_refreshes_recency_and_hit_count() {
        let db = test_db();
        let c = TranslationCache::with_limits(db.clone(), 100, TTL);
        let base = now_unix_ms();
        insert_raw(&db, "x", base).await;

        assert!(c.get("x", "zh").await.unwrap().is_some());

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
        let c = TranslationCache::with_limits(db.clone(), 100, TTL);

        put(&c, "hello", "zh", "旧译文", "deepl").await;
        assert!(c.get("hello", "zh").await.unwrap().is_some()); // hit_count → 1

        put(&c, "hello", "zh", "新译文", "tencent").await;

        let hit = c.get("hello", "zh").await.unwrap().unwrap(); // hit_count → 2
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
}
