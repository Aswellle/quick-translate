// e2e/run.mjs
// 场景运行器：顺序执行全部场景，收集结果，非零退出码表示有失败。
// 用法：node e2e/run.mjs [场景名 ...]（不带参数 = 全部）

import { launchApp, shutdown } from "./harness.mjs";
import { run as smoke } from "./scenarios/smoke.mjs";
import { run as manualTranslate } from "./scenarios/manual-translate.mjs";
import { run as glossaryCrud } from "./scenarios/glossary-crud.mjs";
import { run as settingsApply } from "./scenarios/settings-apply.mjs";
import { run as clipboardTranslate } from "./scenarios/clipboard-translate.mjs";

const ALL = {
  smoke,
  "manual-translate": manualTranslate,
  "glossary-crud": glossaryCrud,
  "settings-apply": settingsApply,
  "clipboard-translate": clipboardTranslate,
};

const requested = process.argv.slice(2);
const names = requested.length > 0 ? requested : Object.keys(ALL);
for (const n of requested) {
  if (!ALL[n]) {
    console.error(`未知场景: ${n}（可用: ${Object.keys(ALL).join(", ")}）`);
    process.exit(2);
  }
}

const results = [];
for (const name of names) {
  const started = Date.now();
  process.stdout.write(`▶ ${name} … `);
  try {
    await ALL[name]();
    results.push({ name, ok: true, ms: Date.now() - started });
    console.log(`✓ (${((Date.now() - started) / 1000).toFixed(1)}s)`);
  } catch (err) {
    results.push({ name, ok: false, ms: Date.now() - started });
    console.log(`✗ (${((Date.now() - started) / 1000).toFixed(1)}s)`);
    console.log(`    ${String(err.message ?? err).split("\n").join("\n    ")}`);
  }
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} 场景通过`);
if (failed.length > 0) {
  console.log(`失败: ${failed.map((f) => f.name).join(", ")}`);
  process.exit(1);
}
