fn main() {
    // 所有可执行目标（bin / 测试）统一由链接器嵌入 windows-app.manifest。
    //
    // 不能用 tauri-winres 自带的清单资源：它只嵌进 bin，而测试可执行文件
    // 同样需要 Common-Controls v6（tauri-plugin-dialog→rfd 与 muda 的
    // TaskDialogIndirect 静态导入在 System32 comctl32 5.82 里不存在，
    // 没有清单的测试进程启动即 STATUS_ENTRYPOINT_NOT_FOUND）；反过来，
    // 若在 winres 清单之外再传 /MANIFESTINPUT，bin 链接时两份 RT_MANIFEST
    // ID 1 资源会撞出 CVT1100 资源重复。跳过 winres 清单、让链接器对
    // 全部目标嵌同一份，两个问题同时消失。
    #[cfg(windows)]
    {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("failed to run tauri-build");
        return;
    }
    #[cfg(not(windows))]
    {
        tauri_build::build()
    }
}
