// 验证脚本：e2e 启动的隔离实例必须跳过更新检查（updater_skipped_isolated）。
import { launchApp, connectMainWindow, shutdown, sleep } from "./harness.mjs";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const app = await launchApp();
try {
  const page = await connectMainWindow(app.port);
  // 启动 5s 后的静默检查此时已触发（或已被 guard 拦截），等一拍再读日志
  await sleep(7000);
  const logsDir = join(app.dataDir, "logs");
  const all = readdirSync(logsDir).flatMap((f) =>
    readFileSync(join(logsDir, f), "utf8").split("\n")
  );
  const skipped = all.filter((l) => l.includes("updater_skipped_isolated"));
  const updaterLines = all.filter((l) => /updater|latest\.json|检查更新/i.test(l));
  console.log("updater_skipped_isolated 命中:", skipped.length);
  console.log(updaterLines.slice(0, 3).join("\n"));
  if (skipped.length === 0) {
    console.log("FAIL: 隔离实例仍执行了更新检查");
    process.exit(1);
  }
  console.log("PASS: 隔离实例的更新检查已被拦截");
} finally {
  await shutdown(app);
}
