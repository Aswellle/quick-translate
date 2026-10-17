// src/windows/settings/AboutTab.tsx
// 「关于」页：产品信息 + 应用内更新体验 + 开源署名。
//
// 更新流程用 @tauri-apps/plugin-updater 的 JS API 直连（capabilities 已
// 放行 updater:allow-check / updater:allow-download-and-install）：
//   检查更新 → 「当前已为最新版，无需升级」或「检查到新版本 <版本号>」
//   → 按钮变「立即下载 vX.Y.Z」→ 可视化下载进度 → 「立即安装」
//   → 退出旧版进程进入安装器升级。
// 后台启动时的静默检查（system::updater）保持不变，两者共用同一
// endpoint 与签名校验。

import { useCallback, useEffect, useRef, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import type { Update, DownloadEvent } from "@tauri-apps/plugin-updater";
import appIcon from "@/assets/app-icon.png";
import { openUrl, getAppVersion } from "@/lib/commands";

const REPO_URL = "https://github.com/Aswellle/quick-translate";

type UpdatePhase =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "latest" }
  | { state: "available"; version: string }
  | { state: "downloading"; version: string; received: number; total: number | null }
  | { state: "ready"; version: string }
  | { state: "error"; message: string };

export function AboutTab() {
  const [version, setVersion] = useState<string>("");
  const [phase, setPhase] = useState<UpdatePhase>({ state: "idle" });
  // Update 资源必须 close()，否则底层句柄泄漏；ref 保证跨回调可达
  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    getAppVersion().then(setVersion).catch(() => setVersion(""));
    return () => {
      // 组件卸载时释放未消费的 Update 资源
      updateRef.current?.close().catch(() => {});
      updateRef.current = null;
    };
  }, []);

  const handleCheck = useCallback(async () => {
    setPhase({ state: "checking" });
    try {
      const update = await check();
      if (update) {
        updateRef.current = update;
        setPhase({ state: "available", version: update.version });
      } else {
        setPhase({ state: "latest" });
      }
    } catch (e) {
      setPhase({
        state: "error",
        message: "检查更新失败，请检查网络后重试",
      });
      console.error("[About] 检查更新失败:", e);
    }
  }, []);

  const handleDownload = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase({
      state: "downloading",
      version: update.version,
      received: 0,
      total: null,
    });
    try {
      await update.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === "Started") {
          setPhase((p) =>
            p.state === "downloading"
              ? { ...p, total: event.data.contentLength ?? null }
              : p
          );
        } else if (event.event === "Progress") {
          setPhase((p) =>
            p.state === "downloading"
              ? { ...p, received: p.received + event.data.chunkLength }
              : p
          );
        } else if (event.event === "Finished") {
          setPhase({ state: "ready", version: update.version });
        }
      });
      // downloadAndInstall 正常返回且 Finished 事件已到：进入待安装态。
      // （install 模式为 passive：安装器自行接管，无需额外动作）
      setPhase((p) =>
        p.state === "downloading" ? { state: "ready", version: update.version } : p
      );
    } catch (e) {
      console.error("[About] 下载更新失败:", e);
      setPhase({ state: "error", message: "下载更新失败，请检查网络后重试" });
    }
  }, []);

  const handleInstall = useCallback(() => {
    // 重启进安装器：退出当前应用，已下载的安装包由 updater 插件接管执行
    // （plugin 在 install 时已写入临时目录并调度升级命令）
    const update = updateRef.current;
    if (!update) return;
    update
      .install()
      .catch((e) => {
        console.error("[About] 安装失败:", e);
        setPhase({ state: "error", message: "安装失败，请重试或手动下载更新" });
      });
    updateRef.current = null;
  }, []);

  const fmtBytes = (n: number) =>
    n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`;

  const progress =
    phase.state === "downloading" && phase.total
      ? Math.min(100, Math.round((phase.received / phase.total) * 100))
      : null;

  return (
    <div className="space-y-4 max-w-lg mx-auto">
      {/* ── 产品标识 ── */}
      <div className="flex flex-col items-center gap-2 pt-4 pb-2 text-center">
        <img
          src={appIcon}
          alt="QuickTranslate"
          className="w-16 h-16 rounded-2xl shadow-macos-lg"
          draggable={false}
        />
        <h2 className="text-[17px] font-semibold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>
          QuickTranslate
        </h2>
        <p className="text-[12px] text-[var(--text-secondary)] tabular-nums">
          {version ? `版本 ${version}` : ""}
        </p>
        <p className="text-[11.5px] text-[var(--text-secondary)]">
          复制即翻译的桌面翻译工具 · 数据全程本地处理
        </p>
      </div>

      {/* ── 更新卡片 ── */}
      <div className="macos-card divide-y divide-[var(--border-secondary)] overflow-hidden">
        <div className="px-4 py-3.5">
          <div className="flex items-center justify-between gap-4">
            <div className="flex-1 min-w-0">
              <p className="text-[13px] font-medium text-[var(--text-primary)]">软件更新</p>
              <StatusLine phase={phase} />
            </div>
            <MainUpdateButton phase={phase} onCheck={handleCheck} onDownload={handleDownload} onInstall={handleInstall} />
          </div>
          {/* 下载进度条 */}
          {phase.state === "downloading" && (
            <div className="mt-3 space-y-1.5">
              <div className="h-1.5 rounded-full bg-[var(--surface-tertiary)] overflow-hidden">
                <div
                  className="h-full rounded-full bg-[var(--system-blue)] transition-[width] duration-200"
                  style={{ width: `${progress ?? 4}%` }}
                />
              </div>
              <p className="text-[11px] text-[var(--text-secondary)] tabular-nums text-right">
                {fmtBytes(phase.received)}
                {phase.total ? ` / ${fmtBytes(phase.total)}` : ""}
              </p>
            </div>
          )}
        </div>
      </div>

      {/* ── 开源署名 ── */}
      <div className="macos-card px-4 py-3.5 flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[12.5px] font-medium text-[var(--text-primary)]">
            QuickTranslate 是开源软件
          </p>
          <p className="text-[11.5px] text-[var(--text-secondary)] mt-0.5 leading-snug">
            基于 MIT 许可证发布。如果它对你有帮助，欢迎到项目仓库点一个 Star
            —— 这是开源作者最实在的鼓励，也能让更多人发现这个工具。
          </p>
        </div>
        <a
          href="#github"
          onClick={(e) => {
            e.preventDefault();
            openUrl(REPO_URL).catch(console.error);
          }}
          title="前往 GitHub 开源仓库"
          aria-label="前往 GitHub 开源仓库"
          className="shrink-0 w-10 h-10 rounded-full bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] flex items-center justify-center hover:bg-[var(--hover-bg)] active:scale-95 transition-all"
        >
          <GitHubMark className="w-6 h-6 text-[var(--text-primary)]" />
        </a>
      </div>
    </div>
  );
}

function StatusLine({ phase }: { phase: UpdatePhase }) {
  const base = "text-[11.5px] mt-0.5 leading-snug";
  switch (phase.state) {
    case "idle":
      return <p className={`${base} text-[var(--text-secondary)]`}>检查是否有可用的新版本</p>;
    case "checking":
      return <p className={`${base} text-[var(--text-secondary)]`}>正在检查更新…</p>;
    case "latest":
      return <p className={`${base} text-green-600 dark:text-green-500`}>当前已为最新版，无需升级</p>;
    case "available":
      return <p className={`${base} text-[var(--system-blue)] font-medium`}>检查到新版本 {phase.version}</p>;
    case "downloading":
      return <p className={`${base} text-[var(--text-secondary)]`}>正在下载 {phase.version}…</p>;
    case "ready":
      return (
        <p className={`${base} text-green-600 dark:text-green-500`}>
          {phase.version} 已就绪，点击「立即安装」完成升级
        </p>
      );
    case "error":
      return <p className={`${base} text-red-500`}>{phase.message}</p>;
  }
}

function MainUpdateButton({
  phase,
  onCheck,
  onDownload,
  onInstall,
}: {
  phase: UpdatePhase;
  onCheck: () => void;
  onDownload: () => void;
  onInstall: () => void;
}) {
  const busy = phase.state === "checking" || phase.state === "downloading";
  const label =
    phase.state === "downloading"
      ? "下载中…"
      : phase.state === "ready"
        ? "立即安装"
        : phase.state === "available"
          ? `立即下载 ${phase.version}`
          : busy
            ? "检查中…"
            : "检查更新";
  const onClick = () => {
    if (phase.state === "ready") onInstall();
    else if (phase.state === "available") onDownload();
    else if (!busy) onCheck();
  };
  const accent =
    phase.state === "ready" || phase.state === "available"
      ? "bg-[var(--system-blue)] text-white hover:brightness-110 shadow-sm"
      : "border border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)]";

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy}
      className={[
        "shrink-0 text-[12.5px] font-medium px-3.5 py-1.5 rounded-lg transition-all active:scale-95 disabled:opacity-60 disabled:cursor-not-allowed",
        accent,
      ].join(" ")}
    >
      {busy && (
        <span className="inline-block w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin align-[-2px] mr-1.5" />
      )}
      {label}
    </button>
  );
}

/** GitHub 章鱼猫（Octicon mark-github 路径） */
function GitHubMark({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}
