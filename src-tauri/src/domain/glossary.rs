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
    pub source_lang: String,
    pub target_lang: String,
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
    /// 导入文件可省略语言列；缺省时由导入命令选定的语言对填充
    #[serde(default)]
    pub source_lang: String,
    #[serde(default)]
    pub target_lang: String,
    pub source_term: String,
    pub target_term: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub case_sensitive: bool,
}

/// 校验并归一化一条输入：去首尾空白、检查语言对、空值与长度。
///
/// 返回错误文案直接面向用户（设置页原样展示）。重复项由仓库层在写库前
/// 按语言对检查。
pub fn normalize_entry(entry: GlossaryEntryInput) -> Result<GlossaryEntryInput, String> {
    let source_lang = entry.source_lang.trim().to_string();
    let target_lang = entry.target_lang.trim().to_string();
    let source_term = entry.source_term.trim().to_string();
    let target_term = entry.target_term.trim().to_string();
    let note = entry.note.trim().to_string();

    if !crate::types::SUPPORTED_LANGS.contains(&source_lang.as_str()) {
        return Err("源语言不受支持".to_string());
    }
    if !crate::types::SUPPORTED_LANGS.contains(&target_lang.as_str()) {
        return Err("目标语言不受支持".to_string());
    }
    if source_lang == target_lang {
        return Err("源语言与目标语言不能相同".to_string());
    }
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
    // 指纹分隔符防御：术语原文/译法不允许出现 U+0001 —— 它是
    // fingerprint() 的字段分隔符，("x\u{1}y","z") 与 ("x","y\u{1}z")
    // 两条单条目表会产出同一指纹，让 DeepL 同步复用错误的 glossary。
    // 术语是人工维护的词条，含控制字符本身就是粘贴垃圾。
    if source_term.contains('\u{1}') || target_term.contains('\u{1}') {
        return Err("术语不能包含控制字符".to_string());
    }
    if note.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("说明超过 {} 个字符", MAX_NOTE_CHARS));
    }

    Ok(GlossaryEntryInput {
        source_lang,
        target_lang,
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
        let key = format!(
            "{}\u{1}{}\u{1}{}",
            entry.source_lang,
            entry.target_lang,
            entry.source_term.to_lowercase()
        );
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

    /// 指定语言对的全部条目，按原文排序（大小写不敏感）。存储不可用 →
    /// 空列表（设置页如实显示为空，写入会返回 StorageUnavailable）。
    pub async fn list(
        &self,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<Vec<GlossaryEntry>, AppError> {
        let Some(conn) = self.db.try_conn() else {
            return Ok(Vec::new());
        };
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, source_lang, target_lang, source_term, target_term,
                        note, case_sensitive, created_at, updated_at
                   FROM glossary_entries
                  WHERE source_lang = ?1 AND target_lang = ?2
                  ORDER BY source_term COLLATE NOCASE ASC, id ASC",
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![source_lang, target_lang], map_row)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::DatabaseError(e.to_string()))
    }

    pub async fn count(&self, source_lang: &str, target_lang: &str) -> Result<i64, AppError> {
        let Some(conn) = self.db.try_conn() else {
            return Ok(0);
        };
        let conn = conn.lock().await;
        conn.query_row(
            "SELECT COUNT(*) FROM glossary_entries
              WHERE source_lang = ?1 AND target_lang = ?2",
            rusqlite::params![source_lang, target_lang],
            |r| r.get(0),
        )
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
                  WHERE source_lang = ?1 AND target_lang = ?2
                    AND source_term = ?3 COLLATE NOCASE",
                rusqlite::params![input.source_lang, input.target_lang, input.source_term],
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
                 (source_lang, target_lang, source_term, target_term, note,
                  case_sensitive, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            rusqlite::params![
                input.source_lang,
                input.target_lang,
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
            source_lang: input.source_lang,
            target_lang: input.target_lang,
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
                  WHERE source_lang = ?1 AND target_lang = ?2
                    AND source_term = ?3 COLLATE NOCASE AND id <> ?4",
                rusqlite::params![input.source_lang, input.target_lang, input.source_term, id],
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
                    SET source_lang = ?1, target_lang = ?2, source_term = ?3,
                        target_term = ?4, note = ?5, case_sensitive = ?6,
                        updated_at = ?7
                  WHERE id = ?8",
                rusqlite::params![
                    input.source_lang,
                    input.target_lang,
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
            source_lang: input.source_lang,
            target_lang: input.target_lang,
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

    /// 清空指定语言对的全部条目（其他语言对不受影响）。
    pub async fn clear(&self, source_lang: &str, target_lang: &str) -> Result<i64, AppError> {
        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        let deleted = conn
            .execute(
                "DELETE FROM glossary_entries
                  WHERE source_lang = ?1 AND target_lang = ?2",
                rusqlite::params![source_lang, target_lang],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        Ok(deleted as i64)
    }

    /// 全量替换**指定语言对**的条目（导入使用）：整批校验通过后在同一
    /// 事务里删旧写新；其他语言对不受影响。任意一条非法/重复/语言对
    /// 不一致 → 整批拒绝，原数据不动。
    pub async fn replace_pair(
        &self,
        source_lang: &str,
        target_lang: &str,
        entries: Vec<GlossaryEntryInput>,
    ) -> Result<usize, AppError> {
        // 导入文件可以省略语言列；统一用命令选定的语言对填充，
        // 显式给出的语言对必须与选定的完全一致。
        let entries: Vec<GlossaryEntryInput> = entries
            .into_iter()
            .map(|mut e| {
                if e.source_lang.trim().is_empty() {
                    e.source_lang = source_lang.to_string();
                }
                if e.target_lang.trim().is_empty() {
                    e.target_lang = target_lang.to_string();
                }
                e
            })
            .collect();
        let entries = normalize_batch(entries).map_err(AppError::ConfigError)?;
        if let Some(bad) = entries
            .iter()
            .find(|e| e.source_lang != source_lang || e.target_lang != target_lang)
        {
            return Err(AppError::ConfigError(format!(
                "导入内容属于 {} → {}，与当前语言对 {} → {} 不一致",
                bad.source_lang, bad.target_lang, source_lang, target_lang
            )));
        }

        let conn = self.db.try_conn().ok_or(AppError::StorageUnavailable)?;
        let conn = conn.lock().await;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        tx.execute(
            "DELETE FROM glossary_entries
              WHERE source_lang = ?1 AND target_lang = ?2",
            rusqlite::params![source_lang, target_lang],
        )
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        let now = now_unix_ms();
        for entry in &entries {
            tx.execute(
                "INSERT INTO glossary_entries
                     (source_lang, target_lang, source_term, target_term, note,
                      case_sensitive, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                rusqlite::params![
                    entry.source_lang,
                    entry.target_lang,
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
        source_lang: row.get(1)?,
        target_lang: row.get(2)?,
        source_term: row.get(3)?,
        target_term: row.get(4)?,
        note: row.get(5)?,
        case_sensitive: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

// ── 导入 / 导出（纯函数，CSV 与 JSON）─────────────────────────────────

/// 导出为 JSON（数组，包含语言对与业务字段；不含 id/时间戳，便于跨设备
/// 导入）。导入到不同语言对会被拒绝。
pub fn export_json(entries: &[GlossaryEntry]) -> Result<String, AppError> {
    let payload: Vec<GlossaryEntryInput> = entries
        .iter()
        .map(|e| GlossaryEntryInput {
            source_lang: e.source_lang.clone(),
            target_lang: e.target_lang.clone(),
            source_term: e.source_term.clone(),
            target_term: e.target_term.clone(),
            note: e.note.clone(),
            case_sensitive: e.case_sensitive,
        })
        .collect();
    serde_json::to_string_pretty(&payload).map_err(|e| AppError::SerdeError(e.to_string()))
}

/// 解析 JSON 导入内容。支持裸数组；未知字段忽略，缺失语言对的条目用
/// 命令选定的语言对填充；显式给出的语言对必须与选定值一致。解析失败
/// 返回用户可读的文案（不包含文件原始内容）。
pub fn import_json(
    input: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<GlossaryEntryInput>, String> {
    let entries: Vec<GlossaryEntryInput> =
        serde_json::from_str(input).map_err(|e| format!("JSON 解析失败：{}", e))?;
    finish_import(entries, source_lang, target_lang)
}

fn finish_import(
    entries: Vec<GlossaryEntryInput>,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<GlossaryEntryInput>, String> {
    let entries = entries
        .into_iter()
        .map(|mut e| {
            if e.source_lang.trim().is_empty() {
                e.source_lang = source_lang.to_string();
            }
            if e.target_lang.trim().is_empty() {
                e.target_lang = target_lang.to_string();
            }
            e
        })
        .collect();
    let entries = normalize_batch(entries)?;
    if entries
        .iter()
        .any(|e| e.source_lang != source_lang || e.target_lang != target_lang)
    {
        return Err(format!(
            "导入内容包含其他语言对，请按 {} → {} 分别导入",
            source_lang, target_lang
        ));
    }
    Ok(entries)
}

/// 导出 CSV：首行为表头，字段按需加引号（逗号/引号/换行）。
pub fn export_csv(entries: &[GlossaryEntry]) -> String {
    let mut out =
        String::from("source_lang,target_lang,source_term,target_term,note,case_sensitive\n");
    for entry in entries {
        out.push_str(&csv_field(&entry.source_lang));
        out.push(',');
        out.push_str(&csv_field(&entry.target_lang));
        out.push(',');
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
/// 首行是表头时按列名取；否则按 `source_lang,target_lang,source_term,
/// target_term,note,case_sensitive` 排序取列。为了兼容手写文件，也接受
/// 不含语言列的 `source_term,target_term,note,case_sensitive` 四列格式
/// （语言对取命令选定值）。空行跳过；多出的列忽略。字段支持双引号包裹、
/// `""` 转义与字段内换行。
pub fn import_csv(
    input: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<GlossaryEntryInput>, String> {
    let rows = parse_csv(input)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let first: Vec<String> = rows[0].iter().map(|v| v.trim().to_lowercase()).collect();
    let header_index = |name: &str| first.iter().position(|v| v == name);
    let has_header = first.first().map(|v| v == "source_term").unwrap_or(false)
        || first.first().map(|v| v == "source_lang").unwrap_or(false);

    let (idx_lang, idx_term) = if has_header {
        let src = header_index("source_lang");
        let tgt = header_index("target_lang");
        let term = header_index("source_term").ok_or("表头缺少 source_term 列")?;
        let tgt_term = header_index("target_term").ok_or("表头缺少 target_term 列")?;
        let note = header_index("note");
        let case = header_index("case_sensitive");
        (Some((src, tgt)), (term, tgt_term, note, case))
    } else {
        // 无表头：按列数判断是六列（含语言）还是四列（不含语言）
        let with_langs = rows[0].len() >= 6;
        if with_langs {
            (Some((Some(0), Some(1))), (2, 3, Some(4), Some(5)))
        } else {
            (None, (0, 1, Some(2), Some(3)))
        }
    };

    let start = usize::from(has_header);
    let mut entries = Vec::with_capacity(rows.len().saturating_sub(start));
    for (line_no, row) in rows.iter().enumerate().skip(start) {
        if row.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let term_idx = idx_term.0;
        let tgt_idx = idx_term.1;
        if row.len() <= term_idx || row.len() <= tgt_idx {
            return Err(format!("第 {} 行缺少原文/译法列", line_no + 1));
        }
        let pick = |idx: Option<usize>| -> String {
            idx.and_then(|i| row.get(i))
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let case_sensitive = idx_term
            .3
            .and_then(|i| row.get(i))
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
            .unwrap_or(false);

        entries.push(GlossaryEntryInput {
            source_lang: idx_lang.map(|(s, _)| pick(s)).unwrap_or_default(),
            target_lang: idx_lang.map(|(_, t)| pick(t)).unwrap_or_default(),
            source_term: pick(Some(term_idx)),
            target_term: pick(Some(tgt_idx)),
            note: pick(idx_term.2),
            case_sensitive,
        });
    }

    finish_import(entries, source_lang, target_lang)
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
                "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
                e.source_lang,
                e.target_lang,
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
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
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
        assert_eq!(created.source_lang, "en");

        let list = repo.list("en", "zh").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].target_term, "苹果");

        let updated = repo
            .update(
                created.id,
                GlossaryEntryInput {
                    source_lang: "en".into(),
                    target_lang: "zh".into(),
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
        assert!(repo.list("en", "zh").await.unwrap().is_empty());
        assert!(
            !repo.delete(created.id).await.unwrap(),
            "重复删除返回 false"
        );
    }

    /// 同一原文在不同语言对中是两条独立术语。
    #[tokio::test]
    async fn entries_are_scoped_to_their_language_pair() {
        let repo = repo();
        repo.create(input("Apple", "苹果")).await.unwrap();
        let mut ja = input("Apple", "アップル");
        ja.target_lang = "ja".into();
        repo.create(ja).await.unwrap();

        assert_eq!(repo.list("en", "zh").await.unwrap().len(), 1);
        assert_eq!(repo.list("en", "ja").await.unwrap().len(), 1);
        assert_eq!(
            repo.list("en", "ja").await.unwrap()[0].target_term,
            "アップル"
        );
        assert!(repo.list("en", "fr").await.unwrap().is_empty());
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
    async fn invalid_language_pair_and_fields_are_rejected() {
        let repo = repo();
        assert!(repo.create(input("  ", "x")).await.is_err());
        assert!(repo.create(input("x", "  ")).await.is_err());

        let mut same = input("x", "y");
        same.target_lang = "en".into();
        assert!(repo.create(same).await.is_err(), "源/目标语言相同");

        let mut unknown = input("x", "y");
        unknown.source_lang = "xx".into();
        assert!(repo.create(unknown).await.is_err(), "未知语言");

        let long = "a".repeat(MAX_TERM_CHARS + 1);
        assert!(repo.create(input(&long, "x")).await.is_err());
    }

    #[tokio::test]
    async fn replace_pair_is_atomic_and_leaves_other_pairs_alone() {
        let repo = repo();
        repo.create(input("Apple", "苹果")).await.unwrap();
        let mut ja = input("Apple", "アップル");
        ja.target_lang = "ja".into();
        repo.create(ja).await.unwrap();

        // 批内重复 → 整批拒绝，原数据不动
        let err = repo
            .replace_pair("en", "zh", vec![input("A", "1"), input("a", "2")])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("重复"));
        assert_eq!(repo.count("en", "zh").await.unwrap(), 1);
        assert_eq!(repo.list("en", "zh").await.unwrap()[0].source_term, "Apple");

        // 显式给出其他语言对 → 拒绝
        let mut wrong = input("Banana", "香蕉");
        wrong.target_lang = "ja".into();
        assert!(repo.replace_pair("en", "zh", vec![wrong]).await.is_err());

        let written = repo
            .replace_pair(
                "en",
                "zh",
                vec![input("Banana", "香蕉"), input("Cherry", "樱桃")],
            )
            .await
            .unwrap();
        assert_eq!(written, 2);
        assert_eq!(repo.count("en", "zh").await.unwrap(), 2);
        assert_eq!(
            repo.count("en", "ja").await.unwrap(),
            1,
            "其他语言对不受影响"
        );
    }

    // ── 导入 / 导出 ─────────────────────────────────────────────────────

    fn sample_entries() -> Vec<GlossaryEntry> {
        vec![
            GlossaryEntry {
                id: 1,
                source_lang: "en".into(),
                target_lang: "zh".into(),
                source_term: "Apple".into(),
                target_term: "苹果".into(),
                note: "含,逗号".into(),
                case_sensitive: true,
                created_at: 0,
                updated_at: 0,
            },
            GlossaryEntry {
                id: 2,
                source_lang: "en".into(),
                target_lang: "zh".into(),
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
        let imported = import_csv(&exported, "en", "zh").unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].source_term, "Apple");
        assert_eq!(imported[0].note, "含,逗号");
        assert!(imported[0].case_sensitive);
        assert_eq!(imported[0].source_lang, "en");
        assert_eq!(imported[1].source_term, "say \"hi\"");
        assert_eq!(imported[1].note, "含\n换行");
        assert!(!imported[1].case_sensitive);
    }

    #[test]
    fn csv_import_accepts_four_column_files_without_languages() {
        let imported = import_csv("Apple,苹果\nBanana,香蕉,备注\n", "en", "zh").unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[1].note, "备注");
        assert_eq!(imported[0].source_lang, "en");
        assert_eq!(imported[0].target_lang, "zh");
    }

    #[test]
    fn csv_import_rejects_malformed_rows_and_other_pairs() {
        assert!(import_csv("Apple\n", "en", "zh").is_err(), "少于两列");
        assert!(
            import_csv("\"Apple,苹果\n", "en", "zh").is_err(),
            "引号未闭合"
        );
        assert!(
            import_csv("Apple,苹果\napple,苹果2\n", "en", "zh").is_err(),
            "重复"
        );
        assert!(
            import_csv("en,ja,Apple,アップル,,\n", "en", "zh").is_err(),
            "显式不同语言对必须拒绝"
        );
    }

    #[test]
    fn json_export_then_import_roundtrips() {
        let exported = export_json(&sample_entries()).unwrap();
        let imported = import_json(&exported, "en", "zh").unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].source_term, "Apple");
        assert_eq!(imported[1].note, "含\n换行");
    }

    #[test]
    fn json_import_rejects_invalid_duplicate_and_other_pairs() {
        assert!(import_json("not json", "en", "zh").is_err());
        assert!(import_json(
            r#"[{"source_term":"A","target_term":"1"},{"source_term":"a","target_term":"2"}]"#,
            "en",
            "zh"
        )
        .is_err());
        assert!(import_json(r#"[{"source_term":"","target_term":"1"}]"#, "en", "zh").is_err());
        assert!(import_json(
            r#"[{"source_lang":"en","target_lang":"ja","source_term":"A","target_term":"1"}]"#,
            "en",
            "zh"
        )
        .is_err());
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
