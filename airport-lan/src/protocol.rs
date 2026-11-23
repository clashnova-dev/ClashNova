//! LocalSend v2 兼容协议的报文结构。
//!
//! 字段名严格对齐 LocalSend 规范（camelCase），私有扩展一律走可选字段，
//! 以保证默认可与第三方 LocalSend App 互通（见设计文档第 4 节、第 4.4 节）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LocalSend HTTP API 前缀。
pub const API_PREFIX: &str = "/api/localsend/v2";
/// 默认端口（UDP 组播与 HTTP/TCP 同端口，可配置）。
pub const DEFAULT_PORT: u16 = 53317;
/// 本实现宣告的协议版本。
pub const PROTOCOL_VERSION: &str = "2.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mobile,
    Desktop,
    Web,
    Headless,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportProtocol {
    Http,
    Https,
}

/// 设备信息，用于发现登记（register）与 prepare-upload 的 `info` 字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub alias: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<TransportProtocol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<bool>,
}

/// UDP 组播 announce 报文：设备信息 + `announce` 标志。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticastAnnounce {
    #[serde(flatten)]
    pub device: DeviceInfo,
    /// true 时收到方必须回复（HTTP register 或组播回复）。
    pub announce: bool,
}

/// prepare-upload 中单个文件的描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub file_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FileMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessed: Option<String>,
}

/// `POST /prepare-upload` 请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadRequest {
    pub info: DeviceInfo,
    /// key 为 fileId，与各 `FileDto.id` 一致。
    pub files: HashMap<String, FileDto>,
}

/// `POST /prepare-upload` 响应体：会话 id 与各需传输文件的上传 token。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadResponse {
    pub session_id: String,
    /// key 为 fileId，value 为该文件的上传 token。
    pub files: HashMap<String, String>,
}
