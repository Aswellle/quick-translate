// src/windows/settings/GlossaryTab.tsx
// 术语表管理（计划 Task 10）：按语言对管理本地术语，如实展示当前翻译源
// 的原生 glossary 能力。
//
// 应用边界（见 docs/GLOSSARY_CAPABILITY_ASSESSMENT.md）：
// - 只有 DeepL 会在翻译时应用本地术语表，且要求显式源语言；
// - 其他 Provider 只能引用各自控制台的术语库或完全不支持 —— 界面如实
//   说明，绝不暗示本地条目会自动生效。

import { useCallback, useEffect, useRef, useState } from "react";
import {
  clearGlossary,
  createGlossaryEntry,
  deleteGlossaryEntry,
  exportGlossaryCsv,
  exportGlossaryJson,
  importGlossaryCsv,
  importGlossaryJson,
  listGlossaryEntries,
  updateGlossaryEntry,
  type AppConfig,
  type AppError,
  type GlossaryEntry,
} from "@/lib/commands";
import { SUPPORTED_LANGUAGES } from "@/lib/constants";
import { toast } from "@/components/ToastManager";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ToggleRow } from "./SettingsControls";

function providerNote(provider: string): { className: string; text: string } {
  switch (provider) {
    case "deepl":
      return {
        className: "text-[var(--text-secondary)]",
        text: "DeepL 支持应用本地术语表：手动翻译时选择具体源语言即生效；自动检测源语言的路径（含剪贴板）不应用术语表。",
      };
    case "tencent":
      return {
        className: "text-[var(--text-secondary)]",
        text: "腾讯云支持引用控制台创建的术语库（请求参数 TermRepoIDList）；术语内容在腾讯云控制台维护，本地条目不会自动同步。",
      };
    case "baidu":
      return {
        className: "text-[var(--text-tertiary)]",
        text: "百度翻译通用接口只能使用控制台的「我的术语库」；本地条目不会自动同步。",
      };
    case "youdao":
      return {
        className: "text-[var(--text-tertiary)]",
        text: "有道智云只能引用控制台上传的术语表；本地条目不会自动同步。",
      };
    case "google":
      return {
        className: "text-[var(--text-tertiary)]",
        text: "当前 Google 翻译接口（v2 Basic）不支持术语表。",
      };
    default:
      return {
        className: "text-[var(--text-tertiary)]",
        text: "当前翻译源不支持术语表。",
      };
  }
}

function errorMessage(err: unknown): string {
  if (typeof err === "object" && err && "message" in err) {
    return String((err as AppError).message);
  }
  return String(err);
}

export function GlossaryTab({
  draft,
  applyConfig,
}: {
  draft: AppConfig;
  applyConfig: (key: keyof AppConfig, value: AppConfig[keyof AppConfig], feedback: "inline" | "toast" | null) => Promise<boolean>;
}) {
  const [sourceLang, setSourceLang] = useState("en");
  const [targetLang, setTargetLang] = useState(
    draft.target_lang === "en" ? "zh" : draft.target_lang
  );
  const [entries, setEntries] = useState<GlossaryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  // 读取失败与空态分离：失败要显示原因与重试入口，不能落回「还没有术语」
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [editingId, setEditingId] = useState<number | null>(null);
  const [sourceTerm, setSourceTerm] = useState("");
  const [targetTerm, setTargetTerm] = useState("");
  const [note, setNote] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  // 破坏性操作守卫：单条删除与整批导入替换都在用户确认后才执行 ——
  // 同一窗口里「清空该语言对」有二次确认，危害相同的这两个入口不能裸奔
  const [pendingDelete, setPendingDelete] = useState<{ id: number; term: string } | null>(null);
  const [pendingImport, setPendingImport] = useState<{ file: File; kind: "json" | "csv" } | null>(null);

  const jsonInputRef = useRef<HTMLInputElement>(null);
  const csvInputRef = useRef<HTMLInputElement>(null);
  const reloadSeqRef = useRef(0);

  const noteInfo = providerNote(draft.provider);
  const pairInvalid = sourceLang === targetLang;

  const reload = useCallback(async () => {
    // 请求序号门：语言对快速切换或 mutation 后的补拉乱序返回时，只让
    // 最新一次的结果生效 —— 旧语言对的迟到列表会盖掉新语言对的条目，
    // 在新语言对标题下渲染文不对题的数据；此时点「保存修改」还会把
    // 旧条目的语言对静默改写成当前对。
    const seq = ++reloadSeqRef.current;
    setLoading(true);
    try {
      const list = await listGlossaryEntries(sourceLang, targetLang);
      if (seq !== reloadSeqRef.current) return;
      setEntries(list);
      setLoadError(null);
    } catch (err) {
      if (seq !== reloadSeqRef.current) return;
      toast("读取术语表失败", "error");
      setLoadError(errorMessage(err));
      setEntries([]);
    } finally {
      if (seq === reloadSeqRef.current) setLoading(false);
    }
  }, [sourceLang, targetLang]);

  useEffect(() => {
    if (pairInvalid) {
      setEntries([]);
      return;
    }
    void reload();
  }, [reload, pairInvalid]);

  const resetForm = () => {
    setEditingId(null);
    setSourceTerm("");
    setTargetTerm("");
    setNote("");
    setCaseSensitive(false);
    setFormError(null);
  };

  const submit = useCallback(async () => {
    if (pairInvalid) {
      setFormError("源语言与目标语言不能相同");
      return;
    }
    setBusy(true);
    setFormError(null);
    const input = {
      source_lang: sourceLang,
      target_lang: targetLang,
      source_term: sourceTerm,
      target_term: targetTerm,
      note,
      case_sensitive: caseSensitive,
    };
    try {
      if (editingId === null) {
        await createGlossaryEntry(input);
        toast("术语已添加", "success");
      } else {
        await updateGlossaryEntry(editingId, input);
        toast("术语已更新", "success");
      }
      resetForm();
      await reload();
    } catch (err) {
      setFormError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [
    pairInvalid,
    sourceLang,
    targetLang,
    sourceTerm,
    targetTerm,
    note,
    caseSensitive,
    editingId,
    reload,
  ]);

  const startEdit = (entry: GlossaryEntry) => {
    setEditingId(entry.id);
    setSourceTerm(entry.source_term);
    setTargetTerm(entry.target_term);
    setNote(entry.note);
    setCaseSensitive(entry.case_sensitive);
    setFormError(null);
  };

  const remove = async (id: number) => {
    try {
      await deleteGlossaryEntry(id);
      await reload();
    } catch (err) {
      toast(`删除失败：${errorMessage(err)}`, "error");
    }
  };

  /** 确认后执行单条删除；取消不产生任何调用 */
  const confirmDelete = () => {
    const target = pendingDelete;
    setPendingDelete(null);
    if (target) void remove(target.id);
  };

  /** 选完文件先确认再替换 —— 导入是整批覆盖，误选旧文件会静默清掉
      当前语言对的全部条目且无法恢复 */
  const importFile = (file: File, kind: "json" | "csv") => {
    setPendingImport({ file, kind });
  };

  const confirmImport = async () => {
    const pending = pendingImport;
    setPendingImport(null);
    if (!pending) return;
    try {
      const content = await pending.file.text();
      const written =
        pending.kind === "json"
          ? await importGlossaryJson(sourceLang, targetLang, content)
          : await importGlossaryCsv(sourceLang, targetLang, content);
      toast(`已导入 ${written} 条术语（替换当前语言对）`, "success");
      await reload();
    } catch (err) {
      toast(`导入失败：${errorMessage(err)}`, "error");
    }
  };

  const clearPair = async () => {
    setConfirmClear(false);
    try {
      const deleted = await clearGlossary(sourceLang, targetLang);
      toast(deleted > 0 ? `已清除 ${deleted} 条术语` : "该语言对没有术语", "success");
      await reload();
    } catch (err) {
      toast(`清除失败：${errorMessage(err)}`, "error");
    }
  };

  const copyExport = async (kind: "json" | "csv") => {
    try {
      const content =
        kind === "json"
          ? await exportGlossaryJson(sourceLang, targetLang)
          : await exportGlossaryCsv(sourceLang, targetLang);
      await navigator.clipboard.writeText(content);
      toast(`已复制 ${kind.toUpperCase()} 到剪贴板`, "success");
    } catch (err) {
      toast(`导出失败：${errorMessage(err)}`, "error");
    }
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <p className="text-[12px] font-semibold text-[var(--text-secondary)] tracking-wide px-1">
          术语表
        </p>
        <div className="macos-card px-3 py-2.5 space-y-2.5">
          <ToggleRow
            label="启用术语表"
            hint="关闭后术语不参与任何翻译请求与缓存键；条目保留"
            value={draft.glossary_enabled}
            onApply={(v) => applyConfig("glossary_enabled", v, "inline")}
          />
          <p className={`text-[11px] leading-relaxed ${noteInfo.className}`}>
            {noteInfo.text}
          </p>
          {draft.provider === "baidu" && (
            <ToggleRow
              label="使用百度控制台术语库"
              hint="请求参数 needIntervene=1；术语在百度控制台「我的术语库」维护，提交后约 10 分钟生效"
              value={draft.baidu_use_term_base}
              onApply={(v) => applyConfig("baidu_use_term_base", v, "inline")}
            />
          )}
          {draft.provider === "tencent" && (
            <ConsoleIdRow
              label="控制台术语库 ID"
              hint="腾讯云控制台创建的术语库 ID，多个用英文逗号分隔（请求参数 TermRepoIDList）；留空不使用。"
              placeholder="repo-id-1, repo-id-2"
              ariaLabel="腾讯云术语库 ID"
              value={draft.tencent_term_repo_ids}
              onCommit={(v) => applyConfig("tencent_term_repo_ids", v, "toast")}
            />
          )}
          {draft.provider === "youdao" && (
            <ConsoleIdRow
              label="控制台术语表 ID"
              hint="有道控制台上传术语表后取得的 out_id（请求参数 vocabId）；留空不使用。"
              placeholder="vocabId"
              ariaLabel="有道术语表 ID"
              value={draft.youdao_vocab_id}
              onCommit={(v) => applyConfig("youdao_vocab_id", v, "toast")}
            />
          )}
        </div>
      </div>

      {/* 语言对选择 */}
      <div className="macos-card px-3 py-2.5 flex items-center gap-2">
        <label className="text-[12px] text-[var(--text-secondary)] shrink-0" htmlFor="glossary-source-lang">
          语言对
        </label>
        <select
          id="glossary-source-lang"
          value={sourceLang}
          onChange={(e) => {
            resetForm();
            // 语言对切换让待确认的删除/导入失效：待导入文件是按旧语言对
            // 选的，确认对话框却展示当前语言对 —— 确认会在新对上执行
            setPendingDelete(null);
            setPendingImport(null);
            setSourceLang(e.target.value);
          }}
          className="input-field text-xs py-1.5 flex-1 min-w-0"
        >
          {SUPPORTED_LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.name}
            </option>
          ))}
        </select>
        <span className="text-[var(--text-tertiary)] text-xs shrink-0">→</span>
        <select
          value={targetLang}
          onChange={(e) => {
            resetForm();
            setPendingDelete(null);
            setPendingImport(null);
            setTargetLang(e.target.value);
          }}
          className="input-field text-xs py-1.5 flex-1 min-w-0"
          aria-label="目标语言"
        >
          {SUPPORTED_LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.name}
            </option>
          ))}
        </select>
      </div>

      {pairInvalid ? (
        <p className="text-[12px] text-red-500 px-1">源语言与目标语言不能相同。</p>
      ) : (
        <>
          {/* 新增 / 编辑 */}
          <div className="macos-card px-3 py-3 space-y-2">
            <div className="flex items-center gap-2">
              <input
                value={sourceTerm}
                onChange={(e) => setSourceTerm(e.target.value)}
                placeholder="原文术语"
                className="input-field text-xs py-1.5 flex-1 min-w-0"
                aria-label="原文术语"
              />
              <span className="text-[var(--text-tertiary)] text-xs shrink-0">→</span>
              <input
                value={targetTerm}
                onChange={(e) => setTargetTerm(e.target.value)}
                placeholder="目标译法"
                className="input-field text-xs py-1.5 flex-1 min-w-0"
                aria-label="目标译法"
              />
            </div>
            <input
              value={note}
              onChange={(e) => setNote(e.target.value)}
              placeholder="说明（可选）"
              className="input-field text-xs py-1.5 w-full"
              aria-label="说明"
            />
            <div className="flex items-center justify-between gap-2">
              <label className="flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)]">
                <input
                  type="checkbox"
                  checked={caseSensitive}
                  onChange={(e) => setCaseSensitive(e.target.checked)}
                />
                区分大小写
              </label>
              <div className="flex items-center gap-2">
                {editingId !== null && (
                  <button
                    type="button"
                    onClick={resetForm}
                    className="text-[12px] px-3 py-1 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] transition-colors text-[var(--text-secondary)]"
                  >
                    取消
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => void submit()}
                  disabled={busy || sourceTerm.trim() === "" || targetTerm.trim() === ""}
                  className="text-[12px] font-medium px-3.5 py-1 rounded-lg bg-[var(--system-blue)] text-white disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                >
                  {editingId === null ? "添加术语" : "保存修改"}
                </button>
              </div>
            </div>
            {formError && <p className="text-[11px] text-red-500">{formError}</p>}
            <p className="text-[10.5px] text-[var(--text-tertiary)] leading-relaxed">
              「区分大小写」随术语保存并参与导出；DeepL 当前按其自身的术语匹配规则处理大小写，
              该选项不会改变服务商行为。
            </p>
          </div>

          {/* 条目列表 */}
          <div className="macos-card divide-y divide-[var(--border-secondary)] overflow-hidden">
            {loading ? (
              <p className="px-3 py-4 text-[12px] text-[var(--text-tertiary)]">正在读取…</p>
            ) : loadError ? (
              /* 读取失败必须与空态分离：toast 一闪而过，若落回「还没有
                 术语」空态，已有 30 条术语的用户会信以为真并重复录入 */
              <div className="px-3 py-4 flex items-center justify-between gap-3">
                <p className="text-[12px] text-[var(--popup-text-primary)] leading-relaxed">
                  术语表读取失败：{loadError}
                </p>
                <button
                  type="button"
                  onClick={() => void reload()}
                  className="px-2.5 py-1 rounded-lg text-[12px] font-medium border border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] transition-colors shrink-0"
                >
                  重试
                </button>
              </div>
            ) : entries.length === 0 ? (
              <p className="px-3 py-4 text-[12px] text-[var(--text-tertiary)]">
                该语言对还没有术语。添加后，DeepL 翻译时按目标译法生效。
              </p>
            ) : (
              entries.map((entry) => (
                <div key={entry.id} className="px-3 py-2 flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="text-[13px] text-[var(--text-primary)] break-words">
                      {entry.source_term} → {entry.target_term}
                      {entry.case_sensitive && (
                        <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-[var(--surface-secondary)] text-[var(--text-tertiary)]">
                          区分大小写
                        </span>
                      )}
                    </div>
                    {entry.note && (
                      <div className="text-[11px] text-[var(--text-tertiary)] mt-0.5 break-words">
                        {entry.note}
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    <button
                      type="button"
                      onClick={() => startEdit(entry)}
                      className="text-[11px] px-2 py-0.5 rounded border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]"
                    >
                      编辑
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setPendingDelete({
                          id: entry.id,
                          term: `${entry.source_term} → ${entry.target_term}`,
                        })
                      }
                      className="text-[11px] px-2 py-0.5 rounded border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-red-500"
                    >
                      删除
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>

          {/* 导入导出与清空 */}
          <div className="macos-card px-3 py-2.5 flex flex-wrap items-center gap-2">
            <input
              ref={jsonInputRef}
              type="file"
              accept=".json,application/json"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) void importFile(file, "json");
              }}
            />
            <input
              ref={csvInputRef}
              type="file"
              accept=".csv,text/csv"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) void importFile(file, "csv");
              }}
            />
            <button type="button" onClick={() => jsonInputRef.current?.click()} className="text-[12px] px-3 py-1 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]">
              导入 JSON
            </button>
            <button type="button" onClick={() => csvInputRef.current?.click()} className="text-[12px] px-3 py-1 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]">
              导入 CSV
            </button>
            <button type="button" onClick={() => void copyExport("json")} className="text-[12px] px-3 py-1 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]">
              导出 JSON
            </button>
            <button type="button" onClick={() => void copyExport("csv")} className="text-[12px] px-3 py-1 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]">
              导出 CSV
            </button>
            <button
              type="button"
              onClick={() => setConfirmClear(true)}
              className="ml-auto text-[12px] px-3 py-1 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] text-red-500"
            >
              清空该语言对
            </button>
          </div>
          <p className="text-[11px] text-[var(--text-tertiary)] px-1">
            导入会替换当前语言对的全部条目（整批校验通过才写入）。导出为系统剪贴板文本，便于备份或迁移。
          </p>
        </>
      )}

      {confirmClear && (
        <ConfirmDialog
          title="清空该语言对的术语？"
          description="将删除当前语言对的全部术语条目，其他语言对不受影响。历史记录与翻译缓存不受影响。"
          confirmLabel="确认清空"
          onConfirm={() => void clearPair()}
          onCancel={() => setConfirmClear(false)}
        />
      )}

      {pendingDelete && (
        <ConfirmDialog
          title="删除这条术语？"
          description={`将删除「${pendingDelete.term}」，删除后无法恢复。`}
          confirmLabel="确认删除"
          onConfirm={confirmDelete}
          onCancel={() => setPendingDelete(null)}
        />
      )}

      {pendingImport && (
        <ConfirmDialog
          title="导入并替换当前语言对的术语？"
          description={`将用所选${pendingImport.kind.toUpperCase()} 文件整批替换 ${sourceLang} → ${targetLang} 的全部条目（当前 ${entries.length} 条），其他语言对不受影响。`}
          confirmLabel="确认导入"
          onConfirm={() => void confirmImport()}
          onCancel={() => setPendingImport(null)}
        />
      )}
    </div>
  );
}

/** 控制台术语库/术语表 ID 输入：失焦提交（与凭证字段同一即时生效模型）。 */
function ConsoleIdRow({
  label,
  hint,
  placeholder,
  ariaLabel,
  value,
  onCommit,
}: {
  label: string;
  hint: string;
  placeholder: string;
  ariaLabel: string;
  value: string;
  onCommit: (v: string) => void;
}) {
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    setDraft(value);
  }, [value]);

  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        <p className="text-[13px] font-medium text-[var(--text-primary)]">{label}</p>
        <p className="text-[11px] text-[var(--text-secondary)] mt-0.5 leading-snug">
          {hint}
        </p>
      </div>
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          if (draft !== value) onCommit(draft.trim());
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          }
        }}
        placeholder={placeholder}
        className="input-field text-xs py-1.5 w-44 shrink-0"
        aria-label={ariaLabel}
        spellCheck={false}
      />
    </div>
  );
}
