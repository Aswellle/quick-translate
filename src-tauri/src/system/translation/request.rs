// src-tauri/src/system/translation/request.rs
// 翻译请求的身份与代际管理。
//
// 计划第 6 节要求去掉隐式的「当前请求」概念。此前 `AppState` 里只有一个
// `JoinHandle`，谁都无法回答「这个刚回来的结果还属于当前请求吗」——
// 于是迟到的旧结果可以直接覆盖新结果。RequestId + RequestGeneration
// 就是为回答这个问题而存在的。

use uuid::Uuid;

/// 翻译前的最大字符数，超出即截断并置 `truncated`
pub const MAX_TEXT_CHARS: usize = 5000;

/// 一次翻译请求的唯一标识。
///
/// 用 Uuid 而非进程内递增计数：id 的语义是「这一次请求」，而不是「第几次」，
/// 因此不该携带任何顺序含义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestId(Uuid);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// 日志里只打前 8 位：足够在单次会话内区分请求，又不必把完整 UUID 铺满每行。
impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.0.simple().to_string();
        f.write_str(&s[..8])
    }
}

/// 一次翻译请求的完整描述。
///
/// 刻意只有一个 `text` 字段，而不是计划里写的 `source_text` + `normalized_text`
/// 两个：worker 在归一化那一刻就丢弃了原文（`clipboard::worker` 的 pending
/// 存的就是归一化结果），下游 `TranslationRecord` 与翻译引擎消费的也全是
/// 归一化文本。今天没有任何消费者需要原文，加它只会得到一个死字段 ——
/// 真要恢复原文，得先让 worker 同时携带两份文本，那是另一件事。
#[derive(Debug, Clone)]
pub struct TranslationRequest {
    pub id: RequestId,
    /// 待翻译文本（已归一化，已按 MAX_TEXT_CHARS 截断）
    pub text: String,
    pub target_lang: String,
    /// 触发时的光标位置（屏幕物理像素，与 `GetCursorPos` 同坐标系）
    pub cursor: (f64, f64),
    /// 是否因超过 MAX_TEXT_CHARS 被截断
    pub truncated: bool,
}

impl TranslationRequest {
    pub fn new(id: RequestId, text: String, target_lang: String, cursor: (f64, f64)) -> Self {
        let truncated = text.chars().count() > MAX_TEXT_CHARS;
        let text = if truncated {
            text.chars().take(MAX_TEXT_CHARS).collect()
        } else {
            text
        };
        Self {
            id,
            text,
            target_lang,
            cursor,
            truncated,
        }
    }
}

/// 请求代际。
///
/// 所有「这个结果是否已过期」的判断都收敛到这里，且刻意做成不含 I/O、
/// 不含锁的纯状态机（计划第 54 节要求 `is_stale_request()` 纯函数化）——
/// 「A 起、B 起、A 迟到 → 丢弃」这条链因此不需要 Tauri runtime 就能测。
///
/// 注意这里**没有** cancel/finish：作废是「不再是 current」的推论，
/// 不需要显式动作。任何「先标记作废、再启动新请求」的两步写法都会重新
/// 引入两步之间的竞态窗口 —— 而这个类型存在的全部意义就是消灭它。
#[derive(Debug, Default)]
pub struct RequestGeneration {
    current: Option<RequestId>,
}

impl RequestGeneration {
    /// 开启新一代请求。返回新 id，上一代就此作废。
    pub fn begin(&mut self) -> RequestId {
        let id = RequestId::new();
        self.current = Some(id);
        id
    }

    /// 该请求是否仍是当前代 —— 所有结果回传之前的唯一闸门。
    pub fn is_current(&self, id: RequestId) -> bool {
        self.current == Some(id)
    }

    pub fn current(&self) -> Option<RequestId> {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 计划第 36 节：A 起 → B 起 → A 迟到必须被丢弃 ────────────────────

    #[test]
    fn newer_request_supersedes_older() {
        let mut g = RequestGeneration::default();
        let a = g.begin();
        assert!(g.is_current(a), "刚开启的请求必须是当前代");

        let b = g.begin();
        assert!(g.is_current(b), "后开启的请求成为当前代");
        assert!(
            !g.is_current(a),
            "A 必须立刻作废 —— 这正是 B 起之后 A 迟到被丢弃的依据"
        );
    }

    #[test]
    fn first_request_is_current() {
        let mut g = RequestGeneration::default();
        let a = g.begin();
        assert!(g.is_current(a));
    }

    #[test]
    fn unknown_id_is_never_current() {
        let mut g = RequestGeneration::default();
        let a = g.begin();
        assert!(!g.is_current(RequestId::new()));
        assert_eq!(g.current(), Some(a));
    }

    #[test]
    fn empty_generation_has_no_current() {
        let g = RequestGeneration::default();
        assert_eq!(g.current(), None);
        assert!(!g.is_current(RequestId::new()));
    }

    /// 连续多代：只有最后一代是 current，中间全部作废
    #[test]
    fn only_latest_survives_a_burst() {
        let mut g = RequestGeneration::default();
        let ids: Vec<_> = (0..10).map(|_| g.begin()).collect();
        let last = *ids.last().unwrap();

        for id in &ids[..ids.len() - 1] {
            assert!(!g.is_current(*id), "{} 应已作废", id);
        }
        assert!(g.is_current(last));
    }

    /// id 不能碰撞 —— 否则「不是 current」会误判成「是 current」
    #[test]
    fn request_ids_are_unique() {
        let ids: std::collections::HashSet<_> =
            (0..1000).map(|_| RequestId::new().to_string()).collect();
        assert_eq!(ids.len(), 1000);
    }

    // ── TranslationRequest 构造 ──────────────────────────────────────────

    #[test]
    fn short_text_is_not_truncated() {
        let r = TranslationRequest::new(
            RequestId::new(),
            "hello".to_string(),
            "zh".to_string(),
            (10.0, 20.0),
        );
        assert_eq!(r.text, "hello");
        assert!(!r.truncated);
        assert_eq!(r.target_lang, "zh");
        assert_eq!(r.cursor, (10.0, 20.0));
    }

    #[test]
    fn long_text_is_truncated_to_the_limit() {
        let long = "a".repeat(MAX_TEXT_CHARS + 500);
        let r = TranslationRequest::new(RequestId::new(), long, "zh".to_string(), (0.0, 0.0));
        assert!(r.truncated);
        assert_eq!(r.text.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn text_at_exactly_the_limit_is_not_truncated() {
        let exact = "a".repeat(MAX_TEXT_CHARS);
        let r = TranslationRequest::new(RequestId::new(), exact, "zh".to_string(), (0.0, 0.0));
        assert!(!r.truncated, "恰好等于上限不该被标记为截断");
        assert_eq!(r.text.chars().count(), MAX_TEXT_CHARS);
    }

    /// 截断必须按字符而非字节 —— 否则多字节文本会被切在字符中间
    #[test]
    fn truncation_counts_characters_not_bytes() {
        let cjk = "中".repeat(MAX_TEXT_CHARS + 10);
        let r = TranslationRequest::new(RequestId::new(), cjk, "en".to_string(), (0.0, 0.0));
        assert!(r.truncated);
        assert_eq!(r.text.chars().count(), MAX_TEXT_CHARS);
        // 能走到这里说明没有把 UTF-8 序列切断（否则会 panic 或产生替换字符）
        assert!(r.text.chars().all(|c| c == '中'));
    }
}
