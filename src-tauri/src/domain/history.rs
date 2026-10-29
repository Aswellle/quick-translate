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
                   ORDER BY created_at DESC"#,
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
                       ORDER BY created_at ASC
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
           ORDER BY created_at DESC
           LIMIT ?1 OFFSET ?2"#
    } else {
        r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                  provider, created_at, duration_ms, is_starred
           FROM translation_records
           ORDER BY created_at DESC
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
           ORDER BY created_at DESC
           LIMIT ?2 OFFSET ?3"#
    } else {
        r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                  provider, created_at, duration_ms, is_starred
           FROM translation_records
           WHERE source_text LIKE ?1 ESCAPE '\' OR translated_text LIKE ?1 ESCAPE '\'
           ORDER BY created_at DESC
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
