// src-tauri/src/domain/history.rs
// 翻译历史仓库：CRUD、FIFO 超限清理、FTS5 全文搜索

use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::types::{HistoryQuery, StatsResult, TranslationRecord};

pub struct HistoryRepository {
    db: Arc<Mutex<Connection>>,
}

impl HistoryRepository {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        HistoryRepository { db }
    }

    /// 插入翻译记录（FTS 通过 trigger 自动同步）
    pub async fn insert(&self, record: &TranslationRecord) -> Result<(), AppError> {
        let conn = self.db.lock().await;
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
        let conn = self.db.lock().await;

        let starred_only = params.starred_only.unwrap_or(false);

        let records = if let Some(search) = &params.search {
            if search.trim().is_empty() {
                query_all(&conn, params.limit, params.offset, starred_only)?
            } else {
                query_like(&conn, search, params.limit, params.offset, starred_only)?
            }
        } else {
            query_all(&conn, params.limit, params.offset, starred_only)?
        };

        Ok(records)
    }

    /// 获取记录总数（用于前端分页）
    pub async fn count(&self, search: Option<&str>, starred_only: bool) -> Result<i64, AppError> {
        let conn = self.db.lock().await;

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
        let conn = self.db.lock().await;

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
        let conn = self.db.lock().await;
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

    /// 获取使用统计
    pub async fn get_stats(&self) -> Result<StatsResult, AppError> {
        let conn = self.db.lock().await;

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
        let ms_7_days: i64 = 7 * 24 * 60 * 60 * 1000;
        let ms_30_days: i64 = 30 * 24 * 60 * 60 * 1000;

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
        })
    }

    /// FIFO 超限清理：删除最旧的记录，使总数不超过 limit
    pub async fn enforce_limit(&self, limit: i64) -> Result<u64, AppError> {
        let conn = self.db.lock().await;
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
        let conn = self.db.lock().await;
        conn.execute("DELETE FROM translation_records WHERE id = ?1", [id])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// 清空所有历史记录
    pub async fn clear_all(&self) -> Result<(), AppError> {
        let conn = self.db.lock().await;
        conn.execute("DELETE FROM translation_records", [])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}

// ---- 内部查询辅助函数 ----

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
        search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
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
        search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
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
