//! 流式 SHA-256（LAN-006）。内存占用有界，不随文件大小线性增长。

use crate::error::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// 字节转小写 hex。
pub fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// 流式计算文件的 SHA-256（小写 hex）。
pub async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hashes_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        tokio::fs::write(&path, b"abc").await.unwrap();
        // SHA-256("abc")
        assert_eq!(
            sha256_file(&path).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
