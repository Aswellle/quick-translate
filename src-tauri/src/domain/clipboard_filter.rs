// src-tauri/src/domain/clipboard_filter.rs
// 剪贴板敏感内容过滤器 —— 纯函数、纯本地，不发起任何 I/O。
//
// 产品边界：「用户复制了文本」不等于「同意把它发给第三方翻译服务」。
// 本模块在剪贴板内容进入翻译流程（进而离开本机）之前做一次本地判定，
// 只拦**高置信度**的凭证形态 —— 私钥、JWT、各服务商的固定前缀令牌。
// 通用密码、普通随机串等低置信度形态刻意不拦：误拦普通技术文本的代价
// （用户失去信任、被迫反复手动覆盖）高于漏拦的理论收益。
//
// 空文本、重复去重、超长截断由剪贴板 worker 与请求构造层各自负责
// （`worker::is_new_content` / `request::MAX_TEXT_CHARS`），不在本模块重复。
//
// 实现说明：全部规则用字节级扫描完成，不引入 regex 依赖 —— 这些形态
// （固定前缀 + 字符集 + 长度 + 词边界）用顺序扫描既准确又快，且每条
// 分支都能确定性测试。所有 needle 均为 ASCII，按字节切片天然落在
// UTF-8 字符边界上。

/// 单条过滤规则。`code()` 供前端做文案映射（与 `AppError::error_code`
/// 的 SCREAMING_SNAKE 约定一致）；`reason()` 是后端直接给出的中文解释。
/// 两者都不携带原文 —— 被拦截的内容不出现在任何事件或日志里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveRule {
    PrivateKeyPem,
    Jwt,
    AwsAccessKeyId,
    GithubToken,
    GoogleApiKey,
    SlackToken,
    StripeSecretKey,
    OpenAiApiKey,
    AnthropicApiKey,
    TencentSecretId,
    AuthorizationHeader,
    DeeplAuthKey,
}

impl SensitiveRule {
    pub fn code(self) -> &'static str {
        match self {
            SensitiveRule::PrivateKeyPem => "SENSITIVE_PRIVATE_KEY",
            SensitiveRule::Jwt => "SENSITIVE_JWT",
            SensitiveRule::AwsAccessKeyId => "SENSITIVE_AWS_KEY",
            SensitiveRule::GithubToken => "SENSITIVE_GITHUB_TOKEN",
            SensitiveRule::GoogleApiKey => "SENSITIVE_GOOGLE_KEY",
            SensitiveRule::SlackToken => "SENSITIVE_SLACK_TOKEN",
            SensitiveRule::StripeSecretKey => "SENSITIVE_STRIPE_KEY",
            SensitiveRule::OpenAiApiKey => "SENSITIVE_OPENAI_KEY",
            SensitiveRule::AnthropicApiKey => "SENSITIVE_ANTHROPIC_KEY",
            SensitiveRule::TencentSecretId => "SENSITIVE_TENCENT_CREDENTIAL",
            SensitiveRule::AuthorizationHeader => "SENSITIVE_AUTH_HEADER",
            SensitiveRule::DeeplAuthKey => "SENSITIVE_DEEPL_KEY",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            SensitiveRule::PrivateKeyPem => "该内容疑似包含私钥，未自动发送到翻译服务",
            SensitiveRule::Jwt => "该内容疑似包含访问令牌（JWT），未自动发送到翻译服务",
            SensitiveRule::AwsAccessKeyId => "该内容疑似包含云服务访问密钥，未自动发送到翻译服务",
            SensitiveRule::GithubToken => "该内容疑似包含 GitHub 访问令牌，未自动发送到翻译服务",
            SensitiveRule::GoogleApiKey => "该内容疑似包含 Google API 密钥，未自动发送到翻译服务",
            SensitiveRule::SlackToken => "该内容疑似包含 Slack 令牌，未自动发送到翻译服务",
            SensitiveRule::StripeSecretKey => "该内容疑似包含支付平台密钥，未自动发送到翻译服务",
            SensitiveRule::OpenAiApiKey => "该内容疑似包含 OpenAI API 密钥，未自动发送到翻译服务",
            SensitiveRule::AnthropicApiKey => {
                "该内容疑似包含 Anthropic API 密钥，未自动发送到翻译服务"
            }
            SensitiveRule::TencentSecretId => "该内容疑似包含腾讯云访问凭证，未自动发送到翻译服务",
            SensitiveRule::AuthorizationHeader => {
                "该内容疑似包含 HTTP 鉴权头，未自动发送到翻译服务"
            }
            SensitiveRule::DeeplAuthKey => "该内容疑似包含 DeepL 认证密钥，未自动发送到翻译服务",
        }
    }
}

/// 过滤判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDecision {
    Allow,
    BlockSensitive { rule: SensitiveRule },
}

type Detector = fn(&str) -> bool;

/// 规则表：顺序即优先级（最独特的形态优先命中），每条规则一个检测函数。
const RULES: &[(SensitiveRule, Detector)] = &[
    (SensitiveRule::PrivateKeyPem, contains_pem_private_key),
    (SensitiveRule::Jwt, contains_jwt),
    (SensitiveRule::AwsAccessKeyId, contains_aws_access_key_id),
    (SensitiveRule::GithubToken, contains_github_token),
    (SensitiveRule::GoogleApiKey, contains_google_api_key),
    (SensitiveRule::SlackToken, contains_slack_token),
    (SensitiveRule::StripeSecretKey, contains_stripe_secret_key),
    (SensitiveRule::OpenAiApiKey, contains_openai_api_key),
    (SensitiveRule::AnthropicApiKey, contains_anthropic_api_key),
    (SensitiveRule::TencentSecretId, contains_tencent_secret_id),
    (
        SensitiveRule::AuthorizationHeader,
        contains_authorization_header,
    ),
    (SensitiveRule::DeeplAuthKey, contains_deepl_auth_key),
];

/// 对即将自动发送的文本做敏感内容判定。纯函数，可对每条规则穷举正反例。
pub fn classify(text: &str) -> FilterDecision {
    for (rule, detect) in RULES {
        if detect(text) {
            return FilterDecision::BlockSensitive { rule: *rule };
        }
    }
    FilterDecision::Allow
}

// ── 扫描辅助 ─────────────────────────────────────────────────────────────

/// token 通用字符集：字母数字下划线连字符。用于「词边界」判定 ——
/// 命中点前后若仍是同类字符，说明这只是某个更长随机串的中段。
fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// 在 `text` 中寻找 `prefix` 的出现位置，使其满足：
/// - 前一个字节不是 token 字符（避免在更长 token 的中间命中）；
/// - 前缀后紧跟至少 `min` 个满足 `class` 的字节。
///
/// run 的终止由「最大贪心」语义自然保证：run 吞掉所有连续 class 字节，
/// 因此 run ≥ min 就是一个完整凭证形态，无需再检查尾部。
/// 前缀与 class 均为 ASCII，`match_indices` 与字节切片都落在字符边界上。
fn find_prefixed_run(text: &str, prefix: &str, min: usize, class: fn(u8) -> bool) -> bool {
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices(prefix) {
        if i > 0 && is_token_byte(bytes[i - 1]) {
            continue;
        }
        let mut end = i + prefix.len();
        while end < bytes.len() && class(bytes[end]) {
            end += 1;
        }
        if end - (i + prefix.len()) >= min {
            return true;
        }
    }
    false
}

fn is_upper_alnum(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}

fn is_alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

fn is_b64url(b: u8) -> bool {
    is_alnum(b) || b == b'_' || b == b'-'
}

// ── 各规则检测函数 ───────────────────────────────────────────────────────

const PEM_PRIVATE_KEY_HEADERS: &[&str] = &[
    "-----BEGIN PRIVATE KEY-----",
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN DSA PRIVATE KEY-----",
    "-----BEGIN EC PRIVATE KEY-----",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "-----BEGIN PGP PRIVATE KEY BLOCK-----",
    "-----BEGIN ENCRYPTED PRIVATE KEY-----",
];

fn contains_pem_private_key(text: &str) -> bool {
    PEM_PRIVATE_KEY_HEADERS.iter().any(|h| text.contains(h))
}

/// JWT / JWS 紧凑序列化：`eyJ<seg1>.<seg2>.<seg3>`。
/// 三段都是 base64url；JWE 的五段式前三段同样命中。
/// 每段至少 6 字节（真实 JWT 的 header 远长于此，下限只为挡住巧合）；
/// 起点必须是词边界 —— base64 串的中段虽然也可能出现 "eyJ"，
/// 但后随的点号不属于 base64url 字符集，形状校验已经足够排他。
fn contains_jwt(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices("eyJ") {
        if i > 0 && is_token_byte(bytes[i - 1]) {
            continue;
        }
        if jwt_shape_matches(bytes, i) {
            return true;
        }
    }
    false
}

fn jwt_shape_matches(bytes: &[u8], start: usize) -> bool {
    let mut pos = start;
    for seg in 0..3 {
        if seg > 0 {
            if bytes.get(pos) != Some(&b'.') {
                return false;
            }
            pos += 1;
        }
        let seg_start = pos;
        while pos < bytes.len() && is_b64url(bytes[pos]) {
            pos += 1;
        }
        if pos - seg_start < 6 {
            return false;
        }
    }
    true
}

fn contains_aws_access_key_id(text: &str) -> bool {
    for prefix in ["AKIA", "ASIA"] {
        if find_prefixed_run(text, prefix, 16, is_upper_alnum) {
            return true;
        }
    }
    false
}

fn contains_github_token(text: &str) -> bool {
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        if find_prefixed_run(text, prefix, 36, is_alnum) {
            return true;
        }
    }
    find_prefixed_run(text, "github_pat_", 22, |b| is_alnum(b) || b == b'_')
}

fn contains_google_api_key(text: &str) -> bool {
    find_prefixed_run(text, "AIza", 35, is_b64url)
}

/// `xox<type>-<至少 10 个 token 字符>`，type ∈ {b, a, p, r, s}。
fn contains_slack_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices("xox") {
        if i > 0 && is_token_byte(bytes[i - 1]) {
            continue;
        }
        match bytes.get(i + 3) {
            Some(b'b' | b'a' | b'p' | b'r' | b's') => {}
            _ => continue,
        }
        if bytes.get(i + 4) != Some(&b'-') {
            continue;
        }
        let mut end = i + 5;
        while end < bytes.len() && (is_alnum(bytes[end]) || bytes[end] == b'-') {
            end += 1;
        }
        if end - (i + 5) >= 10 {
            return true;
        }
    }
    false
}

fn contains_stripe_secret_key(text: &str) -> bool {
    for prefix in ["sk_live_", "rk_live_"] {
        if find_prefixed_run(text, prefix, 16, is_alnum) {
            return true;
        }
    }
    false
}

/// OpenAI 密钥两种形态：
/// - 项目密钥 `sk-proj-` 前缀 + 至少 40 个 token 字符；
/// - 旧版 `sk-<alnum>T3BlbkFJ<alnum>`，以 T3BlbkFJ 中段标记为锚。
///
/// 裸 `sk-` 前缀是大量服务的通用约定，置信度不足，刻意不拦。
fn contains_openai_api_key(text: &str) -> bool {
    if find_prefixed_run(text, "sk-proj-", 40, is_b64url) {
        return true;
    }
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices("T3BlbkFJ") {
        // 回扫 T3BlbkFJ 之前的连续 alnum run
        let mut seg_start = i;
        while seg_start > 0 && is_alnum(bytes[seg_start - 1]) {
            seg_start -= 1;
        }
        if i - seg_start < 16 {
            continue; // sk- 与锚点之间不足 16 位
        }
        // alnum run 之前必须紧跟 "sk-" 字面量
        if seg_start < 3 || &bytes[seg_start - 3..seg_start] != b"sk-" {
            continue;
        }
        if seg_start > 3 && is_token_byte(bytes[seg_start - 4]) {
            continue; // sk- 前还连着 token 字符
        }
        // T3BlbkFJ 之后至少 16 个 alnum
        let after = i + 8;
        let mut end = after;
        while end < bytes.len() && is_alnum(bytes[end]) {
            end += 1;
        }
        if end - after >= 16 {
            return true;
        }
    }
    false
}

fn contains_anthropic_api_key(text: &str) -> bool {
    find_prefixed_run(text, "sk-ant-", 20, is_b64url)
}

fn contains_tencent_secret_id(text: &str) -> bool {
    find_prefixed_run(text, "AKID", 30, is_alnum)
}

/// 行级扫描：行内任意位置的 `authorization:` 后，跟至少 16 个连续非空白
/// 字符的值（Bearer/Basic 的值段、或直接的长凭证）。允许行中扫描是为了
/// 命中 curl 命令、日志行等「头部出现在中段」的常见粘贴形态。
/// `Api-Key`、`Cookie` 等其他鉴权头形态的误报面更宽，暂不纳入。
fn contains_authorization_header(text: &str) -> bool {
    const HEADER: &str = "authorization:";
    text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        let mut from = 0;
        while let Some(pos) = lower[from..].find(HEADER) {
            // lower 由 ASCII 小写化而来，字节位置与 line 对齐
            let value_start = from + pos + HEADER.len();
            if line[value_start..]
                .split_whitespace()
                .any(|seg| seg.len() >= 16)
            {
                return true;
            }
            from = value_start;
        }
        false
    })
}

/// DeepL 免费版密钥：36 位十六进制-连字符串后跟 `:fx`。
/// 裸 UUID 形态（付费版）与普通 UUID 无法区分，置信度不足，不拦。
fn contains_deepl_auth_key(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices(":fx") {
        if i < 36 {
            continue;
        }
        let key = &bytes[i - 36..i];
        if !key.iter().all(|&b| b.is_ascii_hexdigit() || b == b'-') {
            continue;
        }
        // 前一个字节若仍是同类字符，说明这只是更长串的尾巴
        if i > 36 && (bytes[i - 37].is_ascii_hexdigit() || bytes[i - 37] == b'-') {
            continue;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked_rule(text: &str) -> Option<SensitiveRule> {
        match classify(text) {
            FilterDecision::BlockSensitive { rule } => Some(rule),
            FilterDecision::Allow => None,
        }
    }

    fn assert_blocked(text: impl AsRef<str>, expected: SensitiveRule) {
        let text = text.as_ref();
        assert_eq!(blocked_rule(text), Some(expected), "应拦截: {text:?}");
    }

    fn assert_allowed(text: impl AsRef<str>) {
        let text = text.as_ref();
        assert_eq!(blocked_rule(text), None, "不应拦截: {text:?}");
    }

    /// 合成 JWT 样本：形状合法（eyJ 三段式），内容无意义。
    fn jwt_sample() -> String {
        format!(
            "eyJ{}.{}.{}",
            "a".repeat(30),
            "b".repeat(12),
            "c".repeat(43)
        )
    }

    // ── 正例：每条规则一个真实形态 ──────────────────────────────────────

    #[test]
    fn blocks_pem_private_keys() {
        assert_blocked(
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            SensitiveRule::PrivateKeyPem,
        );
        assert_blocked(
            "-----BEGIN RSA PRIVATE KEY-----",
            SensitiveRule::PrivateKeyPem,
        );
        assert_blocked(
            "-----BEGIN PGP PRIVATE KEY BLOCK-----",
            SensitiveRule::PrivateKeyPem,
        );
        assert_blocked(
            "日志前缀 -----BEGIN EC PRIVATE KEY----- 后续内容",
            SensitiveRule::PrivateKeyPem,
        );
    }

    #[test]
    fn blocks_jwt() {
        assert_blocked(jwt_sample().as_str(), SensitiveRule::Jwt);
        // 嵌在日志中间
        assert_blocked(
            format!("GET /api HTTP/1.1\nAuthorization: Bearer {}", jwt_sample()).as_str(),
            SensitiveRule::Jwt,
        );
    }

    #[test]
    fn blocks_aws_access_key_ids() {
        assert_blocked("AKIAIOSFODNN7EXAMPLE", SensitiveRule::AwsAccessKeyId);
        assert_blocked("ASIAIOSFODNN7EXAMPLE", SensitiveRule::AwsAccessKeyId);
        assert_blocked(
            "aws_access_key_id = AKIAIOSFODNN7EXAMPLE",
            SensitiveRule::AwsAccessKeyId,
        );
    }

    #[test]
    fn blocks_github_tokens() {
        assert_blocked(
            format!("ghp_{}", "a".repeat(36)).as_str(),
            SensitiveRule::GithubToken,
        );
        assert_blocked(
            format!("gho_{}", "B1xYz9".repeat(6)).as_str(),
            SensitiveRule::GithubToken,
        );
        assert_blocked(
            format!("github_pat_{}", "A1b2C3d4E5f6G7h8I9j0K1"),
            SensitiveRule::GithubToken,
        );
    }

    #[test]
    fn blocks_google_api_keys() {
        assert_blocked(
            format!("AIza{}", "a".repeat(35)).as_str(),
            SensitiveRule::GoogleApiKey,
        );
        assert_blocked(
            format!(
                "key=AIza{}&dummy=1",
                "SyC4x_9z-".repeat(4)[..35].to_string()
            )
            .as_str(),
            SensitiveRule::GoogleApiKey,
        );
    }

    #[test]
    fn blocks_slack_tokens() {
        assert_blocked(
            "xoxb-1234567890-1234567890123-abcdefabcdef",
            SensitiveRule::SlackToken,
        );
        assert_blocked(
            "xoxp-1234567890-1234567890123-1234567890123",
            SensitiveRule::SlackToken,
        );
    }

    #[test]
    fn blocks_stripe_live_keys() {
        assert_blocked(
            format!("sk_live_{}", "a1".repeat(12)),
            SensitiveRule::StripeSecretKey,
        );
        assert_blocked(
            format!("rk_live_{}", "K9xZ2mQwAbCdEfGh"),
            SensitiveRule::StripeSecretKey,
        );
    }

    #[test]
    fn blocks_openai_api_keys() {
        // 项目密钥形态
        assert_blocked(
            format!("sk-proj-{}", "aB3xY9kL2mNpQ4rStUvWxYz0123456789abcdefgh"),
            SensitiveRule::OpenAiApiKey,
        );
        // 旧版形态（T3BlbkFJ 锚点）
        assert_blocked(
            format!("sk-{}T3BlbkFJ{}", "a".repeat(20), "b".repeat(20)),
            SensitiveRule::OpenAiApiKey,
        );
    }

    #[test]
    fn blocks_anthropic_api_keys() {
        assert_blocked(
            format!("sk-ant-api03-{}", "a".repeat(25)),
            SensitiveRule::AnthropicApiKey,
        );
    }

    #[test]
    fn blocks_tencent_secret_ids() {
        assert_blocked(
            "AKIDz8krbsJ5yKBZQpn74WFkmLPx5EXAMPLE",
            SensitiveRule::TencentSecretId,
        );
    }

    #[test]
    fn blocks_authorization_headers() {
        assert_blocked(
            "Authorization: Bearer abcdef1234567890abcd",
            SensitiveRule::AuthorizationHeader,
        );
        // 大小写不敏感
        assert_blocked(
            "authorization: Basic dXNlcjpwYXNzd29yZA==",
            SensitiveRule::AuthorizationHeader,
        );
        // curl 命令行里的鉴权头
        assert_blocked(
            "curl -H \"Authorization: Bearer abcdef1234567890abcd\" https://api.example.com",
            SensitiveRule::AuthorizationHeader,
        );
    }

    #[test]
    fn blocks_deepl_auth_keys() {
        assert_blocked(
            "85d2c1b4-2a9e-4f3b-9c1d-7e6f5a4b3c2d:fx",
            SensitiveRule::DeeplAuthKey,
        );
        assert_blocked(
            "key: 85d2c1b4-2a9e-4f3b-9c1d-7e6f5a4b3c2d:fx 结尾",
            SensitiveRule::DeeplAuthKey,
        );
    }

    // ── 优先级：最独特的形态先命中 ──────────────────────────────────────

    #[test]
    fn private_key_wins_over_other_rules() {
        assert_blocked(
            format!("-----BEGIN RSA PRIVATE KEY----- {}", jwt_sample()).as_str(),
            SensitiveRule::PrivateKeyPem,
        );
    }

    // ── 负例：普通内容不得误拦 ──────────────────────────────────────────

    #[test]
    fn allows_normal_prose() {
        assert_allowed("The quick brown fox jumps over the lazy dog.");
        assert_allowed("这是一段普通的中文内容，包含一些技术词汇比如 JWT 和 API。");
        assert_allowed("");
        assert_allowed("   \n  ");
    }

    #[test]
    fn allows_common_code_snippets() {
        assert_allowed("const jwt_token = getJwt();");
        assert_allowed("let apiKey = config.api_key;");
        assert_allowed("git clone https://github.com/user/repo.git");
        assert_allowed("Authorization 头部需要设置 Bearer token");
    }

    #[test]
    fn allows_base64_without_jwt_shape() {
        // eyJ 开头但没有三段式点号 —— 只是普通 base64
        assert_allowed("eyJhbGciOiJIUzI1NiJ9");
        // 有点号但段太短
        assert_allowed("eyJaa.bbcccc.dddddd");
    }

    #[test]
    fn allows_public_certificates() {
        assert_allowed("-----BEGIN CERTIFICATE-----");
        assert_allowed("-----BEGIN PUBLIC KEY-----");
    }

    #[test]
    fn allows_bare_uuids() {
        assert_allowed("85d2c1b4-2a9e-4f3b-9c1d-7e6f5a4b3c2d");
        assert_allowed("correlation-id: 85d2c1b4-2a9e-4f3b-9c1d-7e6f5a4b3c2d");
    }

    #[test]
    fn allows_test_and_short_tokens() {
        // Stripe 测试密钥刻意放行（ live 才拦）
        assert_allowed("sk_test_abcdefghijklmnopqrstuv");
        // 裸 sk- 是大量服务的通用前缀，置信度不足
        assert_allowed("sk-Nw2vq7mfxT3zabc123");
        // ghp_ 后不足 36 位
        assert_allowed("ghp_short");
        // xox 后不是合法类型或太短
        assert_allowed("xoxz-1234567890123");
        assert_allowed("xoxb-1");
        // AKID 后不足 30 位
        assert_allowed("AKID123");
        // AIza 后不足 35 位
        assert_allowed("AIzaSyC4x");
        // ASIA 后是小写（不属于 key id 字符集）
        assert_allowed("ASIAiosfodnn7example");
    }

    #[test]
    fn allows_tokens_embedded_in_longer_words() {
        // 词中段命中必须被词边界检查挡住
        assert_allowed("xxAKIAIOSFODNN7EXAMPLEyy");
        assert_allowed("xxAKIDz8krbsJ5yKBZQpn74WFkmLPx5EXAMPLEyy");
        assert_allowed("prefix_ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_suffix");
        assert_allowed("no-sso-xoxb-123456789012-1234567890123-abcdef");
    }

    #[test]
    fn allows_prose_mentioning_authorization() {
        assert_allowed("the authorization header is important for auth");
        assert_allowed("Authorization");
        // 值太短
        assert_allowed("Authorization: Bearer abc");
    }

    #[test]
    fn allows_uppercase_words_resembling_aws() {
        assert_allowed("AKIAIOSFODNN7EXAMPL"); // 15 位，差一个
    }

    // ── 多行与混合内容 ──────────────────────────────────────────────────

    #[test]
    fn detects_credentials_inside_multiline_logs() {
        let log = "2026-11-07 10:00:00 INFO connecting...\n\
                   2026-11-07 10:00:01 ERROR auth failed with AKIAIOSFODNN7EXAMPLE\n\
                   2026-11-07 10:00:02 retrying";
        assert_blocked(log, SensitiveRule::AwsAccessKeyId);
    }

    #[test]
    fn reason_never_contains_the_original_text() {
        // 解释文案是静态字符串，天然不含原文 —— 用一条样本锁定这个契约
        for rule in [
            SensitiveRule::PrivateKeyPem,
            SensitiveRule::Jwt,
            SensitiveRule::AwsAccessKeyId,
            SensitiveRule::GithubToken,
            SensitiveRule::GoogleApiKey,
            SensitiveRule::SlackToken,
            SensitiveRule::StripeSecretKey,
            SensitiveRule::OpenAiApiKey,
            SensitiveRule::AnthropicApiKey,
            SensitiveRule::TencentSecretId,
            SensitiveRule::AuthorizationHeader,
            SensitiveRule::DeeplAuthKey,
        ] {
            let reason = rule.reason();
            assert!(!reason.is_empty());
            assert!(!reason.contains("AKIA"));
            assert!(!reason.contains("eyJ"));
            assert!(!reason.contains("-----BEGIN"));
        }
    }

    #[test]
    fn codes_are_unique_and_screaming_snake() {
        let codes: Vec<&str> = [
            SensitiveRule::PrivateKeyPem,
            SensitiveRule::Jwt,
            SensitiveRule::AwsAccessKeyId,
            SensitiveRule::GithubToken,
            SensitiveRule::GoogleApiKey,
            SensitiveRule::SlackToken,
            SensitiveRule::StripeSecretKey,
            SensitiveRule::OpenAiApiKey,
            SensitiveRule::AnthropicApiKey,
            SensitiveRule::TencentSecretId,
            SensitiveRule::AuthorizationHeader,
            SensitiveRule::DeeplAuthKey,
        ]
        .iter()
        .map(|r| r.code())
        .collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "code 不得重复");
        for code in &codes {
            assert!(
                code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "{code} 应为 SCREAMING_SNAKE"
            );
            assert!(
                code.starts_with("SENSITIVE_"),
                "{code} 应带 SENSITIVE_ 前缀"
            );
        }
    }
}
