// src-tauri/src/system/glossary_sync.rs
// DeepL glossary 同步（计划 Task 10）。
//
// 职责：把本地术语表（按语言对）同步成 DeepL glossary，并返回本次翻译
// 应使用的 glossary_id 与指纹。
//
// 边界：
// - 只在「术语表开启 + 当前翻译源是 DeepL + 有显式源语言 + 该语言对有
//   条目」时工作。自动检测源语言的路径不应用（DeepL 限制，见
//   docs/GLOSSARY_CAPABILITY_ASSESSMENT.md）。
// - 同步失败只降级为「本次不应用术语表」并记日志，绝不阻塞翻译。
// - 指纹未变时复用已记录的 glossary_id，不重复创建。
// - v2 glossary 无编辑接口：指纹变化 → 新建 → 更新映射 → 尽力删除旧
//   glossary（删除失败只记 warn，旧条目在 DeepL 账号侧残留不影响使用）。

use rusqlite::OptionalExtension;

use crate::domain::glossary::{self, GlossaryEntry};
use crate::domain::translator::deepl;
use crate::error::AppError;
use crate::infra::database::Db;
use crate::state::AppState;
use crate::types::now_unix_ms;

pub struct GlossarySyncRecord {
    pub fingerprint: String,
    pub glossary_id: String,
}

/// 「语言对 → DeepL glossary」映射的持久化。
pub struct GlossarySyncStore {
    db: Db,
}

impl GlossarySyncStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get(&self, pair_key: &str) -> Result<Option<GlossarySyncRecord>, AppError> {
        let Some(conn) = self.db.try_conn() else {
            return Ok(None);
        };
        let conn = conn.lock().await;
        conn.query_row(
            "SELECT fingerprint, glossary_id FROM deepl_glossary_sync WHERE pair_key = ?1",
            rusqlite::params![pair_key],
            |row| {
                Ok(GlossarySyncRecord {
                    fingerprint: row.get(0)?,
                    glossary_id: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::DatabaseError(e.to_string()))
    }

    pub async fn upsert(
        &self,
        pair_key: &str,
        fingerprint: &str,
        glossary_id: &str,
    ) -> Result<(), AppError> {
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        conn.execute(
            "INSERT INTO deepl_glossary_sync (pair_key, fingerprint, glossary_id, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(pair_key) DO UPDATE SET
                 fingerprint = excluded.fingerprint,
                 glossary_id = excluded.glossary_id,
                 updated_at  = excluded.updated_at",
            rusqlite::params![pair_key, fingerprint, glossary_id, now_unix_ms()],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn remove(&self, pair_key: &str) -> Result<(), AppError> {
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        conn.execute(
            "DELETE FROM deepl_glossary_sync WHERE pair_key = ?1",
            rusqlite::params![pair_key],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}

/// 本次翻译实际应用的术语表。
pub struct AppliedGlossary {
    pub glossary_id: String,
    pub fingerprint: String,
}

fn pair_key(source_lang: &str, target_lang: &str) -> String {
    format!("deepl\u{1}{}\u{1}{}", source_lang, target_lang)
}

/// TSV 每行 `原文\t译文`；术语里的制表符/换行替换为空格（DeepL 以行为
/// 单位解析，控制字符会破坏格式）。
fn entries_tsv(entries: &[GlossaryEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            format!(
                "{}\t{}",
                sanitize_tsv(&e.source_term),
                sanitize_tsv(&e.target_term)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_tsv(term: &str) -> String {
    term.replace(['\t', '\n', '\r'], " ")
}

/// 按当前配置准备 DeepL glossary。返回 None 表示本次不应用术语表
/// （未开启、当前源不是 DeepL、无显式源语言、该语言对无条目、或同步失败）。
pub async fn prepare_deepl_glossary(
    state: &AppState,
    source_lang: Option<&str>,
    target_lang: &str,
) -> Option<AppliedGlossary> {
    let source_lang = source_lang?;

    let (enabled, active_provider) = {
        let cfg = state.config.read().await;
        (
            cfg.get("glossary_enabled")
                .map(|v| v == "true")
                .unwrap_or(true),
            cfg.get("provider").unwrap_or_default(),
        )
    };
    if !enabled || active_provider != "deepl" {
        return None;
    }

    let entries = match state.glossary.list(source_lang, target_lang).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(
                event = "glossary_list_failed",
                "[glossary] 读取术语表失败，本次不应用: {}",
                e
            );
            return None;
        }
    };
    if entries.is_empty() {
        return None;
    }

    let fingerprint = glossary::fingerprint(&entries, true);
    let key = pair_key(source_lang, target_lang);

    // 指纹未变：直接复用已同步的 glossary
    if let Ok(Some(record)) = state.glossary_sync.get(&key).await {
        if record.fingerprint == fingerprint && !record.glossary_id.is_empty() {
            return Some(AppliedGlossary {
                glossary_id: record.glossary_id,
                fingerprint,
            });
        }
    }

    let api_key = state.config.read().await.get_credential("deepl_api_key");
    let name = format!(
        "quicktranslate-{}-{}-{}",
        source_lang,
        target_lang,
        &fingerprint[..8.min(fingerprint.len())]
    );
    let tsv = entries_tsv(&entries);

    match deepl::create_glossary(
        &state.http_client,
        &api_key,
        &name,
        source_lang,
        target_lang,
        &tsv,
    )
    .await
    {
        Ok(glossary_id) => {
            let previous = state.glossary_sync.get(&key).await.ok().flatten();
            if let Err(e) = state
                .glossary_sync
                .upsert(&key, &fingerprint, &glossary_id)
                .await
            {
                tracing::warn!(
                    event = "glossary_sync_store_failed",
                    "[glossary] 记录 glossary 映射失败（本次仍使用）: {}",
                    e
                );
            }
            if let Some(previous) = previous {
                if previous.glossary_id != glossary_id {
                    // 尽力清理旧 glossary：失败只记日志，不影响本次结果
                    if let Err(e) =
                        deepl::delete_glossary(&state.http_client, &api_key, &previous.glossary_id)
                            .await
                    {
                        tracing::warn!(
                            event = "glossary_cleanup_failed",
                            "[glossary] 删除旧 DeepL glossary 失败: {}",
                            e
                        );
                    }
                }
            }
            tracing::info!(
                event = "glossary_synced",
                source_lang = %source_lang,
                target_lang = %target_lang,
                entries = entries.len(),
                "[glossary] 术语表已同步到 DeepL"
            );
            Some(AppliedGlossary {
                glossary_id,
                fingerprint,
            })
        }
        Err(e) => {
            tracing::warn!(
                event = "glossary_sync_failed",
                source_lang = %source_lang,
                target_lang = %target_lang,
                "[glossary] 同步 DeepL 术语表失败，本次不应用（翻译继续）: {}",
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use super::*;
    use crate::infra::database::GLOSSARY_SYNC_TABLE_SQL;

    fn store() -> GlossarySyncStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(GLOSSARY_SYNC_TABLE_SQL).unwrap();
        GlossarySyncStore::new(Db::available(Arc::new(Mutex::new(conn))))
    }

    #[tokio::test]
    async fn upsert_then_get_roundtrips() {
        let store = store();
        assert!(store.get("deepl\u{1}en\u{1}zh").await.unwrap().is_none());

        store
            .upsert("deepl\u{1}en\u{1}zh", "fp1", "glossary-1")
            .await
            .unwrap();
        let record = store.get("deepl\u{1}en\u{1}zh").await.unwrap().unwrap();
        assert_eq!(record.fingerprint, "fp1");
        assert_eq!(record.glossary_id, "glossary-1");

        // 冲突更新：只保留最新映射
        store
            .upsert("deepl\u{1}en\u{1}zh", "fp2", "glossary-2")
            .await
            .unwrap();
        let record = store.get("deepl\u{1}en\u{1}zh").await.unwrap().unwrap();
        assert_eq!(record.fingerprint, "fp2");
        assert_eq!(record.glossary_id, "glossary-2");
    }

    #[tokio::test]
    async fn remove_deletes_only_the_selected_pair() {
        let store = store();
        store
            .upsert("deepl\u{1}en\u{1}zh", "fp1", "g1")
            .await
            .unwrap();
        store
            .upsert("deepl\u{1}en\u{1}ja", "fp2", "g2")
            .await
            .unwrap();

        store.remove("deepl\u{1}en\u{1}zh").await.unwrap();
        assert!(store.get("deepl\u{1}en\u{1}zh").await.unwrap().is_none());
        assert!(store.get("deepl\u{1}en\u{1}ja").await.unwrap().is_some());
    }

    #[test]
    fn tsv_sanitizes_control_characters() {
        let entries = vec![GlossaryEntry {
            id: 1,
            source_lang: "en".into(),
            target_lang: "zh".into(),
            source_term: "a\tb".into(),
            target_term: "c\nd".into(),
            note: String::new(),
            case_sensitive: false,
            created_at: 0,
            updated_at: 0,
        }];
        assert_eq!(entries_tsv(&entries), "a b\tc d");
    }

    #[test]
    fn pair_key_is_scoped_by_provider_and_languages() {
        assert_eq!(pair_key("en", "zh"), "deepl\u{1}en\u{1}zh");
        assert_ne!(pair_key("en", "zh"), pair_key("en", "ja"));
    }
}
