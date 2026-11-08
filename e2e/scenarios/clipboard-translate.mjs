// e2e/scenarios/clipboard-translate.mjs
// 剪贴板主链路端到端：写剪贴板（合成文本）→ 监控器拾取 → 过滤门 →
// 翻译引擎 → 落历史。这是产品的核心路径，此前只有 mock 层覆盖。
//
// 注意：场景会替换真实系统剪贴板内容（e2e 的固有代价，报告中明示）。
// 冷启动首秒的请求可能因网络栈未就绪失败 —— 监控器对相同文本会去重，
// 重试用不同文本（真实用户会复制新的内容）。

import {
  launchApp,
  connectMainWindow,
  shutdown,
  assert,
  sleep,
} from "../harness.mjs";

import { execFileSync } from "node:child_process";

const SAMPLE = "The quick brown fox jumps over the lazy dog.";
const SAMPLE2 = "Pack my box with five dozen liquor jugs.";

function setClipboard(text) {
  // PowerShell 通道写入避免 Git Bash 参数转义问题
  execFileSync(
    "powershell",
    ["-NoProfile", "-Command", `Set-Clipboard -Value '${text.replace(/'/g, "''")}'`],
    { encoding: "utf8" }
  );
}

/** 轮询历史直到出现指定文本的记录（clipboard 来源） */
async function waitForHistoryRecord(page, textPrefix, { timeoutMs = 25000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const history = await page.invoke("query_history", {
      params: { limit: 10, offset: 0 },
    });
    const rec = (Array.isArray(history) ? history : []).find(
      (r) => (r.source_text ?? "").startsWith(textPrefix)
    );
    if (rec) return rec;
    await sleep(500);
  }
  return null;
}

export async function run() {
  const app = await launchApp();
  try {
    const page = await connectMainWindow(app.port);

    // 0. 剪贴板监控默认开启（隔离新环境默认值）
    const cfg = await page.invoke("get_config");
    assert(cfg.clipboard_monitor_enabled === true, "新环境的剪贴板监控应为开启");

    // 1. 写入合成文本 → 监控器拾取 → 翻译 → 落历史
    setClipboard(SAMPLE);
    let rec = await waitForHistoryRecord(page, SAMPLE, { timeoutMs: 25000 });

    // 2. 冷启动首请求可能因网络栈未就绪失败：换一段文本重试（相同文本
    //    会被监控器去重，真实用户此时也会复制新的内容）
    if (!rec) {
      setClipboard(SAMPLE2);
      rec = await waitForHistoryRecord(page, SAMPLE2, { timeoutMs: 25000 });
    }
    assert(rec, "两次写入后历史里仍无记录 —— 剪贴板主链路异常");

    // 3. 来源必须标记为 clipboard；译文非空
    assert(rec.source === "clipboard", `来源应为 clipboard，实际: ${rec.source}`);
    assert(
      typeof rec.translated_text === "string" && rec.translated_text.length > 0,
      `译文应为非空字符串，实际: ${JSON.stringify(rec.translated_text).slice(0, 80)}`
    );
  } finally {
    await shutdown(app);
  }
}
