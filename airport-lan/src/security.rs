//! 身份、配对与输入校验（LAN-007）。
//!
//! 骨架阶段实现两类纯逻辑的安全边界：证书指纹计算、文件名/路径校验。
//! TLS 证书生成、TOFU 配对持久化在实现阶段补齐（见设计文档第 5 节）。

use crate::error::{LanError, Result};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

/// 计算证书指纹：对 DER 字节做 SHA-256，输出小写 hex。
///
/// HTTPS 模式下 LocalSend 的 `fingerprint` 即证书的 SHA-256，用于 TOFU 记忆与 pin。
pub fn fingerprint_of(cert_der: &[u8]) -> String {
    hex_lower(&Sha256::digest(cert_der))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Windows 保留设备名（忽略扩展名、大小写）。
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 清洗对端提供的文件名：仅保留单一名字分量，拒绝空、`.`/`..`、路径分隔符、
/// 控制字符与 Windows 保留名。防止目录穿越与非法写入（见设计文档第 5 节）。
pub fn sanitize_file_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(LanError::InvalidPath(raw.to_string()));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(LanError::InvalidPath(raw.to_string()));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(LanError::InvalidPath(raw.to_string()));
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED_NAMES.contains(&stem.as_str()) {
        return Err(LanError::InvalidPath(raw.to_string()));
    }
    Ok(name.to_string())
}

/// 将清洗后的文件名安全拼接到目标目录内，并确认最终路径未逃出 `base`。
///
/// Rust 层负责一切目标路径拼接，前端不得直接拼接（见设计文档第 9 节约束）。
pub fn resolve_within(base: &Path, file_name: &str) -> Result<PathBuf> {
    let clean = sanitize_file_name(file_name)?;
    let candidate = base.join(&clean);
    // 不允许最终路径包含任何上溯分量。
    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(LanError::InvalidPath(file_name.to_string()));
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_separators_and_reserved() {
        assert!(sanitize_file_name("../etc/passwd").is_err());
        assert!(sanitize_file_name("a/b.txt").is_err());
        assert!(sanitize_file_name("a\\b.txt").is_err());
        assert!(sanitize_file_name("..").is_err());
        assert!(sanitize_file_name("").is_err());
        assert!(sanitize_file_name("CON").is_err());
        assert!(sanitize_file_name("con.txt").is_err());
        assert!(sanitize_file_name("report.pdf").is_ok());
    }

    #[test]
    fn resolve_stays_within_base() {
        let base = Path::new("/tmp/recv");
        assert!(resolve_within(base, "ok.bin").is_ok());
        assert!(resolve_within(base, "../escape").is_err());
    }

    #[test]
    fn fingerprint_is_lowercase_hex() {
        let fp = fingerprint_of(b"hello");
        assert_eq!(fp.len(), 64);
        assert!(fp
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }
}
