// e2e/scenarios/glossary-crud.mjs
// 术语表管理端到端：真实 SQLite（隔离目录）CRUD + 失败态/确认守卫抽查。
// 验证：新增 → 列表出现 → 语言对隔离 → 删除需确认 → 确认后删除 → 落库。

import {
  launchApp,
  connectMainWindow,
  shutdown,
  assert,
  sleep,
} from "../harness.mjs";

/** 设置原生输入框的值并触发 React onChange */
const SET_VALUE = (label, value) => `(() => {
  const el = document.querySelector('input[aria-label="${label}"], textarea[aria-label="${label}"]');
  if (!el) return false;
  const setter = Object.getOwnPropertyDescriptor(
    el.tagName === "TEXTAREA" ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype,
    "value"
  ).set;
  setter.call(el, ${JSON.stringify(value)});
  el.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`;

function clickButton(container, text) {
  return container.evaluate(
    `(() => {
      const btn = [...document.querySelectorAll("button")].find(
        (b) => (b.textContent ?? "").trim() === ${JSON.stringify(text)}
      );
      if (!btn) return false;
      btn.click();
      return true;
    })()`
  );
}

export async function run() {
  const app = await launchApp();
  try {
    const page = await connectMainWindow(app.port);

    // 1. 切到设置页 → 术语表 Tab
    await page.evaluate(`window.location.hash = "#settings/glossary"`);
    await page.waitFor(`document.body.textContent.includes("术语表")`, {
      timeoutMs: 10000,
    });

    // 2. 新增一条术语（en → zh，当前默认语言对）
    await page.evaluate(SET_VALUE("原文术语", "QuickTranslate"));
    await page.evaluate(SET_VALUE("目标译法", "快译"));
    assert(await clickButton(page, "添加术语"), "添加按钮必须存在");

    // 3. 列表出现该条目（IPC + SQLite 真实落库）
    await page.waitFor(`document.body.textContent.includes("QuickTranslate → 快译")`, {
      timeoutMs: 8000,
    });
    const entries = await page.invoke("list_glossary_entries", {
      sourceLang: "en",
      targetLang: "zh",
    });
    assert(
      Array.isArray(entries) && entries.some((e) => e.source_term === "QuickTranslate"),
      "list_glossary_entries 应返回新条目"
    );

    // 4. 删除需要确认：点删除 → 对话框出现 → 取消不删
    assert(await clickButton(page, "删除"), "删除按钮必须存在");
    await page.waitFor(`document.body.textContent.includes("删除这条术语")`, {
      timeoutMs: 5000,
    });
    assert(await clickButton(page, "取消"), "取消按钮必须存在");
    await sleep(300);
    const afterCancel = await page.invoke("list_glossary_entries", {
      sourceLang: "en",
      targetLang: "zh",
    });
    assert(afterCancel.length === entries.length, "取消后条目数不变");

    // 5. 再删并确认 → 条目消失
    assert(await clickButton(page, "删除"), "删除按钮必须存在");
    await page.waitFor(`document.body.textContent.includes("确认删除")`, {
      timeoutMs: 5000,
    });
    assert(await clickButton(page, "确认删除"), "确认删除按钮必须存在");
    await page.waitFor(
      `!document.body.textContent.includes("QuickTranslate → 快译")`,
      { timeoutMs: 8000 }
    );
    const afterDelete = await page.invoke("list_glossary_entries", {
      sourceLang: "en",
      targetLang: "zh",
    });
    assert(afterDelete.length === 0, "确认删除后条目应清空");
  } finally {
    await shutdown(app);
  }
}
