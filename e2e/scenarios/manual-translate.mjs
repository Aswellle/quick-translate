// e2e/scenarios/manual-translate.mjs
// 手动翻译端到端：真实 IPC + 真实翻译链路（Google 免费源、合成文本）。
// 验证：hash 路由切到手动窗口 → 文本输入 → 翻译 → 译文渲染 → manual
// 来源落历史。窗口复用 popup 的 webview（hashchange 路由）。

import {
  launchApp,
  connectMainWindow,
  shutdown,
  assert,
  sleep,
} from "../harness.mjs";

const SAMPLE = "Hello, world. This is a smoke test.";

export async function run() {
  const app = await launchApp();
  try {
    const page = await connectMainWindow(app.port);

    // 1. hash 路由切到手动翻译窗口（App.tsx 的 hashchange 监听负责挂载）
    await page.evaluate(`window.location.hash = "#manual"`);
    await page.waitFor(
      `!!document.querySelector("textarea")`,
      { timeoutMs: 10000 }
    );

    // 2. 写入文本并点击翻译
    await page.evaluate(
      `(() => {
        const ta = document.querySelector("textarea");
        const setter = Object.getOwnPropertyDescriptor(
          window.HTMLTextAreaElement.prototype, "value"
        ).set;
        setter.call(ta, ${JSON.stringify(SAMPLE)});
        ta.dispatchEvent(new Event("input", { bubbles: true }));
      })()`
    );
    const clicked = await page.evaluate(
      `(() => {
        const btn = [...document.querySelectorAll("button")].find((b) =>
          /^翻译$/.test((b.textContent ?? "").trim())
        );
        if (!btn || btn.disabled) return false;
        btn.click();
        return true;
      })()`
    );
    assert(clicked, "翻译按钮必须存在且可点击");

    // 3. 译文出现（真实网络调用）。冷启动实测：启动后 1 秒内的首次请求
    //    会因网络栈未就绪失败（DNS/代理暖机），用户重试即成功 —— 场景
    //    按真实用户行为重试一次。
    const clickTranslate = () =>
      page.evaluate(`(() => {
        const btn = [...document.querySelectorAll("button")].find((b) =>
          /^翻译$/.test((b.textContent ?? "").trim())
        );
        if (!btn || btn.disabled) return false;
        btn.click();
        return true;
      })()`);
    let translated = false;
    for (let attempt = 0; attempt < 3 && !translated; attempt++) {
      try {
        await page.waitFor(`document.body.textContent.includes("复制译文")`, {
          timeoutMs: 12000,
        });
        translated = true;
      } catch {
        if (attempt < 2) await clickTranslate();
      }
    }
    assert(translated, "重试后译文仍未出现（网络不可达或链路异常）");

    // 4. 落历史：来源为 manual
    await sleep(1500); // 落盘 worker 有队列延迟
    const history = await page.invoke("query_history", {
      params: { limit: 5, offset: 0 },
    });
    assert(Array.isArray(history) && history.length >= 1, "历史里应有记录");
    const rec = history[0];
    const src = rec.source;
    assert(src === "manual", `来源应为 manual，实际: ${src}`);

    // 5. 回到 popup（hash 路由可逆）
    await page.evaluate(`window.location.hash = "#popup"`);
    await page.waitFor(`!document.querySelector("textarea")`, { timeoutMs: 5000 });
  } finally {
    await shutdown(app);
  }
}
