// src-tauri/src/infra/crypto.rs
// AES-256-GCM 加解密，用于 API Key 安全存储
// 密钥从「每台机器独有的随机密钥 + 卷序列号」派生（机器级别隔离，非用户级加密）

use std::path::Path;
use std::sync::OnceLock;

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// 应用固定盐值，混入卷序列号与随机密钥生成机器绑定密钥
const APP_SALT: &[u8] = b"QuickTranslate-v1";

/// 密钥派生域名分隔符（v2 = 引入 per-install 随机密钥后的版本）
const KDF_DOMAIN: &[u8] = b"QuickTranslate-credential-v2";

/// 每台机器独有的随机密钥文件（存放于 app_data_dir，受 OS 级 ACL 保护）
const SECRET_FILE_NAME: &str = ".machine_secret";

/// 随机密钥长度（256 位）
const PER_INSTALL_SECRET_LEN: usize = 32;

/// 每台机器独有的随机密钥（进程生命周期内只初始化一次）
static PER_INSTALL_SECRET: OnceLock<[u8; PER_INSTALL_SECRET_LEN]> = OnceLock::new();

/// 初始化每台机器独有的随机密钥。
///
/// 从 `app_data_dir/.machine_secret` 读取；若不存在则生成 256 位随机密钥写入。
/// 必须在首次调用 `get_machine_key()` 之前执行（lib.rs::setup 中调用）。
pub fn init_per_install_secret(app_data_dir: &Path) -> Result<(), AppError> {
    let path = app_data_dir.join(SECRET_FILE_NAME);

    let secret = if path.exists() {
        let bytes = std::fs::read(&path).map_err(|e| {
            AppError::CryptoError(format!("读取机器密钥文件失败 {}: {}", path.display(), e))
        })?;
        if bytes.len() != PER_INSTALL_SECRET_LEN {
            return Err(AppError::CryptoError(format!(
                "机器密钥文件长度无效（期望 {} 字节，实际 {} 字节），文件可能已损坏",
                PER_INSTALL_SECRET_LEN,
                bytes.len()
            )));
        }
        let mut secret = [0u8; PER_INSTALL_SECRET_LEN];
        secret.copy_from_slice(&bytes);
        secret
    } else {
        let mut secret = [0u8; PER_INSTALL_SECRET_LEN];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        std::fs::write(&path, secret).map_err(|e| {
            AppError::CryptoError(format!("写入机器密钥文件失败 {}: {}", path.display(), e))
        })?;
        // app_data_dir 位于用户专属 %APPDATA% 下，继承 OS 级 ACL，
        // 与数据库文件处于同一保护边界。
        tracing::info!("已生成新的机器绑定随机密钥: {}", path.display());
        secret
    };

    PER_INSTALL_SECRET
        .set(secret)
        .map_err(|_| AppError::CryptoError("机器密钥已被初始化，不可重复设置".into()))?;

    Ok(())
}

/// 获取已初始化的 per-install 随机密钥。
fn get_per_install_secret() -> Option<&'static [u8; PER_INSTALL_SECRET_LEN]> {
    PER_INSTALL_SECRET.get()
}

/// 密钥派生：SHA-256(KDF_DOMAIN || per_install_secret || serial_le_bytes || APP_SALT)
fn derive_new_key(secret: &[u8; PER_INSTALL_SECRET_LEN], serial: Option<u32>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(KDF_DOMAIN);
    hasher.update(secret);
    if let Some(serial) = serial {
        hasher.update(serial.to_le_bytes());
    }
    hasher.update(APP_SALT);
    let mut key = [0u8; 32];
    key.copy_from_slice(&hasher.finalize());
    key
}

/// 获取机器绑定的 32 字节 AES 密钥。
///
/// 主路径：per_install_secret || 卷序列号 || APP_SALT（高熵、机器绑定）。
/// 若 per-install 密钥尚未初始化（不应在生产环境发生），回退到旧版派生以
/// 避免数据损坏；此回退路径仅作为防御性保底，不改变主路径安全性。
pub fn get_machine_key() -> [u8; 32] {
    match get_per_install_secret() {
        Some(secret) => {
            #[cfg(target_os = "windows")]
            {
                derive_new_key(secret, get_volume_serial())
            }
            #[cfg(not(target_os = "windows"))]
            {
                derive_new_key(secret, None)
            }
        }
        None => {
            // 防御性保底：per-install 密钥未初始化时使用旧版派生。
            // 生产环境中 init_per_install_secret 始终先于此函数被调用，
            // 故此分支仅在测试/异常路径下生效。
            tracing::warn!("per-install 密钥未初始化，使用旧版密钥派生（防御性保底）");
            #[cfg(target_os = "windows")]
            {
                if let Some(serial) = get_volume_serial() {
                    return derive_old_key(serial);
                }
            }
            *b"QuickTranslate-AES-256-Key-v1.00"
        }
    }
}

/// 旧版密钥派生（仅用于迁移兼容）：SHA-256(serial || APP_SALT)
#[cfg(target_os = "windows")]
fn derive_old_key(serial: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(serial.to_le_bytes());
    hasher.update(APP_SALT);
    let mut key = [0u8; 32];
    key.copy_from_slice(&hasher.finalize());
    key
}

/// 返回旧版密钥候选列表（用于迁移时尝试解密历史数据）。
pub(crate) fn old_key_candidates() -> Vec<[u8; 32]> {
    #[cfg(target_os = "windows")]
    {
        get_volume_serial()
            .map(|serial| vec![derive_old_key(serial), *b"QuickTranslate-AES-256-Key-v1.00"])
            .unwrap_or_else(|| vec![*b"QuickTranslate-AES-256-Key-v1.00"])
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![*b"QuickTranslate-AES-256-Key-v1.00"]
    }
}

/// 读取系统盘（C:\）的卷序列号
#[cfg(target_os = "windows")]
fn get_volume_serial() -> Option<u32> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    let root: Vec<u16> = OsStr::new("C:\\")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut serial: u32 = 0;

    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            &mut serial as *mut u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };

    if ok != 0 {
        Some(serial)
    } else {
        None
    }
}

/// 使用指定密钥加密（供迁移逻辑使用）
pub(crate) fn encrypt_with_key(plaintext: &str, key: &[u8; 32]) -> Result<String, AppError> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let cipher = Aes256Gcm::new(key.as_slice().into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| AppError::CryptoError(format!("加密失败: {}", e)))?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// 使用指定密钥解密（供迁移逻辑使用）
pub(crate) fn decrypt_with_key(encoded: &str, key: &[u8; 32]) -> Result<String, AppError> {
    if encoded.is_empty() {
        return Ok(String::new());
    }
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| AppError::CryptoError(format!("Base64 解码失败: {}", e)))?;
    if combined.len() < 12 {
        return Err(AppError::CryptoError("密文格式无效".to_string()));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key.as_slice().into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::CryptoError(format!("解密失败: {}", e)))?;
    String::from_utf8(plaintext)
        .map_err(|e| AppError::CryptoError(format!("UTF-8 解码失败: {}", e)))
}

/// 加密文本 → Base64 编码的密文（nonce 前缀）
pub fn encrypt(plaintext: &str) -> Result<String, AppError> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let key = get_machine_key();
    encrypt_with_key(plaintext, &key)
}

/// 解密 Base64 编码的密文 → 明文
pub fn decrypt(encoded: &str) -> Result<String, AppError> {
    if encoded.is_empty() {
        return Ok(String::new());
    }
    let key = get_machine_key();
    decrypt_with_key(encoded, &key)
}

/// 脱敏处理：掩盖前 N-4 位，末 4 位以明文后缀显示（char 边界安全）
/// 输出格式：`"••••••aB3x"`（先星号，后末 4 位），而非 `"aB3x****"`
/// 等于 4 位或更短时全部掩盖（防止暴露整体长度）
pub fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len());
    }
    let hidden = "*".repeat(chars.len() - 4);
    let visible: String = chars[chars.len() - 4..].iter().collect();
    format!("{}{}", hidden, visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_api_key_long() {
        assert_eq!(mask_api_key("abcd1234efgh"), "********efgh");
    }

    #[test]
    fn mask_api_key_short() {
        assert_eq!(mask_api_key("abc"), "***");
        assert_eq!(mask_api_key("abcd"), "****");
    }

    #[test]
    fn mask_api_key_empty() {
        assert_eq!(mask_api_key(""), "");
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        // 使用固定密钥测试加解密对称性（不依赖机器状态）
        let key = [0x42u8; 32];
        let plaintext = "sk-test-12345-secret-key";
        let encoded = encrypt_with_key(plaintext, &key).expect("加密应成功");
        let decoded = decrypt_with_key(&encoded, &key).expect("解密应成功");
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn encrypt_empty_is_empty() {
        let key = [0x42u8; 32];
        assert_eq!(encrypt_with_key("", &key).unwrap(), "");
        assert_eq!(decrypt_with_key("", &key).unwrap(), "");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key1 = [0x42u8; 32];
        let key2 = [0x13u8; 32];
        let encoded = encrypt_with_key("secret-data", &key1).unwrap();
        assert!(decrypt_with_key(&encoded, &key2).is_err());
    }

    #[test]
    fn derive_new_key_is_deterministic() {
        let secret = [0xABu8; 32];
        let k1 = derive_new_key(&secret, Some(0x12345678));
        let k2 = derive_new_key(&secret, Some(0x12345678));
        assert_eq!(k1, k2);
    }

    #[test]
    fn derive_new_key_differs_by_serial() {
        let secret = [0xABu8; 32];
        let k1 = derive_new_key(&secret, Some(1));
        let k2 = derive_new_key(&secret, Some(2));
        assert_ne!(k1, k2);
    }

    #[test]
    fn derive_new_key_differs_from_old() {
        // 确保新版派生与旧版（仅 serial+salt）不同，避免迁移时误判
        let secret = [0xABu8; 32];
        let new_k = derive_new_key(&secret, Some(0x12345678));
        let old_k = derive_old_key(0x12345678);
        assert_ne!(new_k, old_k);
    }
}
