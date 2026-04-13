// src-tauri/src/domain/history.rs
// 翻译历史仓库：CRUD、FIFO 超限清理、FTS5 全文搜索

use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::types::{HistoryQuery, TranslationRecord};

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
                provider, created_at, duration_ms)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            rusqlite::params![
                record.id,
                record.source_text,
                record.translated_text,
                record.source_lang,
                record.target_lang,
                record.provider,
                record.created_at,
                record.duration_ms,
            ],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 查询历史记录（支持 FTS5 搜索 + 分页）
    pub async fn query(&self, params: &HistoryQuery) -> Result<Vec<TranslationRecord>, AppError> {
        let conn = self.db.lock().await;

        let records = if let Some(search) = &params.search {
            if search.trim().is_empty() {
                query_all(&conn, params.limit, params.offset)?
            } else {
                query_fts(&conn, search, params.limit, params.offset)?
            }
        } else {
            query_all(&conn, params.limit, params.offset)?
        };

        Ok(records)
    }

    /// 获取记录总数（用于前端分页）
    pub async fn count(&self, search: Option<&str>) -> Result<i64, AppError> {
        let conn = self.db.lock().await;

        let count = if let Some(keyword) = search {
            if keyword.trim().is_empty() {
                count_all(&conn)?
            } else {
                count_fts(&conn, keyword)?
            }
        } else {
            count_all(&conn)?
        };

        Ok(count)
    }

    /// FIFO 超限清理：删除最旧的记录，使总数不超过 limit
    pub async fn enforce_limit(&self, limit: i64) -> Result<u64, AppError> {
        let conn = self.db.lock().await;
        let total = count_all(&conn)?;

        if total <= limit {
            return Ok(0);
        }

        let to_delete = total - limit;
        let deleted = conn
            .execute(
                r#"DELETE FROM translation_records
                   WHERE id IN (
                       SELECT id FROM translation_records
                       ORDER BY created_at ASC
                       LIMIT ?1
                   )"#,
                rusqlite::params![to_delete],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::debug!("FIFO 清理：删除了 {} 条旧记录", deleted);
        Ok(deleted as u64)
    }

    /// 清空所有历史记录并重建 FTS 索引
    pub async fn clear_all(&self) -> Result<(), AppError> {
        let conn = self.db.lock().await;
        conn.execute_batch(
            r#"DELETE FROM translation_records;
               INSERT INTO translation_records_fts(translation_records_fts) VALUES('rebuild');"#,
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

// ---- 内部查询辅助函数 ----

fn query_all(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<TranslationRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            r#"SELECT id, source_text, translated_text, source_lang, target_lang,
                      provider, created_at, duration_ms
               FROM translation_records
               ORDER BY created_at DESC
               LIMIT ?1 OFFSET ?2"#,
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let records = stmt
        .query_map(rusqlite::params![limit, offset], map_row)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(records)
}

fn query_fts(
    conn: &Connection,
    search: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TranslationRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            r#"SELECT tr.id, tr.source_text, tr.translated_text, tr.source_lang,
                      tr.target_lang, tr.provider, tr.created_at, tr.duration_ms
               FROM translation_records tr
               INNER JOIN translation_records_fts fts ON tr.rowid = fts.rowid
               WHERE translation_records_fts MATCH ?1
               ORDER BY tr.created_at DESC
               LIMIT ?2 OFFSET ?3"#,
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let records = stmt
        .query_map(rusqlite::params![search, limit, offset], map_row)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(records)
}

fn count_all(conn: &Connection) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT COUNT(*) FROM translation_records",
        [],
        |row| row.get(0),
    )
    .map_err(|e| AppError::DatabaseError(e.to_string()))
}

fn count_fts(conn: &Connection, search: &str) -> Result<i64, AppError> {
    conn.query_row(
        r#"SELECT COUNT(*) FROM translation_records tr
           INNER JOIN translation_records_fts fts ON tr.rowid = fts.rowid
           WHERE translation_records_fts MATCH ?1"#,
        rusqlite::params![search],
        |row| row.get(0),
    )
    .map_err(|e| AppError::DatabaseError(e.to_string()))
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
    })
}
