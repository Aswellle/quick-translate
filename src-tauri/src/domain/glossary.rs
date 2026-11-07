// src-tauri/src/domain/glossary.rs
// 本地术语表（计划 Task 10 的最小功能：原文术语/目标译法/可选说明、
// 大小写敏感、启用与导入导出）。
//
// 设计边界（见 docs/GLOSSARY_CAPABILITY_ASSESSMENT.md）：
// - 本模块只负责**本地数据**；术语如何应用到翻译请求由 Provider 能力决定。
//   当前只有 DeepL 支持应用端管理 glossary，其余 Provider 只能引用控制台
//   术语库或不支持 —— 不做无差别字符串替换，也不假装生效。
// - 同一原文（大小写不敏感）只允许一条：两条同名条目译法不同时无法决定
//   用哪条，在写入/导入阶段就拒绝，而不是留给运行时猜。
// - 术语属于本地敏感数据：不写日志、不进入历史导出，随数据库文件保存。

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::infra::database::Db;
use crate::types::now_unix_ms;

/// 单条术语字段的长度上限（字符数）。DeepL glossary 的单条上限是 1024
/// 字节，这里取更保守的值，保证导出到各 Provider 都不会先超限。
pub const MAX_TERM_CHARS: usize = 500;
/// 说明字段长度上限（字符数）
pub const MAX_NOTE_CHARS: usize = 500;
/// 术语条数上限：同步到 DeepL glossary 时整包传输，过大既慢又容易超限
pub const MAX_ENTRIES: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryEntry {
    pub id: i64,
    pub source_term: String,
    pub target_term: String,
    pub note: String,
    pub case_sensitive: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 新建/更新/导入时使用的入参（不含 id 与时间戳）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryEntryInput {
    pub source_term: String,
    pub target_term: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub case_sensitive: bool,
}

/// 校验并归一化一条输入：去首尾空白、检查空值与长度。
///
/// 返回错误文案直接面向用户（设置页原样展示），不包含术语内容以外的
/// 内部细节；重复项由仓库层在写库前检查。
pub fn normalize_entry(entry: GlossaryEntryInput) -> Result<GlossaryEntryInput, String> {
    let source_term = entry.source_term.trim().to_string();
    let target_term = entry.target_term.trim().to_string();
    let note = entry.note.trim().to_string();

    if source_term.is_empty() {
        return Err("原文术语不能为空".to_string());
    }
    if target_term.is_empty() {
        return Err("目标译法不能为空".to_string());
    }
    if source_term.chars().count() > MAX_TERM_CHARS {
        return Err(format!("原文术语超过 {} 个字符", MAX_TERM_CHARS));
    }
    if target_term.chars().count() > MAX_TERM_CHARS {
        return Err(format!("目标译法超过 {} 个字符", MAX_TERM_CHARS));
    }
    if note.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("说明超过 {} 个字符", MAX_NOTE_CHARS));
    }

    Ok(GlossaryEntryInput {
        source_term,
        target_term,
        note,
        case_sensitive: entry.case_sensitive,
    })
}

/// 分批校验并检查批内重复（大小写不敏感）。导入路径使用。
pub fn normalize_batch(
    entries: Vec<GlossaryEntryInput>,
) -> Result<Vec<GlossaryEntryInput>, String> {
    if entries.len() > MAX_ENTRIES {
        return Err(format!("术语条数超过上限 {}", MAX_ENTRIES));
    }
    let mut seen = std::collections::HashSet::new();
    let mut normalized = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = normalize_entry(entry)?;
        let key = entry.source_term.to_lowercase();
        if !seen.insert(key) {
            return Err(format!("术语「{}」重复出现", entry.source_term));
        }
        normalized.push(entry);
    }
    Ok(normalized)
}

pub struct GlossaryRepository {
    db: Db,
}

impl GlossaryRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 全部条目，按原文排序（大小写不敏感）。存储不可用 → 空列表
    /// （设置页如实显示为空，写入会返回 StorageUnavailable）。
    pub async fn list(&self) -> Result<Vec<GlossaryEntry>, AppError> {
        let Some(conn) = self.db.try_conn() else {
            return Ok(Vec::new());
        };
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, source_term, target_term, note, case_sensitive,
                        created_at, updated_at
                   FROM glossary_entries
                  ORDER BY source_term COLLATE NOCASE ASC, id ASC",
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        let rows = stmt
            .query_map([], map_row)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::DatabaseError(e.to_string()))
    }

    pub async fn count(&self) -> Result<i64, AppError> {
        let Some(conn) = self.db.try_conn() else {
            return Ok(0);
        };
        let conn = conn.lock().await;
        conn.query_row("SELECT COUNT(*) FROM glossary_entries", [], |r| r.get(0))
            .map_err(|e| AppError::DatabaseError(e.to_string()))
    }

    /// 新增一条。重复（大小写不敏感）→ ConfigError（含用户可读文案）。
    pub async fn create(&self, input: GlossaryEntryInput) -> Result<GlossaryEntry, AppError> {
        let input = normalize_entry(input).map_err(AppError::ConfigError)?;
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;

        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM glossary_entries
                  WHERE source_term = ?1 COLLATE NOCASE",
                rusqlite::params![input.source_term],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        if exists.is_some() {
            return Err(duplicate_error(&input.source_term));
        }

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM glossary_entries", [], |r| r.get(0))
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        if count >= MAX_ENTRIES as i64 {
            return Err(AppError::ConfigError(format!(
                "术语条数已达上限 {}",
                MAX_ENTRIES
            )));
        }

        let now = now_unix_ms();
        conn.execute(
            "INSERT INTO glossary_entries
                 (source_term, target_term, note, case_sensitive, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            rusqlite::params![
                input.source_term,
                input.target_term,
                input.note,
                input.case_sensitive as i64,
                now
            ],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(GlossaryEntry {
            id: conn.last_insert_rowid(),
            source_term: input.source_term,
            target_term: input.target_term,
            note: input.note,
            case_sensitive: input.case_sensitive,
            created_at: now,
            updated_at: now,
        })
    }

    /// 更新一条（id 不存在 → ConfigError，而不是静默成功）。
    pub async fn update(
        &self,
        id: i64,
        input: GlossaryEntryInput,
    ) -> Result<GlossaryEntry, AppError> {
        let input = normalize_entry(input).map_err(AppError::ConfigError)?;
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;

        let duplicate: Option<i64> = conn
            .query_row(
                "SELECT id FROM glossary_entries
                  WHERE source_term = ?1 COLLATE NOCASE AND id <> ?2",
                rusqlite::params![input.source_term, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        if duplicate.is_some() {
            return Err(duplicate_error(&input.source_term));
        }

        let now = now_unix_ms();
        let changed = conn
            .execute(
                "UPDATE glossary_entries
                    SET source_term = ?1, target_term = ?2, note = ?3,
                        case_sensitive = ?4, updated_at = ?5
                  WHERE id = ?6",
                rusqlite::params![
                    input.source_term,
                    input.target_term,
                    input.note,
                    input.case_sensitive as i64,
                    now,
                    id
                ],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        if changed == 0 {
            return Err(AppError::ConfigError("术语条目不存在".to_string()));
        }

        let (created_at,): (i64,) = conn
            .query_row(
                "SELECT created_at FROM glossary_entries WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?,)),
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(GlossaryEntry {
            id,
            source_term: input.source_term,
            target_term: input.target_term,
            note: input.note,
            case_sensitive: input.case_sensitive,
            created_at,
            updated_at: now,
        })
    }

    pub async fn delete(&self, id: i64) -> Result<bool, AppError> {
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        let deleted = conn
            .execute(
                "DELETE FROM glossary_entries WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(deleted > 0)
    }

    pub async fn clear(&self) -> Result<i64, AppError> {
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        let deleted = conn
            .execute("DELETE FROM glossary_entries", [])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(deleted as i64)
    }

    /// 全量替换（导入使用）：整批校验通过后在同一事务里删旧写新。
    /// 任意一条非法或重复 → 整批拒绝，原数据不动。
    pub async fn replace_all(&self, entries: Vec<GlossaryEntryInput>) -> Result<usize, AppError> {
        let entries = normalize_batch(entries).map_err(AppError::ConfigError)?;
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        tx.execute("DELETE FROM glossary_entries", [])
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        let now = now_unix_ms();
        for entry in &entries {
            tx.execute(
                "INSERT INTO glossary_entries
                     (source_term, target_term, note, case_sensitive, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                rusqlite::params![
                    entry.source_term,
                    entry.target_term,
                    entry.note,
                    entry.case_sensitive as i64,
                    now
                ],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        }
        tx.commit()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(entries.len())
    }
}

fn duplicate_error(source_term: &str) -> AppError {
    AppError::ConfigError(format!(
        "术语「{}」已存在（大小写不敏感），请直接修改该条",
        source_term
    ))
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GlossaryEntry> {
    Ok(GlossaryEntry {
        id: row.get(0)?,
        source_term: row.get(1)?,
        target_term: row.get(2)?,
        note: row.get(3)?,
        case_sensitive: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

// ── 导入 / 导出（纯函数，CSV 与 JSON）─────────────────────────────────

/// 导出为 JSON（数组，包含全部业务字段；不含 id/时间戳，便于跨设备导入）。
pub fn export_json(entries: &[GlossaryEntry]) -> Result<String, AppError> {
    let payload: Vec<GlossaryEntryInput> = entries
        .iter()
        .map(|e| GlossaryEntryInput {
            source_term: e.source_term.clone(),
            target_term: e.target_term.clone(),
            note: e.note.clone(),
            case_sensitive: e.case_sensitive,
        })
        .collect();
    serde_json::to_string_pretty(&payload).map_err(|e| AppError::SerdeError(e.to_string()))
}

/// 解析 JSON 导入内容。支持裸数组；未知字段忽略，缺失 case_sensitive 视为
/// false。解析失败返回用户可读的文案（不包含文件原始内容）。
pub fn import_json(input: &str) -> Result<Vec<GlossaryEntryInput>, String> {
    let entries: Vec<GlossaryEntryInput> =
        serde_json::from_str(input).map_err(|e| format!("JSON 解析失败：{}", e))?;
    normalize_batch(entries)
}

/// 导出 CSV：首行为表头，字段按需加引号（逗号/引号/换行）。
pub fn export_csv(entries: &[GlossaryEntry]) -> String {
    let mut out = String::from("source_term,target_term,note,case_sensitive\n");
    for entry in entries {
        out.push_str(&csv_field(&entry.source_term));
        out.push(',');
        out.push_str(&csv_field(&entry.target_term));
        out.push(',');
        out.push_str(&csv_field(&entry.note));
        out.push(',');
        out.push_str(if entry.case_sensitive {
            "true"
        } else {
            "false"
        });
        out.push('\n');
    }
    out
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// 解析 CSV 导入内容。
///
/// 首行是表头（`source_term,target_term,note,case_sensitive`）时按列名取；
/// 否则按同序取前四列。空行跳过；多出的列忽略。字段支持双引号包裹、
/// `""` 转义与字段内换行。
pub fn import_csv(input: &str) -> Result<Vec<GlossaryEntryInput>, String> {
    let rows = parse_csv(input)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let first = &rows[0];
    let has_header = first
        .first()
        .map(|v| v.trim().eq_ignore_ascii_case("source_term"))
        .unwrap_or(false);

    let start = usize::from(has_header);
    let mut entries = Vec::with_capacity(rows.len().saturating_sub(start));
    for (line_no, row) in rows.iter().enumerate().skip(start) {
        if row.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        if row.len() < 2 {
            return Err(format!("第 {} 行少于两列", line_no + 1));
        }
        let case_sensitive = row
            .get(3)
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
            .unwrap_or(false);
        entries.push(GlossaryEntryInput {
            source_term: row[0].clone(),
            target_term: row[1].clone(),
            note: row.get(2).cloned().unwrap_or_default(),
            case_sensitive,
        });
    }

    normalize_batch(entries)
}

fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut field_started = false;
    let mut chars = input.chars().peekable();

    fn push_field(row: &mut Vec<String>, field: &mut String, field_started: &mut bool) {
        row.push(std::mem::take(field));
        *field_started = false;
    }

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }

        match ch {
            '"' => {
                if field_started {
                    return Err("CSV 引号位置非法".to_string());
                }
                in_quotes = true;
                field_started = true;
            }
            ',' => {
                push_field(&mut row, &mut field, &mut field_started);
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                push_field(&mut row, &mut field, &mut field_started);
                rows.push(std::mem::take(&mut row));
            }
            _ => {
                field.push(ch);
                field_started = true;
            }
        }
    }

    if in_quotes {
        return Err("CSV 引号未闭合".to_string());
    }
    if field_started || !field.is_empty() || !row.is_empty() {
        push_field(&mut row, &mut field, &mut field_started);
        rows.push(row);
    }
    Ok(rows)
}

// ── 指纹（缓存键用）────────────────────────────────────────────────────

/// 术语表指纹：启用且有条目时非空，供缓存键区分不同术语状态。
///
/// 排序后再哈希：条目顺序变化不应导致缓存整体失效。说明字段不参与 ——
/// 它不影响翻译结果。禁用或空表返回空串，保持与引入术语表之前的缓存键
/// 兼容。
pub fn fingerprint(entries: &[GlossaryEntry], enabled: bool) -> String {
    if !enabled || entries.is_empty() {
        return String::new();
    }
    use sha2::{Digest, Sha256};
    let mut lines: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "{}\u{1}{}\u{1}{}",
                e.source_term.to_lowercase(),
                e.target_term,
                u8::from(e.case_sensitive)
            )
        })
        .collect();
    lines.sort();
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use super::*;
    use crate::infra::database::GLOSSARY_TABLE_SQL;

    fn repo() -> GlossaryRepository {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(GLOSSARY_TABLE_SQL).unwrap();
        GlossaryRepository::new(Db::available(Arc::new(Mutex::new(conn))))
    }

    fn input(source: &str, target: &str) -> GlossaryEntryInput {
        GlossaryEntryInput {
            source_term: source.to_string(),
            target_term: target.to_string(),
            note: String::new(),
            case_sensitive: false,
        }
    }

    #[tokio::test]
    async fn create_list_update_delete_roundtrip() {
        let repo = repo();
        let created = repo.create(input("Apple", "苹果")).await.unwrap();
        assert!(created.id > 0);
        assert_eq!(created.source_term, "Apple");

        let list = repo.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].target_term, "苹果");

        let updated = repo
            .update(
                created.id,
                GlossaryEntryInput {
                    source_term: "Apple".into(),
                    target_term: "苹果公司".into(),
                    note: "公司名".into(),
                    case_sensitive: true,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.target_term, "苹果公司");
        assert!(updated.case_sensitive);

        assert!(repo.delete(created.id).await.unwrap());
        assert!(repo.list().await.unwrap().is_empty());
        assert!(
            !repo.delete(created.id).await.unwrap(),
            "重复删除返回 false"
        );
    }

    #[tokio::test]
    async fn duplicate_source_is_rejected_case_insensitively() {
        let repo = repo();
        repo.create(input("Apple", "苹果")).await.unwrap();
        let err = repo.create(input("apple", "苹果")).await.unwrap_err();
        assert!(err.to_string().contains("已存在"), "{}", err);

        let err = repo.create(input("APPLE", "苹果")).await.unwrap_err();
        assert!(err.to_string().contains("已存在"));
    }

    #[tokio::test]
    async fn update_cannot_collide_with_another_entry() {
        let repo = repo();
        let a = repo.create(input("Apple", "苹果")).await.unwrap();
        repo.create(input("Banana", "香蕉")).await.unwrap();
        let err = repo
            .update(a.id, input("banana", "香蕉"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("已存在"));
    }

    #[tokio::test]
    async fn empty_and_oversized_fields_are_rejected() {
        let repo = repo();
        assert!(repo.create(input("  ", "x")).await.is_err());
        assert!(repo.create(input("x", "  ")).await.is_err());
        let long = "a".repeat(MAX_TERM_CHARS + 1);
        assert!(repo.create(input(&long, "x")).await.is_err());
    }

    #[tokio::test]
    async fn replace_all_is_atomic_and_validates_the_batch() {
        let repo = repo();
        repo.create(input("Apple", "苹果")).await.unwrap();

        // 批内重复 → 整批拒绝，原数据不动
        let err = repo
            .replace_all(vec![input("A", "1"), input("a", "2")])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("重复"));
        assert_eq!(repo.count().await.unwrap(), 1);
        assert_eq!(repo.list().await.unwrap()[0].source_term, "Apple");

        let written = repo
            .replace_all(vec![input("Banana", "香蕉"), input("Cherry", "樱桃")])
            .await
            .unwrap();
        assert_eq!(written, 2);
        assert_eq!(repo.count().await.unwrap(), 2);
    }

    // ── 导入 / 导出 ─────────────────────────────────────────────────────

    fn sample_entries() -> Vec<GlossaryEntry> {
        vec![
            GlossaryEntry {
                id: 1,
                source_term: "Apple".into(),
                target_term: "苹果".into(),
                note: "含,逗号".into(),
                case_sensitive: true,
                created_at: 0,
                updated_at: 0,
            },
            GlossaryEntry {
                id: 2,
                source_term: "say \"hi\"".into(),
                target_term: "问好".into(),
                note: "含\n换行".into(),
                case_sensitive: false,
                created_at: 0,
                updated_at: 0,
            },
        ]
    }

    #[test]
    fn csv_export_then_import_roundtrips_quotes_commas_and_newlines() {
        let exported = export_csv(&sample_entries());
        let imported = import_csv(&exported).unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].source_term, "Apple");
        assert_eq!(imported[0].note, "含,逗号");
        assert!(imported[0].case_sensitive);
        assert_eq!(imported[1].source_term, "say \"hi\"");
        assert_eq!(imported[1].note, "含\n换行");
        assert!(!imported[1].case_sensitive);
    }

    #[test]
    fn csv_import_accepts_files_without_header() {
        let imported = import_csv("Apple,苹果\nBanana,香蕉,备注\n").unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[1].note, "备注");
    }

    #[test]
    fn csv_import_rejects_malformed_rows() {
        assert!(import_csv("Apple\n").is_err(), "少于两列");
        assert!(import_csv("\"Apple,苹果\n").is_err(), "引号未闭合");
        assert!(import_csv("Apple,苹果\napple,苹果2\n").is_err(), "重复");
    }

    #[test]
    fn json_export_then_import_roundtrips() {
        let exported = export_json(&sample_entries()).unwrap();
        let imported = import_json(&exported).unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].source_term, "Apple");
        assert_eq!(imported[1].note, "含\n换行");
    }

    #[test]
    fn json_import_rejects_invalid_and_duplicate_content() {
        assert!(import_json("not json").is_err());
        assert!(import_json(
            r#"[{"source_term":"A","target_term":"1"},{"source_term":"a","target_term":"2"}]"#
        )
        .is_err());
        assert!(import_json(r#"[{"source_term":"","target_term":"1"}]"#).is_err());
    }

    // ── 指纹 ─────────────────────────────────────────────────────────────

    #[test]
    fn fingerprint_is_stable_order_independent_and_ignores_notes() {
        let entries = sample_entries();
        let mut reversed = entries.clone();
        reversed.reverse();

        let fp = fingerprint(&entries, true);
        assert!(!fp.is_empty());
        assert_eq!(fp, fingerprint(&reversed, true), "顺序不应影响指纹");

        let mut with_note = entries.clone();
        with_note[0].note = "改了说明".into();
        assert_eq!(fp, fingerprint(&with_note, true), "说明不影响翻译结果");

        let mut changed = entries.clone();
        changed[0].target_term = "苹果公司".into();
        assert_ne!(fp, fingerprint(&changed, true), "译法变化必须改变指纹");
    }

    #[test]
    fn fingerprint_is_empty_when_disabled_or_empty() {
        let entries = sample_entries();
        assert_eq!(fingerprint(&entries, false), "");
        assert_eq!(fingerprint(&[], true), "");
    }
}
