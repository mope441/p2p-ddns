use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
};

use iroh::{EndpointAddr, EndpointId};
use serde::{Deserialize, Serialize};

/// 节点在教学场景中的角色标签。
/// Phase 2 中仅为数据字段，不影响 P2P 行为。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeRole {
    Teacher,
    Student,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub node_id: EndpointId,
    pub invitor: EndpointId,
    pub addr: EndpointAddr,
    pub domain: String,
    pub services: BTreeMap<String, u32>,
    pub last_heartbeat: u64,
    /// 教学角色不在 Node 中持久化，仅存于 ClassMember。
    /// 保留此字段以兼容旧 DB 数据（反序列化时忽略，不序列化）。
    #[serde(skip)]
    pub role: Option<NodeRole>,
}

#[cfg(test)]
impl Node {
    pub fn random_node() -> Self {
        let mut rng = rand::rng();
        let sk = iroh::SecretKey::generate(&mut rng);
        let pk = sk.public();

        Self {
            node_id: pk,
            invitor: pk,
            addr: EndpointAddr::new(pk),
            domain: String::new(),
            services: BTreeMap::new(),
            last_heartbeat: 0,
            role: None,
        }
    }
}

impl Hash for Node {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_id.hash(state);
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
    }
}

impl Eq for Node {}
