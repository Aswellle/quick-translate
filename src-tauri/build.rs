fn main() {
    // 测试可执行文件需要自带 Common-Controls v6 清单，否则 tauri-plugin-dialog
    // 引入的 TaskDialogIndirect 静态导入会在测试进程启动时直接失败
    // （bin 目标的清单由 tauri_build::build() 提供；同一可执行文件里的两份
    // 清单输入由 MSVC 链接器合并，依赖声明是增量性质，互不冲突）。
    #[cfg(target_os = "windows")]
    {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-tests.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::build()
}
