// src/lib/constants.ts

export const APP_NAME = "QuickTranslate";
export const APP_VERSION = "0.1.0";

export const EVENTS = {
  TRANSLATION_LOADING: "translation-loading",
  TRANSLATION_RESULT: "translation-result",
  TRANSLATION_ERROR: "translation-error",
} as const;

export const WINDOWS = {
  POPUP: "popup",
  SETTINGS: "settings",
  HISTORY: "history",
} as const;

export const SUPPORTED_LANGUAGES = [
  { code: "zh",    name: "简体中文",   flag: "🇨🇳" },
  { code: "zh-tw", name: "繁體中文",   flag: "🇹🇼" },
  { code: "en",    name: "English",   flag: "🇺🇸" },
  { code: "ja",    name: "日本語",     flag: "🇯🇵" },
  { code: "ko",    name: "한국어",     flag: "🇰🇷" },
  { code: "fr",    name: "Français",  flag: "🇫🇷" },
  { code: "de",    name: "Deutsch",   flag: "🇩🇪" },
  { code: "es",    name: "Español",   flag: "🇪🇸" },
  { code: "ru",    name: "Русский",   flag: "🇷🇺" },
  { code: "pt",    name: "Português", flag: "🇵🇹" },
  { code: "it",    name: "Italiano",  flag: "🇮🇹" },
  { code: "ar",    name: "العربية",   flag: "🇸🇦" },
] as const;

/** 翻译源完整元数据（用于设置面板与引导向导） */
export const PROVIDERS = [
  {
    id: "tencent",
    name: "腾讯翻译",
    badge: "推荐",
    badgeColor: "bg-blue-500",
    freeQuota: "500 万字符/月",
    quality: "high",
    requiresApiKey: true,
    credentialFields: [
      { key: "tencent_secret_id",  label: "SecretId",  placeholder: "AKIDxxxxxxxx", type: "text" },
      { key: "tencent_secret_key", label: "SecretKey", placeholder: "xxxxxxxx",     type: "password" },
    ],
    setupUrl: "https://console.cloud.tencent.com/cam/capi",
    setupSteps: [
      "登录腾讯云控制台",
      "进入「访问管理 → API 密钥管理」",
      "点击「新建密钥」，复制 SecretId 和 SecretKey",
      "在腾讯云「机器翻译 TMT」控制台开通服务",
    ],
    description: "腾讯云机器翻译，中英互译质量优秀，每月 500 万免费字符",
  },
  {
    id: "deepl",
    name: "DeepL",
    badge: "高质量",
    badgeColor: "bg-purple-500",
    freeQuota: "50 万字符/月",
    quality: "highest",
    requiresApiKey: true,
    credentialFields: [
      { key: "deepl_api_key", label: "API Key", placeholder: "xxxx-xxxx-xxxx:fx", type: "password" },
    ],
    setupUrl: "https://www.deepl.com/pro-api",
    setupSteps: [
      "访问 deepl.com/pro-api",
      "注册免费账号，选择「DeepL API Free」",
      "在账号设置中找到「Authentication Key」",
      "复制 API Key（末尾含 :fx 为 Free 版）",
    ],
    description: "欧美语言翻译质量顶尖，每月 50 万免费字符，超出付费",
  },
  {
    id: "baidu",
    name: "百度翻译",
    badge: "中文优化",
    badgeColor: "bg-red-500",
    freeQuota: "100 万字符/月",
    quality: "high",
    requiresApiKey: true,
    credentialFields: [
      { key: "baidu_app_id",     label: "APP ID",     placeholder: "2015xxxxx",  type: "text" },
      { key: "baidu_secret_key", label: "密钥",       placeholder: "xxxxxxxxxx", type: "password" },
    ],
    setupUrl: "https://fanyi-api.baidu.com/",
    setupSteps: [
      "访问百度翻译开放平台，注册账号",
      "进入「管理控制台 → 通用翻译」",
      "创建应用，获取 APP ID 和密钥",
      "实名认证后每月享有 100 万免费字符",
    ],
    description: "百度翻译，中文语境理解优秀，适合中日韩互译",
  },
  {
    id: "youdao",
    name: "有道翻译",
    badge: "备用",
    badgeColor: "bg-green-500",
    freeQuota: "按量计费",
    quality: "medium",
    requiresApiKey: true,
    credentialFields: [
      { key: "youdao_app_key",    label: "应用 ID",   placeholder: "xxxxxxxxxx", type: "text" },
      { key: "youdao_app_secret", label: "应用密钥", placeholder: "xxxxxxxxxx", type: "password" },
    ],
    setupUrl: "https://ai.youdao.com/",
    setupSteps: [
      "访问有道智云，注册账号",
      "创建「自然语言翻译」应用",
      "获取应用 ID 和应用密钥",
      "新用户赠送 50 元体验金",
    ],
    description: "有道智云翻译，支持多语言，新用户有赠送额度",
  },
  {
    id: "google",
    name: "Google Translate",
    badge: "免费无限",
    badgeColor: "bg-gray-500",
    freeQuota: "无限制（非官方）",
    quality: "medium",
    requiresApiKey: false,
    credentialFields: [],
    setupUrl: "",
    setupSteps: [],
    description: "无需配置，开箱即用。作为其他翻译源的兜底备选",
  },
] as const;

export type ProviderId = typeof PROVIDERS[number]["id"];

/** 翻译源 ID → 显示名 */
export const PROVIDER_LABELS: Record<string, string> = {
  deepl:   "DeepL",
  tencent: "腾讯",
  baidu:   "百度",
  youdao:  "有道",
  google:  "Google",
};

export const ERROR_MESSAGES: Record<string, string> = {
  EMPTY_TEXT:          "未检测到选中文本",
  NON_TEXT_CONTENT:    "仅支持文本翻译",
  SAME_LANGUAGE:       "源语言与目标语言相同",
  NETWORK_ERROR:       "网络连接失败，请检查网络设置",
  AUTH_ERROR:          "API Key 无效，请在设置中检查",
  RATE_LIMIT:          "请求频率过高，请稍后重试",
  QUOTA_EXHAUSTED:     "翻译额度已用尽",
  TIMEOUT:             "翻译超时，请重试",
  ALL_PROVIDERS_FAILED:"所有翻译源均不可用",
  CLIPBOARD_ERROR:     "剪贴板操作失败",
  HOTKEY_CONFLICT:     "快捷键冲突，请在设置中修改",
  DATABASE_ERROR:      "数据存储错误",
  CONFIG_ERROR:        "配置错误",
  UNKNOWN:             "翻译失败，请重试",
};

export const LANG_NAMES: Record<string, string> = {
  zh: "中文", "zh-tw": "繁中", en: "英语", ja: "日语",
  ko: "韩语", fr: "法语", de: "德语", es: "西语",
  ru: "俄语", pt: "葡语", it: "意语", ar: "阿语", auto: "自动",
};

export function getLangName(code: string): string {
  return LANG_NAMES[code.toLowerCase()] ?? code.toUpperCase();
}
