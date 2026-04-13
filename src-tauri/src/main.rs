// src-tauri/src/main.rs
// 二进制入口：仅保留 fn main()，业务逻辑全部在 lib.rs

// 禁止在 Windows 上弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    quicktranslate_lib::run();
}
