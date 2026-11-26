//! 传输任务模型与状态机（LAN-004/005/006）。
//!
//! 骨架阶段提供状态机、进度与任务结构；分块发送/接收、流式 SHA-256、临时文件
//! 原子重命名、断点续传在实现阶段补齐（见设计文档第 7、8 节）。

use serde::{Deserialize, Serialize};

/// 建议分块大小，1–4 MiB，实测后调整。
pub const DEFAULT_CHUNK_SIZE: usize = 2 * 1024 * 1024;
/// 未完成数据的临时文件后缀，校验通过后原子重命名为目标文件。
pub const PART_SUFFIX: &str = ".clashnova-part";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Send,
    Receive,
}

/// 传输状态（见设计文档第 8 节状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransferState {
    Pending,
    WaitingApproval,
    Transferring,
    Paused,
    Completed,
    Failed,
    Canceled,
}

impl TransferState {
    /// 状态机允许的迁移。非法迁移应被拒绝。
    pub fn can_transition_to(self, next: TransferState) -> bool {
        use TransferState::*;
        matches!(
            (self, next),
            (Pending, WaitingApproval)
                | (Pending, Canceled)
                | (Pending, Failed)
                | (WaitingApproval, Transferring)
                | (WaitingApproval, Canceled)
                | (WaitingApproval, Failed)
                | (Transferring, Paused)
                | (Transferring, Completed)
                | (Transferring, Canceled)
                | (Transferring, Failed)
                | (Paused, Transferring)
                | (Paused, Canceled)
                | (Paused, Failed)
        )
    }

    /// 终态：不再迁移。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferState::Completed | TransferState::Failed | TransferState::Canceled
        )
    }

    /// 对外字符串（SCREAMING_SNAKE，与序列化一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            TransferState::Pending => "PENDING",
            TransferState::WaitingApproval => "WAITING_APPROVAL",
            TransferState::Transferring => "TRANSFERRING",
            TransferState::Paused => "PAUSED",
            TransferState::Completed => "COMPLETED",
            TransferState::Failed => "FAILED",
            TransferState::Canceled => "CANCELED",
        }
    }
}

/// 进度（基于实际落盘并确认的字节数，见设计文档第 7.3 节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub task_id: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub speed_bps: u64,
    pub eta_secs: Option<u64>,
}

/// 单个传输任务的对外快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferTask {
    pub id: String,
    pub direction: Direction,
    pub peer_alias: String,
    pub peer_fingerprint: String,
    pub file_name: String,
    pub size: u64,
    pub state: TransferState,
    /// 本机文件路径：发送方记录源文件，接收完成后记录目标文件。
    #[serde(default)]
    pub save_path: Option<String>,
}

/// 传输过程更新：由 transport 层上报，LanService 据此维护任务表、广播事件、落历史。
#[derive(Debug, Clone)]
pub enum TransferUpdate {
    /// 任务开始（state=Transferring）。
    Started(TransferTask),
    /// 进度（已节流）。
    Progress {
        task_id: String,
        bytes_done: u64,
        bytes_total: u64,
    },
    /// 任务结束（Completed/Failed/Canceled）。
    Finished(TransferTask),
}

/// 传输更新回传通道。
pub type TransferSink = tokio::sync::mpsc::UnboundedSender<TransferUpdate>;

#[cfg(test)]
mod tests {
    use super::TransferState::*;

    #[test]
    fn valid_and_invalid_transitions() {
        assert!(Pending.can_transition_to(WaitingApproval));
        assert!(WaitingApproval.can_transition_to(Transferring));
        assert!(Transferring.can_transition_to(Paused));
        assert!(Paused.can_transition_to(Transferring));
        assert!(Transferring.can_transition_to(Completed));

        assert!(!Pending.can_transition_to(Completed));
        assert!(!Completed.can_transition_to(Transferring));
        assert!(Completed.is_terminal());
        assert!(!Transferring.is_terminal());
    }
}
