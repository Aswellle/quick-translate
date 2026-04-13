# Tauri Updater 签名密钥配置指南

## 一、生成签名密钥对

在项目根目录执行（需已安装 Tauri CLI）：

```bash
npm run tauri signer generate -- -w ~/.tauri/quicktranslate.key
```

执行后会输出：
```
Private key saved to: ~/.tauri/quicktranslate.key
Public key: dW50cnVzdGVkIGNvbW1lbnQ6IG1...（base64 字符串）
```

将公钥字符串复制备用。

## 二、配置 tauri.conf.json

将生成的公钥替换 `PLACEHOLDER_REPLACE_WITH_YOUR_PUBKEY`，
并将 `YOUR_GITHUB_USERNAME` 替换为你的 GitHub 用户名：

```json
"plugins": {
  "updater": {
    "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6...",
    "endpoints": [
      "https://github.com/YOUR_GITHUB_USERNAME/quicktranslate/releases/latest/download/latest.json"
    ],
    "dialog": false,
    "windows": { "installMode": "passive" }
  }
}
```

## 三、配置 GitHub Secrets

GitHub 仓库 → Settings → Secrets and variables → Actions，新增：

| Secret 名称 | 值来源 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | `cat ~/.tauri/quicktranslate.key` 的全文 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 生成时输入的密码（留空则填空字符串） |

可选（macOS 代码签名）：

| Secret 名称 | 说明 |
|---|---|
| `MACOS_CERTIFICATE` | Base64 编码的 .p12 证书 |
| `MACOS_CERTIFICATE_PWD` | 证书密码 |
| `MACOS_KEYCHAIN_PWD` | 临时 keychain 密码（任意字符串） |
| `APPLE_ID` | Apple Developer 账号邮箱 |
| `APPLE_PASSWORD` | App-specific password |
| `APPLE_TEAM_ID` | Team ID |

## 四、发布新版本

```bash
# 1. 更新版本号（src-tauri/tauri.conf.json 中的 version）
# 2. 提交更改
git add -A && git commit -m "chore: bump version to 0.2.0"
# 3. 打 tag 并推送（自动触发 CI/CD）
git tag v0.2.0
git push origin v0.2.0
```

Actions 将自动在三个平台构建 → 签名 → 发布 → 生成 latest.json。

## 五、测试更新检测

- 将 `tauri.conf.json` 的 `version` 临时改为低版本（如 `0.0.1`）
- 运行 `npm run tauri dev`
- 启动后约 3 秒会出现更新提示 Toast
