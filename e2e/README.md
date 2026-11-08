# QuickTranslate E2E 联调测试

对真实构建的二进制做端到端联调：真实 IPC（`__TAURI_INTERNALS__.invoke`）、
真实 SQLite（隔离目录）、真实翻译链路（Google 免费源 + 合成文本）、真实
剪贴板监控。不 mock 任何后端。

## 运行前提

```bash
# 1. 构建嵌入了前端产物（dist）的调试二进制
npx tauri build --debug --no-bundle

# 2. 退出正在运行的 QuickTranslate（托盘退出）——
#    单实例插件会让第二个实例直接退出，e2e 无法启动
```

## 运行

```bash
node e2e/run.mjs              # 全部场景
node e2e/run.mjs smoke        # 指定场景
```

## 隔离保证

- `QUICKTRANSLATE_APP_DATA_DIR=<临时目录>`：SQLite/日志/密钥/配置全部
  落在临时目录，跑完即删（`smoke` 可用 `keepDir` 排查现场）
- WebView2 用户数据（EBWebView）同样重定向，不会误删真实用户的缓存
- **例外**：`clipboard-translate` 场景会写真实系统剪贴板（合成文本）——
  剪贴板监控的产品语义决定了它必须在真实通道上验证

## 场景清单

| 场景 | 覆盖 |
| --- | --- |
| `smoke` | 隔离启动、数据落点、基础 IPC、设置窗口打开、诊断可读 |
| `manual-translate` | hash 路由切窗、文本输入、真实翻译、manual 来源落历史 |
| `glossary-crud` | 术语表新增/列表/删除确认守卫/确认删除，真实 SQLite |
| `settings-apply` | 开关即时持久化、就近状态提示、开关可访问名称定位 |
| `clipboard-translate` | 剪贴板监控 → 浮窗 → 落历史（核心主链路） |

## 已知边界

- `manual-translate` 与 `clipboard-translate` 依赖 Google 免费源可达；
  网络不可用时这两条会失败（环境问题，报告中单独标注）
- 快捷键的 OS 级注册冲突提示、托盘菜单点击、更新器不在 e2e 范围
  （需要真实的系统级交互或签名凭证）
