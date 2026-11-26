//! 统一错误类型。对外接口一律返回 [`Result`]，不使用 `unwrap()`（见设计文档第 14 节）。

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LanError>;

#[derive(Debug, Error)]
pub enum LanError {
    #[error("Invalid text message")]
    InvalidText,
    #[error("Peer does not support text messages")]
    TextUnsupported,
    /// 骨架阶段占位：对应功能尚未实现。
    #[error("功能尚未实现: {0}")]
    Unimplemented(&'static str),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("HTTP 请求错误: {0}")]
    Http(#[from] reqwest::Error),

    /// 非法文件名或路径（可能的目录穿越/符号链接攻击）。
    #[error("非法路径: {0}")]
    InvalidPath(String),

    /// 设备未配对或证书指纹与记录不符。
    #[error("设备未信任或指纹不匹配: {0}")]
    Untrusted(String),

    #[error("传输被对端拒绝")]
    Rejected,

    #[error("传输已取消")]
    Canceled,

    #[error("完整性校验失败: 期望 {expected}, 实际 {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("协议错误: {0}")]
    Protocol(String),

    #[error("配置错误: {0}")]
    Config(String),
}
