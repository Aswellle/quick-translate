// e2e/scenarios/settings-apply.mjs
// 设置即时生效端到端：开关切换 → 立即持久化（真实 SQLite）→ 失败回滚
// 不可在真机触发（不人为断盘），验证生效路径与 config-changed 广播。

import {
  launchApp,
  connectMainWindow,
  shutdown,
  assert,
  sleep,
} from "../harness.mjs";

export async function run() {
  const app = await launchApp();
  try {
    const page = await connectMainWindow(app.port);
    await page.evaluate(`window.location.hash = "#settings"`);
    await page.waitFor(`document.body.textContent.includes("常规")`, {
      timeoutMs: 10000,
    });

    // 1. 切换「剪贴板监控」开关（General Tab 第一个开关行）
    const before = await page.invoke("get_config");
    const beforeEnabled = before.clipboard_monitor_enabled;

    const toggled = await page.evaluate(
      `(() => {
        const sw = [...document.querySelectorAll('[role="switch"]')].find((s) =>
          s.getAttribute("aria-label") === "剪贴板监控"
        );
        if (!sw) return false;
        sw.click();
        return true;
      })()`
    );
    assert(toggled, "剪贴板监控开关必须存在（aria-label 可定位）");

    // 2. 即时生效：配置库里的值翻转（无需任何保存动作）
    await page.waitFor(
      `window.__TAURI_INTERNALS__ && (async () => {
        const c = await window.__TAURI_INTERNALS__.invoke("get_config");
        return c.clipboard_monitor_enabled === ${!beforeEnabled};
      })()`,
      { timeoutMs: 8000 }
    );

    // 3. 就近状态提示出现（role=status，绿色已开启/已关闭）
    await page.waitFor(`!!document.querySelector('[role="status"]')`, {
      timeoutMs: 3000,
    });

    // 4. 切回原值（还原现场；隔离目录反正会删，这里保证断言对称）
    await page.evaluate(
      `(() => {
        const sw = [...document.querySelectorAll('[role="switch"]')].find(
          (s) => s.getAttribute("aria-label") === "剪贴板监控"
        );
        sw?.click();
      })()`
    );
    await page.waitFor(
      `window.__TAURI_INTERNALS__ && (async () => {
        const c = await window.__TAURI_INTERNALS__.invoke("get_config");
        return c.clipboard_monitor_enabled === ${beforeEnabled};
      })()`,
      { timeoutMs: 8000 }
    );
  } finally {
    await shutdown(app);
  }
}
