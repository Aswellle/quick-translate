// src-tauri/src/domain/history.rs
// 翻译历史仓库：CRUD、FIFO 超限清理、LIKE 子串搜索（FTS5 已随迁移 v3 移除）

use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::domain::config::{HISTORY_LIMIT_MAX, HISTORY_LIMIT_MIN};
use crate::error::AppError;
use crate::infra::database::Db;
use crate::types::{HistoryQuery, StatsResult, TranslationRecord};
pub struct HistoryRepository {
    db: Db,
}

impl HistoryRepository {
    pub fn new(db: Db) -> Self {
        HistoryRepository { db }
    }

    /// 存储不可用时返回 `StorageUnavailable`：历史是辅助能力（计划第 25 节），
    /// 这个错误绝不能出现在翻译主链路上。
    fn conn(&self) -> Result<Arc<Mutex<Connection>>, AppError> {
        self.db.try_conn().ok_or(AppError::StorageUnavailable)
    }

    /// 插入翻译记录（FTS 通过 trigger 自动同步）
    pub async fn insert(&self, record: &TranslationRecord) -> Result<(), AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;
        conn.execute(
            r#"INSERT INTO translation_records
               (id, source_text, translated_text, source_lang, target_lang,
                provider, created_at, duration_ms, is_starred)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            rusqlite::params![
                record.id,
                record.source_text,
                record.translated_text,
                record.source_lang,
                record.target_lang,
                record.provider,
                record.created_at,
                record.duration_ms,
                record.is_starred as i64,
            ],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 查询历史记录（支持 LIKE 子串搜索 + 分页 + starred_only 过滤）
    pub async fn query(&self, params: &HistoryQuery) -> Result<Vec<TranslationRecord>, AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;

        let starred_only = params.starred_only.unwrap_or(false);
        // limit/offset 来自前端，直接绑定 SQL：SQLite 里 LIMIT -1 等于
        // 不限制（一次拖回全部行），负 offset 也会被无声接受 —— 与
        // enforce_limit 的钳制口径对齐，在这里统一收口。
        let limit = params.limit.clamp(1, 1000);
        let offset = params.offset.max(0);

        let records = if let Some(search) = &params.search {
            if search.trim().is_empty() {
                query_all(&conn, limit, offset, starred_only)?
            } else {
                query_like(&conn, search, limit, offset, starred_only)?
            }
        } else {
            query_all(&conn, limit, offset, starred_only)?
        };

        Ok(records)
    }

    /// 获取记录总数（用于前端分页）
    pub async fn count(&self, search: Option<&str>, starred_only: bool) -> Result<i64, AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;

        let count = if let Some(keyword) = search {
            if keyword.trim().is_empty() {
                count_all(&conn, starred_only)?
            } else {
                count_like(&conn, keyword, starred_only)?
            }
        } else {
            count_all(&conn, starred_only)?
        };

        Ok(count)
    }

    /// 切换收藏状态，返回新的收藏状态
    pub async fn toggle_star(&self, id: &str) -> Result<bool, AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE translation_records SET is_starred = CASE WHEN is_starred = 1 THEN 0 ELSE 1 END WHERE id = ?1",
            [id],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let new_value: i64 = conn
            .query_row(
                "SELECT is_starred FROM translation_records WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(new_value != 0)
    }

    /// 导出所有历史记录（无分页）
    pub async fn export_all(&self) -> Result<Vec<TranslationRecord>, AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare(
                r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                          provider, created_at, duration_ms, is_starred
                   FROM translation_records
                   ORDER BY created_at DESC, rowid DESC"#,
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let records = stmt
            .query_map([], map_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(records)
    }

    /// 获取使用统计（近 `days` 天的逐日趋势，1..=90，默认 7）。
    pub async fn get_stats(&self, days: Option<u32>) -> Result<StatsResult, AppError> {
        let days = days.unwrap_or(7).clamp(1, STATS_MAX_DAYS);
        let now_ms = crate::types::now_unix_ms();
        let today = local_midnight_ms(now_ms);
        let range_start = today - (days as i64 - 1) * MS_PER_DAY;
        // 逐日区间要多覆盖一天才能包含「今天」（区间左闭右开）
        self.stats_impl(range_start, today + MS_PER_DAY).await
    }

    /// 获取使用统计（自定义区间，两端都含其所在的本地日）。
    pub async fn get_stats_range(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<StatsResult, AppError> {
        if end_ms < start_ms {
            return Err(AppError::ConfigError("统计区间的结束早于开始".to_string()));
        }
        let range_start = local_midnight_ms(start_ms);
        let range_end = local_midnight_ms(end_ms) + MS_PER_DAY;
        // 区间上限：92 天 ≈ 一个季度。趋势图逐日查询，无界的自定义区间
        // 会退化成上百次 COUNT —— 截断比拖垮数据库好
        if (range_end - range_start) / MS_PER_DAY > STATS_MAX_DAYS as i64 {
            return Err(AppError::ConfigError(format!(
                "统计区间过长，最多 {} 天",
                STATS_MAX_DAYS
            )));
        }
        self.stats_impl(range_start, range_end).await
    }

    /// 统计聚合本体：汇总卡为全量口径，逐日趋势覆盖 [range_start, range_end)。
    async fn stats_impl(&self, range_start: i64, range_end: i64) -> Result<StatsResult, AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;

        let total_records: u64 = conn
            .query_row("SELECT COUNT(*) FROM translation_records", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|n| n as u64)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let total_chars: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(source_text)), 0) FROM translation_records",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let now_ms = crate::types::now_unix_ms();
        let ms_7_days: i64 = 7 * MS_PER_DAY;
        let ms_30_days: i64 = 30 * MS_PER_DAY;
        let ms_24h: i64 = MS_PER_DAY;

        let last_7_days: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM translation_records WHERE created_at >= ?1",
                rusqlite::params![now_ms - ms_7_days],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let last_30_days: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM translation_records WHERE created_at >= ?1",
                rusqlite::params![now_ms - ms_30_days],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let last_24h: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM translation_records WHERE created_at >= ?1",
                rusqlite::params![now_ms - ms_24h],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        // 逐日计数（本地时区日界）：SQL 只按毫秒区间过滤，日归属在 Rust 侧做
        // —— SQLite 的日期函数走 UTC，本地时区的「今天」与其不一致，混用
        // 会让趋势图整体错位数小时。
        let mut daily: Vec<crate::types::DailyCount> =
            Vec::with_capacity(((range_end - range_start) / MS_PER_DAY).max(0) as usize);
        let mut day_start = range_start;
        while day_start < range_end {
            let day_end = day_start + MS_PER_DAY;
            // 次数与字符量一次查询同时取回（悬浮提示需要字符明细）
            let (count, chars): (i64, i64) = conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(LENGTH(source_text)), 0)
                     FROM translation_records WHERE created_at >= ?1 AND created_at < ?2",
                    rusqlite::params![day_start, day_end],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            daily.push(crate::types::DailyCount {
                date_ms: day_start,
                count: count as u64,
                chars: chars as u64,
            });
            day_start = day_end;
        }

        let mut by_provider: HashMap<String, u64> = HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT provider, COUNT(*) as cnt FROM translation_records GROUP BY provider",
                )
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;

            for row in rows {
                let (provider, count) = row.map_err(|e| AppError::DatabaseError(e.to_string()))?;
                by_provider.insert(provider, count as u64);
            }
        }

        Ok(StatsResult {
            total_records,
            total_chars,
            by_provider,
            last_7_days,
            last_30_days,
            last_24h,
            daily,
        })
    }

    /// FIFO 超限清理：删除最旧的记录，使总数不超过 limit
    pub async fn enforce_limit(&self, limit: i64) -> Result<u64, AppError> {
        // 防御性钳制：确保 limit 在有效范围内，避免非正数 limit 删除全部记录
        let limit = limit.clamp(HISTORY_LIMIT_MIN, HISTORY_LIMIT_MAX);
        let conn = self.conn()?;
        let conn = conn.lock().await;
        let total = count_all(&conn, false)?;

        if total <= limit {
            return Ok(0);
        }

        let to_delete = total - limit;
        let deleted = conn
            .execute(
                r#"DELETE FROM translation_records
                   WHERE id IN (
                       SELECT id FROM translation_records
                       WHERE is_starred = 0
                       ORDER BY created_at ASC, rowid ASC
                       LIMIT ?1
                   )"#,
                rusqlite::params![to_delete],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::debug!("FIFO 清理：删除了 {} 条旧记录", deleted);
        Ok(deleted as u64)
    }

    /// 删除单条历史记录（FTS 通过 trg_records_ad trigger 自动同步）
    pub async fn delete_by_id(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;
        conn.execute("DELETE FROM translation_records WHERE id = ?1", [id])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// 清空所有历史记录
    pub async fn clear_all(&self) -> Result<(), AppError> {
        let conn = self.conn()?;
        let conn = conn.lock().await;
        conn.execute("DELETE FROM translation_records", [])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}

// ---- 内部查询辅助函数 ----

/// 一天的毫秒数（逐日趋势与统计窗口共用）
const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// 逐日趋势允许的最大天数（get_stats 的 days 上限 / 自定义区间的跨度上限）
const STATS_MAX_DAYS: u32 = 90;

/// 计算给定 Unix 毫秒时刻所在「本地时区当天 0 点」的 Unix 毫秒。
///
/// 用 `time` crate 的本地偏移（已在依赖树中，Windows 走 WinAPI /
/// macOS 走 localtime_r，比手搓 libc 声明可靠）。DST 切换日一天可能
/// 是 23/25 小时，7 天窗口的日边界因此整体平移 ≤1h，对趋势图无感知。
fn local_midnight_ms(now_ms: i64) -> i64 {
    use time::OffsetDateTime;
    let now = OffsetDateTime::from_unix_timestamp(now_ms.div_euclid(1000))
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    // to_offset(UtcOffset) 失败（拿不到本地时区）时按 UTC 兜底：
    // 误差只是时区偏移，趋势图仍然成立
    let local =
        now.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
    let day_start = local.replace_time(time::Time::MIDNIGHT);
    day_start.unix_timestamp() * 1000
}

fn query_all(
    conn: &Connection,
    limit: i64,
    offset: i64,
    starred_only: bool,
) -> Result<Vec<TranslationRecord>, AppError> {
    let sql = if starred_only {
        r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                  provider, created_at, duration_ms, is_starred
           FROM translation_records
           WHERE is_starred = 1
           ORDER BY created_at DESC, rowid DESC
           LIMIT ?1 OFFSET ?2"#
    } else {
        r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                  provider, created_at, duration_ms, is_starred
           FROM translation_records
           ORDER BY created_at DESC, rowid DESC
           LIMIT ?1 OFFSET ?2"#
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let records = stmt
        .query_map(rusqlite::params![limit, offset], map_row)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(records)
}

/// LIKE 子串搜索（支持中文 / CJK 及任意字符，不依赖 FTS5 分词）
fn query_like(
    conn: &Connection,
    search: &str,
    limit: i64,
    offset: i64,
    starred_only: bool,
) -> Result<Vec<TranslationRecord>, AppError> {
    let pattern = format!(
        "%{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );

    let sql = if starred_only {
        r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                  provider, created_at, duration_ms, is_starred
           FROM translation_records
           WHERE is_starred = 1
             AND (source_text LIKE ?1 ESCAPE '\' OR translated_text LIKE ?1 ESCAPE '\')
           ORDER BY created_at DESC, rowid DESC
           LIMIT ?2 OFFSET ?3"#
    } else {
        r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                  provider, created_at, duration_ms, is_starred
           FROM translation_records
           WHERE source_text LIKE ?1 ESCAPE '\' OR translated_text LIKE ?1 ESCAPE '\'
           ORDER BY created_at DESC, rowid DESC
           LIMIT ?2 OFFSET ?3"#
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let records = stmt
        .query_map(rusqlite::params![pattern, limit, offset], map_row)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(records)
}

fn count_all(conn: &Connection, starred_only: bool) -> Result<i64, AppError> {
    if starred_only {
        conn.query_row(
            "SELECT COUNT(*) FROM translation_records WHERE is_starred = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))
    } else {
        conn.query_row("SELECT COUNT(*) FROM translation_records", [], |row| {
            row.get(0)
        })
        .map_err(|e| AppError::DatabaseError(e.to_string()))
    }
}

fn count_like(conn: &Connection, search: &str, starred_only: bool) -> Result<i64, AppError> {
    let pattern = format!(
        "%{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );

    if starred_only {
        conn.query_row(
            r#"SELECT COUNT(*) FROM translation_records
               WHERE is_starred = 1
                 AND (source_text LIKE ?1 ESCAPE '\' OR translated_text LIKE ?1 ESCAPE '\')"#,
            rusqlite::params![pattern],
            |row| row.get(0),
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))
    } else {
        conn.query_row(
            r#"SELECT COUNT(*) FROM translation_records
               WHERE source_text LIKE ?1 ESCAPE '\' OR translated_text LIKE ?1 ESCAPE '\'"#,
            rusqlite::params![pattern],
            |row| row.get(0),
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranslationRecord> {
    Ok(TranslationRecord {
        id: row.get(0)?,
        source_text: row.get(1)?,
        translated_text: row.get(2)?,
        source_lang: row.get(3)?,
        target_lang: row.get(4)?,
        provider: row.get(5)?,
        created_at: row.get(6)?,
        duration_ms: row.get(7)?,
        is_starred: row.get::<_, i64>(8).map(|v| v != 0)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn repo() -> HistoryRepository {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::infra::database::run_migrations(&conn).unwrap();
        HistoryRepository::new(crate::infra::database::Db::available(Arc::new(Mutex::new(
            conn,
        ))))
    }

    fn record_at(created_at: i64) -> TranslationRecord {
        TranslationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            source_text: "hello".to_string(),
            translated_text: "你好".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            provider: "google".to_string(),
            created_at,
            duration_ms: Some(100),
            is_starred: false,
        }
    }

    /// 第 k 天前（k=0 是今天）本地正午的毫秒时刻：正午离两个日界都远，
    /// DST 平移不会把种子记录挤到邻天
    fn noon_days_ago(k: i64) -> i64 {
        local_midnight_ms(crate::types::now_unix_ms()) - k * MS_PER_DAY + 12 * 60 * 60 * 1000
    }

    // ── 分页稳定性（计划 6.7：时间相同的记录必须有第二排序键）──────────

    /// 同一毫秒内连续落盘的记录，分页必须无重复、无遗漏，且顺序可重复。
    ///
    /// 缺陷形态：`ORDER BY created_at DESC` 只有单一排序键，SQLite 对并列
    /// 行的相对顺序不作保证 —— 分页时同一记录可能出现在两页里，也可能
    /// 被跳过。第二键 `rowid DESC` 让「后落盘的排在前面」，与 created_at
    /// 的降序语义一致，且 rowid 单调递增、查询稳定。
    #[tokio::test]
    async fn pagination_is_stable_when_timestamps_collide() {
        let repo = repo();
        let ts = 1_700_000_000_000;
        // 6 条完全相同的 created_at，插入顺序即期望的「新→旧」顺序
        let mut ids = Vec::new();
        for _ in 0..6 {
            let rec = record_at(ts);
            ids.push(rec.id.clone());
            repo.insert(&rec).await.unwrap();
        }
        let expected: Vec<String> = ids.iter().rev().cloned().collect();

        // 逐页读取（limit=2），合并三页
        let mut paged = Vec::new();
        for offset in [0, 2, 4] {
            let page = repo
                .query(&HistoryQuery {
                    limit: 2,
                    offset,
                    ..Default::default()
                })
                .await
                .unwrap();
            paged.extend(page.into_iter().map(|r| r.id));
        }

        assert_eq!(
            paged, expected,
            "分页合并必须与完整查询顺序一致：无重复、无遗漏"
        );

        // 重复查询结果必须一致（稳定排序）
        let again = repo
            .query(&HistoryQuery {
                limit: 6,
                offset: 0,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            again.into_iter().map(|r| r.id).collect::<Vec<_>>(),
            expected
        );
    }

    /// LIKE 搜索路径同样需要第二排序键 —— 搜索结果的翻页与全量一致。
    #[tokio::test]
    async fn search_pagination_is_stable_when_timestamps_collide() {
        let repo = repo();
        let ts = 1_700_000_000_000;
        let mut ids = Vec::new();
        for i in 0..4 {
            let mut rec = record_at(ts);
            rec.source_text = format!("apple {i}");
            ids.push(rec.id.clone());
            repo.insert(&rec).await.unwrap();
        }
        let expected: Vec<String> = ids.iter().rev().cloned().collect();

        let mut paged = Vec::new();
        for offset in [0, 2] {
            let page = repo
                .query(&HistoryQuery {
                    search: Some("apple".to_string()),
                    limit: 2,
                    offset,
                    ..Default::default()
                })
                .await
                .unwrap();
            paged.extend(page.into_iter().map(|r| r.id));
        }
        assert_eq!(paged, expected, "搜索结果分页必须稳定");
    }

    // ── CRUD 契约 ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn toggle_star_flips_and_persists() {
        let repo = repo();
        let rec = record_at(1_700_000_000_000);
        let id = rec.id.clone();
        repo.insert(&rec).await.unwrap();

        assert!(repo.toggle_star(&id).await.unwrap(), "首次切换 → 已收藏");
        assert!(
            repo.toggle_star(&id).await.unwrap() == false,
            "再次切换 → 取消收藏"
        );

        // 收藏状态必须真的落库（重新查询可见）
        repo.toggle_star(&id).await.unwrap();
        let rows = repo
            .query(&HistoryQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_starred);
    }

    #[tokio::test]
    async fn toggle_star_on_missing_id_is_an_error() {
        let repo = repo();
        let err = repo.toggle_star("no-such-id").await.unwrap_err();
        assert_eq!(err.error_code(), "DATABASE_ERROR");
    }

    #[tokio::test]
    async fn delete_by_id_removes_only_the_target() {
        let repo = repo();
        let a = record_at(1_700_000_000_000);
        let b = record_at(1_700_000_000_001);
        repo.insert(&a).await.unwrap();
        repo.insert(&b).await.unwrap();

        repo.delete_by_id(&a.id).await.unwrap();

        let rows = repo
            .query(&HistoryQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, b.id);
        assert_eq!(repo.count(None, false).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn clear_all_empties_the_table() {
        let repo = repo();
        for i in 0..3 {
            repo.insert(&record_at(1_700_000_000_000 + i))
                .await
                .unwrap();
        }
        repo.clear_all().await.unwrap();
        assert_eq!(repo.count(None, false).await.unwrap(), 0);
        assert!(repo
            .query(&HistoryQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty());
    }

    // ── FIFO 超限清理：只删最旧的未收藏记录 ─────────────────────────────

    #[tokio::test]
    async fn enforce_limit_deletes_oldest_unstarred_first() {
        let repo = repo();
        // 4 条记录，第 0 条（最旧）被收藏
        let mut ids = Vec::new();
        for i in 0..4 {
            let mut rec = record_at(1_700_000_000_000 + i * 1000);
            rec.is_starred = i == 0;
            ids.push(rec.id.clone());
            repo.insert(&rec).await.unwrap();
        }

        // 限制到 2 条：总 4 → 删 2 条，但收藏的第 0 条不能删
        let deleted = repo.enforce_limit(2).await.unwrap();
        assert_eq!(deleted, 2);

        let rows = repo
            .query(&HistoryQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        let remaining: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        assert!(remaining.contains(&ids[0]), "收藏记录永不自动删除");
        assert!(remaining.contains(&ids[3]), "最新的未收藏记录应保留");
        assert_eq!(
            repo.count(None, false).await.unwrap(),
            2,
            "总数应降到 limit（收藏记录占一个名额）"
        );
    }

    #[tokio::test]
    async fn enforce_limit_is_a_noop_below_the_threshold() {
        let repo = repo();
        repo.insert(&record_at(1_700_000_000_000)).await.unwrap();
        assert_eq!(repo.enforce_limit(10).await.unwrap(), 0);
        assert_eq!(repo.count(None, false).await.unwrap(), 1);
    }

    // ── LIKE 搜索：转义与 CJK ───────────────────────────────────────────

    #[tokio::test]
    async fn like_search_escapes_wildcards() {
        let repo = repo();
        let mut plain = record_at(1_700_000_000_000);
        plain.source_text = "100 percent done".to_string();
        repo.insert(&plain).await.unwrap();

        let mut with_pct = record_at(1_700_000_000_001);
        with_pct.source_text = "save 100% now".to_string();
        repo.insert(&with_pct).await.unwrap();

        // 搜索 "%" 只能命中真正含 % 的那条，不能变成「匹配一切」
        let rows = repo
            .query(&HistoryQuery {
                search: Some("%".to_string()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "% 必须按字面量匹配");
        assert_eq!(rows[0].id, with_pct.id);

        // 同理 "_" 不得通配单字符
        let rows = repo
            .query(&HistoryQuery {
                search: Some("_".to_string()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(rows.is_empty(), "_ 必须按字面量匹配");
    }

    #[tokio::test]
    async fn like_search_finds_cjk_substrings() {
        let repo = repo();
        let mut zh = record_at(1_700_000_000_000);
        zh.source_text = "机器学习模型".to_string();
        zh.translated_text = "machine learning model".to_string();
        repo.insert(&zh).await.unwrap();
        repo.insert(&record_at(1_700_000_000_001)).await.unwrap();

        let rows = repo
            .query(&HistoryQuery {
                search: Some("学习".to_string()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "中文子串搜索必须命中");

        // 译文侧也应可搜
        let rows = repo
            .query(&HistoryQuery {
                search: Some("learning".to_string()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    // ── 分页参数钳制 ────────────────────────────────────────────────────

    #[tokio::test]
    async fn query_clamps_limit_and_offset() {
        let repo = repo();
        repo.insert(&record_at(1_700_000_000_000)).await.unwrap();

        // limit=0 会被钳到 1（SQLite 里 LIMIT 0 会静默返回空页）
        let rows = repo
            .query(&HistoryQuery {
                limit: 0,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);

        // 负 offset 按 0 处理，不得报错
        let rows = repo
            .query(&HistoryQuery {
                limit: 10,
                offset: -5,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn starred_only_filters_both_query_and_count() {
        let repo = repo();
        let mut starred = record_at(1_700_000_000_000);
        starred.is_starred = true;
        repo.insert(&starred).await.unwrap();
        repo.insert(&record_at(1_700_000_000_001)).await.unwrap();

        let rows = repo
            .query(&HistoryQuery {
                limit: 10,
                starred_only: Some(true),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, starred.id);
        assert_eq!(repo.count(None, true).await.unwrap(), 1);
        assert_eq!(repo.count(None, false).await.unwrap(), 2);
    }

    // ── 大数据量：分页与搜索的查询计划与耗时（计划 6.6 / 6.9）──────────

    /// 在单事务里批量灌入 `n` 条记录，避免逐条落盘的测试开销。
    async fn seed_bulk(repo: &HistoryRepository, n: usize) -> Vec<String> {
        let conn = repo.conn().unwrap();
        let conn = conn.lock().await;
        let tx = conn.unchecked_transaction().unwrap();
        let mut ids = Vec::with_capacity(n);
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO translation_records
                     (id, source_text, translated_text, source_lang, target_lang,
                      provider, created_at, duration_ms, is_starred)
                     VALUES (?1, ?2, ?3, 'en', 'zh', 'google', ?4, 10, 0)",
                )
                .unwrap();
            for i in 0..n {
                let id = format!("bulk-{i:06}");
                // 时间递增，模拟真实使用；第 i 条比前一条新 1ms
                let text = if i % 100 == 0 {
                    format!("机器学习模型 batch {i}")
                } else {
                    format!("ordinary sample {i}")
                };
                stmt.execute(rusqlite::params![
                    id,
                    text,
                    format!("译文 {i}"),
                    1_700_000_000_000i64 + i as i64
                ])
                .unwrap();
                ids.push(id);
            }
        }
        tx.commit().unwrap();
        ids
    }

    /// 1 万条记录下分页查询走索引、耗时在可接受范围，且翻页结果正确。
    #[tokio::test]
    async fn ten_thousand_records_paginate_and_search_within_budget() {
        let repo = repo();
        let ids = seed_bulk(&repo, 10_000).await;
        assert_eq!(repo.count(None, false).await.unwrap(), 10_000);

        // 查询计划必须命中 created_at 索引，而不是全表扫描后排序
        {
            let conn = repo.conn().unwrap();
            let conn = conn.lock().await;
            let plan: String = conn
                .query_row(
                    "EXPLAIN QUERY PLAN
                     SELECT id FROM translation_records
                     ORDER BY created_at DESC, rowid DESC LIMIT 20 OFFSET 0",
                    [],
                    |r| r.get::<_, String>(3),
                )
                .unwrap_or_default();
            // 多行 plan 时只取第一行；这里用 query_row 已保证单行成立
            assert!(
                plan.contains("idx_records_created_at") || plan.contains("USING INDEX"),
                "分页查询应走索引，实际计划: {plan}"
            );
        }

        // 第一页：最新的 20 条（时间倒序）
        let start = std::time::Instant::now();
        let page = repo
            .query(&HistoryQuery {
                limit: 20,
                offset: 0,
                ..Default::default()
            })
            .await
            .unwrap();
        let first_page_elapsed = start.elapsed();
        assert_eq!(page.len(), 20);
        assert_eq!(page[0].id, ids[9_999], "第一页第一条必须是最新记录");
        assert!(
            first_page_elapsed < std::time::Duration::from_millis(500),
            "1 万条下第一页耗时 {:?}，超出可接受范围",
            first_page_elapsed
        );

        // 深分页（第 400 页）仍然正确且足够快
        let start = std::time::Instant::now();
        let deep = repo
            .query(&HistoryQuery {
                limit: 20,
                offset: 8_000,
                ..Default::default()
            })
            .await
            .unwrap();
        let deep_elapsed = start.elapsed();
        assert_eq!(deep.len(), 20);
        assert_eq!(deep[0].id, ids[1_999], "偏移 8000 → 第 8000 新的记录");
        assert!(
            deep_elapsed < std::time::Duration::from_millis(500),
            "1 万条下深分页耗时 {:?}，超出可接受范围",
            deep_elapsed
        );

        // CJK 搜索（LIKE 全表匹配，无索引可用 —— 只要求不失控）
        let start = std::time::Instant::now();
        let hits = repo
            .query(&HistoryQuery {
                search: Some("机器学习".to_string()),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        let search_elapsed = start.elapsed();
        assert_eq!(hits.len(), 20, "搜索命中 100 条，首页取 20");
        assert_eq!(repo.count(Some("机器学习"), false).await.unwrap(), 100);
        assert!(
            search_elapsed < std::time::Duration::from_millis(500),
            "1 万条下中文搜索耗时 {:?}，超出可接受范围",
            search_elapsed
        );
    }

    /// FIFO 清理在 1 万条规模下只删最旧的未收藏行，且数量精确。
    #[tokio::test]
    async fn enforce_limit_at_scale_deletes_exactly_the_overflow() {
        let repo = repo();
        let ids = seed_bulk(&repo, 10_000).await;

        // 收藏最新的 5 条：它们无论如何不能被删
        for id in &ids[9_995..] {
            repo.toggle_star(id).await.unwrap();
        }

        let deleted = repo.enforce_limit(200).await.unwrap();
        assert_eq!(deleted, 9_800);
        assert_eq!(repo.count(None, false).await.unwrap(), 200);

        // 收藏的 5 条必须还在
        let starred = repo
            .query(&HistoryQuery {
                limit: 10,
                starred_only: Some(true),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(starred.len(), 5, "收藏记录必须全部存活");
    }

    // ── get_stats 的天数钳制 ─────────────────────────────────────────────

    #[tokio::test]
    async fn days_parameter_is_clamped_to_bounds() {
        let repo = repo();
        for k in 0..10 {
            repo.insert(&record_at(noon_days_ago(k))).await.unwrap();
        }

        // 缺省 7 天：今天的部分 + 前 6 天
        let s = repo.get_stats(None).await.unwrap();
        assert_eq!(s.daily.len(), 7);
        assert_eq!(s.daily.last().unwrap().count, 1, "末位是今天");

        // 0 与超界都钳到边界
        assert_eq!(repo.get_stats(Some(0)).await.unwrap().daily.len(), 1);
        assert_eq!(repo.get_stats(Some(999)).await.unwrap().daily.len(), 90);

        // 31 天：跨到 9 天前，共 31 个逐日槽位，其中 10 天有数据
        let s = repo.get_stats(Some(31)).await.unwrap();
        assert_eq!(s.daily.len(), 31);
        assert_eq!(s.daily.iter().map(|d| d.count).sum::<u64>(), 10);
    }

    // ── get_stats_range 的输入校验 ───────────────────────────────────────

    #[tokio::test]
    async fn stats_range_rejects_inverted_and_oversized_intervals() {
        let repo = repo();

        let now = crate::types::now_unix_ms();
        let err = repo
            .get_stats_range(now, now - MS_PER_DAY)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("结束早于开始"),
            "倒挂区间应被拒绝: {}",
            err
        );

        let err = repo
            .get_stats_range(now - 91 * MS_PER_DAY, now)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("90"),
            "超过 90 天的区间应被拒绝: {}",
            err
        );
    }

    #[tokio::test]
    async fn stats_range_includes_both_endpoint_days() {
        let repo = repo();
        // 今天与 2 天前各一条，区间恰好覆盖这两天：同日起止（1 天）到
        // 首尾都含的 3 天窗口都应正确聚合
        repo.insert(&record_at(noon_days_ago(0))).await.unwrap();
        repo.insert(&record_at(noon_days_ago(2))).await.unwrap();

        let s = repo
            .get_stats_range(noon_days_ago(0), noon_days_ago(0))
            .await
            .unwrap();
        assert_eq!(s.daily.len(), 1, "同日起止 = 单日窗口");
        assert_eq!(s.daily[0].count, 1);

        let s = repo
            .get_stats_range(noon_days_ago(2), noon_days_ago(0))
            .await
            .unwrap();
        assert_eq!(s.daily.len(), 3, "两端各自的本地日都应包含在内");
        assert_eq!(s.daily.iter().map(|d| d.count).sum::<u64>(), 2);
    }
}
