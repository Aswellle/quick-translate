// src-tauri/src/runtime/mod.rs
// 运行时层：应用「现在到底怎么样」的唯一权威答案（计划第 4 节）。
//
// 与配置严格分离（计划第 49 节）：
//
//   配置（config / DB）—— target_lang、active_provider、凭证、主题、history_limit
//   运行时（本模块）  —— 各子系统的健康、在途请求、网络降级
//
// 严禁把运行时健康写进用户配置，也严禁把凭证带进状态快照。

pub mod lifecycle;
pub mod status;

pub use status::{
    ComponentHealth, ComponentState, RuntimeHealth, RuntimeStatus, RuntimeStatusSnapshot,
};
