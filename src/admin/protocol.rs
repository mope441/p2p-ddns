use crate::{
    class_network::model::{ClassMember, ClassNetwork, Group},
    domain::{client::DaemonStatus, node::Node},
};
use serde::{Deserialize, Serialize};

/// Client到Daemon的认证请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub ticket: String,
    pub client_public_key: Option<String>,
    pub client_name: Option<String>,
}

/// Daemon到Client的认证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub success: bool,
    pub error: Option<String>,
    pub daemon_public_key: Option<String>,
}

/// Client管理命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientCommand {
    Query,
    ResolveNode {
        id_or_domain: String,
    },
    AddNode {
        ticket: String,
    },
    RemoveNode {
        id: String,
    },
    SetService {
        name: String,
        port: u32,
    },
    Status,
    GetTicket,
    Pause,
    Resume,
    Shutdown,
    // ── 班级操作 ──
    ClassCreate {
        class_name: String,
    },
    ClassInfo,
    ClassMembersList,
    ClassGroupsList,
    // ── 邀请/加入/审批 ──
    ClassInviteCreate {
        display_name: String,
        expires_secs: Option<u64>,
    },
    ClassJoin {
        invite_json: String,
    },
    ClassJoinRequestsList,
    ClassJoinRequestApprove {
        node_id: String,
    },
    ClassJoinRequestDeny {
        node_id: String,
    },
    // ── 分组管理 ──
    ClassGroupCreate {
        group_name: String,
    },
    ClassGroupAddMember {
        group_id: String,
        node_id: String,
    },
    ClassGroupRemoveMember {
        group_id: String,
        node_id: String,
    },
    ClassGroupMembersList {
        group_id: String,
    },
    ClassGroupDelete {
        group_id: String,
    },
    // ── 广播/组播 ──
    ClassBroadcast {
        message: String,
    },
    ClassMulticast {
        group_id: String,
        message: String,
    },
    ClassDirectSend {
        node_id: String,
        message: String,
    },
    // ── 消息审计 ──
    ClassMessageStatus {
        message_id: String,
    },
    ClassMessageRetry {
        message_id: String,
    },
    ClassAuditList,
}

/// Daemon响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientResponse {
    Nodes(Vec<Node>),
    Node(Option<Node>),
    Status(DaemonStatus),
    Ticket(String),
    Ack(String),
    Error(String),
    // ── 班级响应 ──
    ClassCreated(ClassNetwork),
    ClassInfo(ClassNetwork),
    ClassMembers(Vec<ClassMember>),
    ClassGroups(Vec<Group>),
    // ── 邀请/加入/审批响应 ──
    InviteCreated(String),
    JoinRequested(String),
    JoinRequests(Vec<ClassMember>),
    JoinRequestApproved(String),
    JoinRequestDenied(String),
    // ── 分组响应 ──
    GroupCreated(Group),
    GroupMembers(Vec<ClassMember>),
    BroadcastResult {
        total: usize,
        sent: usize,
        failed: usize,
        failures: Vec<String>,
    },
    MessageStatus(String),    // JSON status info
    AuditLog(String),         // JSON audit log
}

/// HTTP等无连接场景下的一次性请求封装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminCommandRequest {
    pub auth: AuthRequest,
    pub command: ClientCommand,
}
