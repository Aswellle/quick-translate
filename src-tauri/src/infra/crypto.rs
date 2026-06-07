// src-tauri/src/infra/crypto.rs
// AES-256-GCM 加解密，用于 API Key 安全存储
// 密钥从卷序列号派生（机器级别隔离，非用户级加密）

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::error::AppError;

/// 应用固定盐值，混入卷序列号生成机器绑定密钥
const APP_SALT: &[u8] = b"QuickTranslate-v1";

/// 获取机器绑定的 32 字节 AES 密钥
/// Windows: SHA-256(volume_serial_number_bytes || APP_SALT)
/// 非 Windows 或获取失败时：回退到编译期固定密钥（保持向后兼容）
#[cfg(target_os = "windows")]
pub fn get_machine_key() -> [u8; 32] {
    use sha2::{Digest, Sha256};

    if let Some(serial) = get_volume_serial() {
        let mut hasher = Sha256::new();
        hasher.update(serial.to_le_bytes());
        hasher.update(APP_SALT);
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        return key;
    }

    // 获取失败时回退到固定密钥
    *b"QuickTranslate-AES-256-Key-v1.00"
}

#[cfg(not(target_os = "windows"))]
pub fn get_machine_key() -> [u8; 32] {
    *b"QuickTranslate-AES-256-Key-v1.00"
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

/// 加密文本 → Base64 编码的密文（nonce 前缀）
pub fn encrypt(plaintext: &str) -> Result<String, AppError> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }

    let key = get_machine_key();
    let cipher = Aes256Gcm::new(key.as_slice().into());
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

    let key = get_machine_key();
    let cipher = Aes256Gcm::new(key.as_slice().into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::CryptoError(format!("解密失败: {}", e)))?;

    String::from_utf8(plaintext)
        .map_err(|e| AppError::CryptoError(format!("UTF-8 解码失败: {}", e)))
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
