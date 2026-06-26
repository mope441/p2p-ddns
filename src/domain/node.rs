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
    /// 节点在教学场景中的角色。向后兼容：旧数据反序列化时默认为 None。
    #[serde(default)]
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
