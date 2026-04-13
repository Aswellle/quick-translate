// src-tauri/src/infra/crypto.rs
// AES-256-GCM 加解密，用于 API Key 安全存储
// 密钥从应用标识符派生（机器级别隔离，非用户级加密）

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::error::AppError;

/// 应用加密密钥（编译时固定，用于本地存储保护）
/// 生产环境应从 OS keychain 或机器唯一标识派生，此处简化处理
const APP_KEY: &[u8; 32] = b"QuickTranslate-AES-256-Key-v1.00";

/// 加密文本 → Base64 编码的密文（nonce 前缀）
pub fn encrypt(plaintext: &str) -> Result<String, AppError> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }

    let cipher = Aes256Gcm::new(APP_KEY.into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| AppError::CryptoError(format!("加密失败: {}", e)))?;

    // 格式：base64(nonce || ciphertext)
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

/// 解密 Base64 编码的密文 → 明文
pub fn decrypt(encoded: &str) -> Result<String, AppError> {
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

    let cipher = Aes256Gcm::new(APP_KEY.into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::CryptoError(format!("解密失败: {}", e)))?;

    String::from_utf8(plaintext)
        .map_err(|e| AppError::CryptoError(format!("UTF-8 解码失败: {}", e)))
}

/// 脱敏处理：仅显示末 4 位
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 4 {
        return "*".repeat(key.len());
    }
    let visible = &key[key.len() - 4..];
    format!("{}****", visible)
}
