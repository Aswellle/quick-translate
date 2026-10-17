// src/lib/export.ts
// 历史记录导出格式化：纯函数，独立于组件便于测试。
// 从 HistoryWindow.tsx 抽出 —— markdown/html/json 三套模板与 null 处理
// 都是纯逻辑，埋在组件文件里没有任何测试触达（HTML 版因此漏过一个
// null 耗时渲染成 "null ms" 的真实 bug）。

export type ExportRecord = {
  id: string;
  source_text: string;
  translated_text: string;
  source_lang: string;
  target_lang: string;
  provider: string;
  created_at: number;
  duration_ms: number | null;
  is_starred: boolean;
};

export function formatAsMarkdown(records: ExportRecord[]): string {
  const date = new Date().toLocaleString("zh-CN");
  const lines: string[] = [
    `# QuickTranslate 翻译历史`,
    ``,
    `> 导出时间：${date}　共 ${records.length} 条记录`,
    ``,
    `---`,
    ``,
  ];
  for (const r of records) {
    const ts = new Date(r.created_at).toLocaleString("zh-CN");
    const star = r.is_starred ? " ⭐" : "";
    lines.push(`## ${r.source_lang} → ${r.target_lang}　\`${r.provider}\`${star}`);
    lines.push(``);
    lines.push(`**原文**`);
    lines.push(``);
    lines.push(r.source_text);
    lines.push(``);
    lines.push(`**译文**`);
    lines.push(``);
    lines.push(r.translated_text);
    lines.push(``);
    lines.push(`_${ts}${r.duration_ms != null ? `　耗时 ${r.duration_ms} ms` : ""}_`);
    lines.push(``);
    lines.push(`---`);
    lines.push(``);
  }
  return lines.join("\n");
}

export function formatAsHtml(records: ExportRecord[]): string {
  const esc = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const date = new Date().toLocaleString("zh-CN");
  const rows = records
    .map((r) => {
      const ts = new Date(r.created_at).toLocaleString("zh-CN");
      return `
    <tr>
      <td>${esc(r.source_text)}</td>
      <td>${esc(r.translated_text)}</td>
      <td>${esc(r.source_lang)} → ${esc(r.target_lang)}</td>
      <td>${esc(r.provider)}</td>
      <td>${r.is_starred ? "⭐" : ""}</td>
      <td>${esc(ts)}</td>
      <td>${r.duration_ms != null ? `${r.duration_ms} ms` : ""}</td>
    </tr>`;
    })
    .join("\n");

  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<title>QuickTranslate 翻译历史</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; padding: 24px; color: #1a1a1a; }
  h1 { font-size: 20px; margin-bottom: 4px; }
  p.meta { color: #888; font-size: 13px; margin-bottom: 20px; }
  table { border-collapse: collapse; width: 100%; font-size: 13px; }
  th { background: #f5f5f5; padding: 8px 12px; text-align: left; border-bottom: 2px solid #ddd; }
  td { padding: 8px 12px; border-bottom: 1px solid #eee; vertical-align: top; white-space: pre-wrap; max-width: 320px; }
  tr:hover td { background: #fafafa; }
</style>
</head>
<body>
<h1>QuickTranslate 翻译历史</h1>
<p class="meta">导出时间：${esc(date)}　共 ${records.length} 条记录</p>
<table>
  <thead>
    <tr>
      <th>原文</th>
      <th>译文</th>
      <th>语言</th>
      <th>引擎</th>
      <th>收藏</th>
      <th>时间</th>
      <th>耗时</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table>
</body>
</html>`;
}
