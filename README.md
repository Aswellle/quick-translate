<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="720">

# QuickTranslate

### 复制，就是翻译。

**看到不认识的英文，`Ctrl+C` 一下。译文就浮在你光标旁边。**

没有切窗口，没有开网页，没有粘贴框。手都不用离开键盘。

[![下载](https://img.shields.io/badge/⬇_立即下载-Windows_x64-2563eb?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)
[![版本](https://img.shields.io/github/v/release/Aswellle/quick-translate?style=for-the-badge&label=版本&color=555)](https://github.com/Aswellle/quick-translate/releases)
[![体积](https://img.shields.io/badge/安装包-5MB-555?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)

</div>

---

## 你有没有经历过这个

你在看一份英文文档。遇到一句读不懂的。

于是你切到浏览器，找到翻译网站那个标签页（如果还没被你关掉），点进输入框，`Ctrl+V`，等它加载，读完译文，再切回文档 —— 然后花三秒钟找回刚才读到哪一行了。

一次十几秒。听着不多。但一份文档下来你要做几十次，**真正被消耗掉的不是时间，是你刚刚建立起来的那点专注**。每一次切换，思路都得重新接上。

QuickTranslate 就是为了删掉这十几秒而存在的。

**复制。译文出现在光标旁边。看完按空格关掉。继续读。**

没有第四步。

---

## 它长在哪里，就在哪里工作

它不挑应用。**任何能复制文字的地方，它都能用。**

**看技术文档** — VS Code 里的报错堆栈、MDN、GitHub Issues。复制那句看不懂的，扫一眼译文，`Esc` 关掉，视线都没离开屏幕中央。

**读英文论文** — Adobe Reader、Zotero、浏览器内嵌 PDF 都一样。选中一段绕口的摘要，复制，译文就在旁边。PDF 里那些烦人的断行，它会自动接好再翻。

**处理外文邮件** — Outlook、企业微信、Slack 里逐段复制，浮窗依次弹出。一整封邮件读完，你没离开过邮件客户端。

**刷外文内容** — YouTube 视频标题、Twitter 推文、海外新闻。遇到不认识的俚语复制一下，顺手就存进了历史记录，晚上还能翻回来看。

**日常搬砖** — 终端里的编译错误、Figma 里的英文术语、Jira 里的需求描述。都一样，复制即翻译。

---

## 三个让你用得下去的细节

产品好不好用，往往不在功能列表里，在这些地方：

**浮窗不会挡住你正在读的那句话。** 它会算光标四周哪边空间大，往那边放。屏幕边缘也不会被截掉一半。多显示器、150% 或 200% 缩放都试过 —— 位置该在哪就在哪。

**关掉它只需要动一动手指。** 空格键。也可以按 `Esc`，或者点浮窗外面任何地方。左上角还有三颗仿macOS的红绿灯 —— 红色关闭，黄色折叠成一条（想留着待会儿再看），绿色把窗口拉宽为文本阅读视口（长文本更好读）。

**翻译源坏了它自己会换。** 配了多个源的话，主力挂了会按顺序自动切下一个，你根本不会察觉。不用等报错，不用手动切。

还有些不用你操心的：译文和原文一键复制、历史记录本地存着可以搜可以收藏、API Key 加密落盘、深浅色跟随系统、新版本后台静默装好。

---

## 五个翻译源，先用哪个都行

| 翻译源 | 要配置吗 | 免费额度 |
|:--|:--|:--|
| **Google 翻译** | **不用，装完就能用** | 免费（非官方接口） |
| **DeepL** | 一个 API Key | 50 万字符 / 月 |
| **腾讯翻译君** | SecretId + Key | 500 万字符 / 月 |
| **百度翻译** | AppId + 密钥 | 100 万字符 / 月 |
| **有道翻译** | 应用 ID + 密钥 | 有免费体验额度 |

**懒人路线**：装完，引导页点「暂时跳过」，立刻开始用 Google 源。

**讲究一点**：DeepL 的中文质量确实更好，去官网注册拿个免费 Key（大概用一分钟），配置上Key就可以用中文效果更好的DeepL翻译源。还有腾讯的免费额度大到日常用不完。

翻译源随时能在设置里切换，配几个当备用回退也好。支持13种语言互译，目标语言默认中文。

---

## 三步开始用

**1. 下载安装**

去 [Releases 页面](https://github.com/Aswellle/quick-translate/releases/latest) 下载`.msi` 按安装向导提示安装。小巧的文件体积只有5MB，装完打开配置翻译源Key即用。

> Windows10/11（x64）。如果提示缺WebView2运行时，安装包会自己引导你装——Win11一般已经自带了。

**2. 走完引导（半分钟）**

选个目标语言，翻译源直接点「暂时跳过」也完全能直接用谷歌翻译源。

**3. 试一下**

随便找段英文，`Ctrl+C`。

浮窗出现了 —— 那就成了。看完按空格退出翻译弹窗。

之后它就安静地待在系统托盘里。右键图标可以打开设置、翻历史、或者临时关掉监控（比如你要复制一堆密码的时候）。

---

## 关于自动更新

装好之后你基本不用再管它。启动几秒后会静默查一次更新，有新版就自己下好装上，只弹个小提示通知你。

> **如果先用上了v0.2.0**：那个版本的更新地址写错了（一个连字符的锅），自动更新连不上。请去 [Releases](https://github.com/Aswellle/quick-translate/releases/latest) 手动下载一次，之后就正常了。给你添麻烦了。

---

## 常见问题

**复制了但浮窗没出来？**
右键托盘图标看看剪贴板监控是不是被关了。另外提醒一句：很多终端里 `Ctrl+C` 是中断命令而不是复制 —— 在 Windows Terminal 里需要先选中文字才算复制。

**能不能改成快捷键触发，别自动弹？**
目前是复制即触发。不想被打扰的时候，托盘菜单里一键关掉监控就行。

**它会偷偷传我的东西吗？**
只有你复制的那段文字会发给你自己配置的翻译服务商（这是翻译的必要条件）。历史记录和 API Key 都在本地，Key 是加密存的。

**占多少资源？**
空闲时内存 50MB 以内，CPU 基本是 0。安装包 5MB。

---

## 想要什么功能

有想法、遇到 bug，都欢迎开 [Issue](https://github.com/Aswellle/quick-translate/issues) 说一声。

正在琢磨的：浮窗磨砂玻璃效果、历史记录导出、快捷键冲突检测。

---

<div align="center">

**如果它帮你省下了那些十几秒，点个 ⭐ 让更多人看到。**

[⬇ 下载最新版](https://github.com/Aswellle/quick-translate/releases/latest) · [🐛 报告问题](https://github.com/Aswellle/quick-translate/issues) · [📋 更新日志](https://github.com/Aswellle/quick-translate/releases)

<sub>QuickTranslate · 专有软件，版权所有 © 2026 Aswellle · 仅供个人使用</sub>

</div>
