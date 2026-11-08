// e2e/harness.mjs
// QuickTranslate e2e 联调 harness：启动带隔离数据目录与 CDP 调试端口的
// 应用实例，通过 Chrome DevTools Protocol 驱动各窗口的 DOM 与 IPC。
//
// 前置条件：
// 1. `npx tauri build --debug --no-bundle`（前端 dist 嵌入 debug 二进制）
// 2. 没有正在运行的 QuickTranslate 实例（单实例插件会让第二个实例退出）
//
// 隔离：QUICKTRANSLATE_APP_DATA_DIR 指向临时目录 —— SQLite/日志/密钥/
// 配置与 WebView2 用户数据全部落在其中，测试绝不触碰真实用户数据。

import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, existsSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
export const APP_EXE = join(HERE, "..", "src-tauri", "target", "debug", "quicktranslate.exe");

export function assert(cond, message) {
  if (!cond) throw new Error(`断言失败: ${message}`);
}

export function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

/** 前置检查：二进制存在、没有正在运行的同名实例（单实例插件会吞掉第二个实例） */
function preflight() {
  if (!existsSync(APP_EXE)) {
    throw new Error(`找不到应用二进制: ${APP_EXE}\n先运行: npx tauri build --debug --no-bundle`);
  }
  const out = execFileSync("tasklist", ["/FI", "IMAGENAME eq quicktranslate.exe", "/FO", "CSV", "/NH"], {
    encoding: "utf8",
  });
  if (out.includes("quicktranslate.exe")) {
    throw new Error(
      "检测到正在运行的 QuickTranslate 实例 —— 单实例插件会让 e2e 实例直接退出。请先退出托盘里的应用再跑 e2e。"
    );
  }
}

/** 启动应用实例并等待 CDP 端点就绪 */
export async function launchApp({ port = 9343 } = {}) {
  preflight();
  const dataDir = mkdtempSync(join(tmpdir(), "qt-e2e-"));
  const child = spawn(APP_EXE, [], {
    env: {
      ...process.env,
      QUICKTRANSLATE_APP_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: "ignore",
  });

  const exited = new Promise((resolve) => child.on("exit", (code) => resolve(code)));
  const targets = await waitForTargets(port, exited, child);
  return { child, dataDir, port, targets };
}

/** 轮询 /json/list 直到出现 webview 目标；进程提前退出则立即报错 */
async function waitForTargets(port, exited, child, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const hasExited = await Promise.race([
      exited.then(() => true),
      sleep(0).then(() => false),
    ]);
    if (hasExited) {
      throw new Error("应用进程提前退出（检查数据目录与端口占用；或已有实例在运行）");
    }
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/list`);
      const list = (await res.json()).filter((t) => t.webSocketDebuggerUrl);
      if (list.length > 0) return list;
    } catch {
      // 端口未就绪，继续轮询
    }
    await sleep(300);
  }
  child.kill();
  throw new Error(`等待 CDP 端点超时（端口 ${port}）`);
}

/** 按 URL 片段（如 "#settings"）挑窗口目标 */
export function findTarget(targets, fragment) {
  return targets.find((t) => t.url.includes(fragment));
}

/** 重新拉取目标列表（窗口是运行中创建的，初始列表里没有） */
export async function refreshTargets(port) {
  const res = await fetch(`http://127.0.0.1:${port}/json/list`);
  return (await res.json()).filter((t) => t.webSocketDebuggerUrl);
}

/** 连接一个窗口目标，提供 evaluate / invoke / waitFor 三个原语 */
export async function connectTarget(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let seq = 0;
  const pending = new Map();

  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(JSON.stringify(msg.error)));
      else resolve(msg.result);
    }
  });

  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", () => reject(new Error("WS 连接失败: " + wsUrl)), { once: true });
  });

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });

  await send("Runtime.enable");

  return {
    /** 在页面上下文求值；awaitPromise=true 时等待 Promise */
    async evaluate(expression, { awaitPromise = false } = {}) {
      let r;
      try {
        r = await send("Runtime.evaluate", { expression, awaitPromise, returnByValue: true });
      } catch (err) {
        // 目标可能已关闭（窗口销毁/导航）：带上表达式开头帮助定位
        throw new Error(`CDP evaluate 传输失败: ${String(err).slice(0, 120)} | expr: ${expression.slice(0, 60)}`);
      }
      if (r.exceptionDetails) {
        const d = r.exceptionDetails;
        throw new Error(
          `Runtime.evaluate 异常: ${d.exception?.description ?? d.text ?? JSON.stringify(d).slice(0, 200)} | expr: ${expression.slice(0, 60)}`
        );
      }
      return r.result.value;
    },
    /** 调用 Tauri 命令（真实 IPC，不是 mock） */
    async invoke(cmd, args = {}) {
      return this.evaluate(
        `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`,
        { awaitPromise: true }
      );
    },
    /** 轮询等待页面条件成立 */
    async waitFor(expression, { timeoutMs = 10000, intervalMs = 250 } = {}) {
      const deadline = Date.now() + timeoutMs;
      let last;
      while (Date.now() < deadline) {
        last = await this.evaluate(expression);
        if (last) return last;
        await sleep(intervalMs);
      }
      throw new Error(`waitFor 超时（${timeoutMs}ms）: ${expression} → ${JSON.stringify(last)}`);
    },
    close() {
      ws.close();
    },
  };
}

/** 连接到指定 URL 片段对应的窗口（自动重拉目标列表） */
export async function connectToWindow(port, fragment, { timeoutMs = 15000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const targets = await refreshTargets(port);
    const target = findTarget(targets, fragment);
    if (target) return await connectTarget(target.webSocketDebuggerUrl);
    await sleep(300);
  }
  throw new Error(`找不到窗口目标: ${fragment}`);
}

/** 连接主窗口（popup 的 webview）。向导是独立窗口（#onboarding）——
 *  连接时必须避开；若向导还在，先在它自己的目标上点「暂时跳过」
 *  （走真实的 complete_onboarding 持久化路径）再连主窗口。 */
export async function connectMainWindow(port, { timeoutMs = 20000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const targets = await refreshTargets(port);
    const main = targets.find(
      (t) => t.url.includes('index.html') && !t.url.includes('#onboarding')
    );
    if (main) return await connectTarget(main.webSocketDebuggerUrl);
    const wizard = targets.find((t) => t.url.includes('#onboarding'));
    if (wizard) {
      // 向导两步走：第 1 步只有「开始配置」，第 2 步才有「暂时跳过」。
      // 走真实的 complete_onboarding 持久化路径，向导自关后 popup 由
      // ensure_popup_window 创建（onboarding 未完成时 startup 不建 popup）。
      const w = await connectTarget(wizard.webSocketDebuggerUrl);
      await w.evaluate(String.raw`(() => {
        const byText = (t) =>
          [...document.querySelectorAll('button')].find((b) =>
            (b.textContent ?? '').trim().includes(t)
          );
        byText('暂时跳过')?.click() ?? byText('开始配置')?.click();
      })()`);
      w.close();
      await sleep(1000);
    }
    await sleep(300);
  }
  throw new Error('找不到主窗口目标（index.html 且非 onboarding）');
}
/** 新环境（隔离目录）首次启动必走 onboarding 向导，它会抢占 hash 路由
 *  —— 需要 hash 导航的场景先点「跳过」完成向导。返回是否跳过了。 */
export async function skipOnboardingIfShowing(page) {
  const hasWizard = await page.evaluate(
    `document.body.textContent.includes("欢迎使用 QuickTranslate")`
  );
  if (!hasWizard) return false;
  await page.evaluate(`(() => {
    const btn = [...document.querySelectorAll("button")].find((b) =>
      (b.textContent ?? "").includes("暂时跳过")
    );
    btn?.click();
  })()`);
  await page.waitFor(
    `!document.body.textContent.includes("欢迎使用 QuickTranslate")`,
    { timeoutMs: 8000 }
  );
  return true;
}

/** 读取隔离目录里的文件列表（校验 SQLite 等确实落在隔离目录） */
export function isolatedFiles(dataDir) {
  const walk = (dir, prefix = "") =>
    readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
      e.isDirectory() ? walk(join(dir, e.name), `${prefix}${e.name}/`) : [`${prefix}${e.name}`]
    );
  return existsSync(dataDir) ? walk(dataDir) : [];
}

/** 关闭应用并清理隔离目录（keepDir=true 时保留现场供排查）。
 *  进程树击杀（WebView2 子进程会继承数据目录的文件锁）+ 删除重试。 */
export async function shutdown(app, { keepDir = false } = {}) {
  try {
    execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // 进程可能已退出
  }
  await sleep(400);
  if (keepDir) return;
  for (let i = 0; i < 10; i++) {
    try {
      rmSync(app.dataDir, { recursive: true, force: true });
      return;
    } catch {
      // WebView2 子进程可能还在释放文件锁
      await sleep(300);
    }
  }
  console.warn(`    （隔离目录未能删除，保留现场: ${app.dataDir}）`);
}
