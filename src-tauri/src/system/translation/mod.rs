// src-tauri/src/system/translation/mod.rs
// 翻译请求的生命周期层。
//
// 与 `system::translation_flow` 的分工：
//
//   translation/（本模块）—— 请求身份、代际、编排、结果闸门。
//     回答「这个结果还属于当前请求吗」「历史写在哪一步」。
//
//   translation_flow —— 浮窗的呈现：建窗、定位、loading/result/error 事件。
//     回答「这个结果该显示在哪里」。Phase 8 会重构它的窗口策略。
//
// 依赖方向是单向的：coordinator → translation_flow。

pub mod coordinator;
pub mod request;

pub use coordinator::{execute_at_position, TranslationCoordinator};
pub use request::{RequestGeneration, RequestId, TranslationRequest, MAX_TEXT_CHARS};
