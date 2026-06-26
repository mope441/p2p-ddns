use iroh::EndpointId;
use iroh_gossip::TopicId;
use serde::{Deserialize, Serialize};

use crate::domain::node::NodeRole;

/// 一个教学班级。与 P2P Topic 1:1 映射。
/// class_id = daemon 启动时建立的 TopicId。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassNetwork {
    pub class_id: TopicId,
    pub class_name: String,
    pub teacher_node_id: EndpointId,
    pub teacher_domain: String,
    pub created_at: u64,
}

/// 成员注册状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemberStatus {
    /// 教师已发邀请，学生尚未响应
    Invited,
    /// 学生已加入，等待教师审批
    Pending,
    /// 已审批，活跃成员
    Approved,
    /// 已从班级移除
    Removed,
}

/// 一个班级成员。node_id 标识 P2P 节点，display_name 是人类可读名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassMember {
    pub node_id: EndpointId,
    /// 人类可读名（如 "张三"），区别于 domain（机器名）
    pub display_name: String,
    pub role: NodeRole,
    pub status: MemberStatus,
    /// 该成员所属的 group_id 列表
    pub groups: Vec<String>,
    pub joined_at: u64,
}

/// 班级内的小组。由教师创建和管理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub group_id: String,
    pub group_name: String,
    /// 小组成员的 node_id 列表
    pub members: Vec<EndpointId>,
    pub created_at: u64,
}

/// 教师创建的待处理邀请。学生 node_id 在邀请时未知。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingInvite {
    pub invite_id: String,
    pub display_name: String,
    pub shared_secret: String, // base64 编码的随机 32 字节
    pub created_at: u64,
    pub expires_at: u64,
}

/// 可 JSON 序列化的邀请文件。教师生成，学生使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinTicket {
    pub invite_id: String,
    pub class_id: TopicId,
    pub class_name: String,
    pub teacher_node_id: EndpointId,
    pub teacher_domain: String,
    pub p2p_ticket: String,
    pub transport_port: u16,
    pub shared_secret: String,
    pub expires_at: u64,
}
