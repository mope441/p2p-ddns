use iroh::EndpointId;
use serde::{Deserialize, Serialize};

/// 投递状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeliveryStatus {
    Created,
    Sent,
    Delivered,
    Failed,
    Expired,
}

/// 一条待发送/已发送的消息记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub message_id: String,
    pub sender: EndpointId,
    pub text: String,
    pub recipient_count: usize,
    pub created_at: u64,
}

/// 发件箱记录 — 追踪发送和重试
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxRecord {
    pub message_id: String,
    pub message: MessageRecord,
    pub status: DeliveryStatus,
    pub retry_count: u32,
    pub max_retries: u32,
    pub next_retry_at: u64,
    pub last_error: Option<String>,
}

impl OutboxRecord {
    /// 计算下次重试时间（指数退避，上限 300s）
    pub fn compute_next_retry(&self, now: u64) -> u64 {
        let base: u64 = match self.retry_count {
            0 => 5,
            1 => 15,
            2 => 60,
            _ => 300,
        };
        now + base
    }
}

/// 审计事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub timestamp: u64,
    pub result: String,
}
