// e2e/scenarios/smoke.mjs
// 冒烟场景：应用能带隔离目录启动、数据确实落在隔离目录、基本 IPC 可用、
// 窗口目标存在。

import {
  launchApp,
  connectMainWindow,
  connectToWindow,
  isolatedFiles,
  shutdown,
  assert,
  sleep,
} from "../harness.mjs";

export async function run() {
  const app = await launchApp();
  try {
    // 1. 数据落在隔离目录：SQLite 与日志必须出现（真实用户目录不受影响）
    await sleep(2000);
    const files = isolatedFiles(app.dataDir);
    assert(
      files.some((f) => f.endsWith(".sqlite") || f.includes("db")),
      `隔离目录里应有数据库文件，实际: ${files.join(", ")}`
    );

    // 2. 连接任一 webview 目标，基础 IPC 可用
    const page = await connectMainWindow(app.port);
    const version = await page.invoke("get_app_version");
    assert(typeof version === "string" && version.length > 0, `get_app_version 返回异常: ${version}`);

    // 3. get_config 返回默认配置（隔离目录是全新环境，无用户痕迹）
    const cfg = await page.invoke("get_config");
    assert(cfg && typeof cfg === "object", "get_config 必须返回配置对象");
    assert(cfg.onboarding_completed === false || cfg.onboarding_completed === true,
      `onboarding_completed 应为布尔，实际: ${cfg.onboarding_completed}`);

    // 4. 运行时诊断可读（引擎/剪贴板/存储组件状态聚合）
    const diag = await page.invoke("get_runtime_diagnostics");
    assert(diag && typeof diag === "object", "get_runtime_diagnostics 必须返回对象");

    // 5. 打开设置窗口 → 目标列表出现 #settings
    await page.invoke("open_settings_window");
    const settings = await connectToWindow(app.port, "#settings");
    const sVersion = await settings.invoke("get_app_version");
    assert(sVersion === version, "两个窗口的版本信息应一致");
    settings.close();

    page.close();
  } finally {
    await shutdown(app);
  }
}
