use std::sync::Arc;

use anyhow::Result;
use iroh::EndpointId;
use redb::{Database, ReadableTable, TableDefinition};

use crate::class_network::{
    messaging::{AuditEvent, OutboxRecord},
    model::{ClassMember, ClassNetwork, Group, MemberStatus, PendingInvite},
};

const CLASS_NETWORK_KEY: &str = "class_network";

const TABLE_CLASS_NETWORK: TableDefinition<&str, &[u8]> = TableDefinition::new("class_network");
const TABLE_CLASS_MEMBERS: TableDefinition<&str, &[u8]> = TableDefinition::new("class_members");
const TABLE_CLASS_GROUPS: TableDefinition<&str, &[u8]> = TableDefinition::new("class_groups");
const TABLE_PENDING_INVITES: TableDefinition<&str, &[u8]> = TableDefinition::new("pending_invites");
const TABLE_OUTBOX: TableDefinition<&str, &[u8]> = TableDefinition::new("outbox");
const TABLE_AUDIT_LOG: TableDefinition<&str, &[u8]> = TableDefinition::new("audit_log");

#[derive(Debug, Clone)]
pub struct ClassStore {
    db: Arc<Database>,
}

impl ClassStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    // ── ClassNetwork ──────────────────────────────────────────

    pub fn save_class_network(&self, network: &ClassNetwork) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CLASS_NETWORK)?;
            let val = postcard::to_allocvec(network)?;
            table.insert(CLASS_NETWORK_KEY, val.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_class_network(&self) -> Result<Option<ClassNetwork>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_CLASS_NETWORK) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(CLASS_NETWORK_KEY)? {
            Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn delete_class_network(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CLASS_NETWORK)?;
            table.remove(CLASS_NETWORK_KEY)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ── ClassMember ────────────────────────────────────────────

    pub fn save_member(&self, member: &ClassMember) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CLASS_MEMBERS)?;
            let key = member.node_id.to_string();
            let val = postcard::to_allocvec(member)?;
            table.insert(key.as_str(), val.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_member(&self, node_id: &EndpointId) -> Result<Option<ClassMember>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_CLASS_MEMBERS) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        let key = node_id.to_string();
        match table.get(key.as_str())? {
            Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn load_all_members(&self) -> Result<Vec<ClassMember>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_CLASS_MEMBERS) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let first = match table.first()? {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let range = table.range(first.0.value()..)?;
        let mut members = Vec::new();
        for entry in range {
            let (_, v) = entry?;
            members.push(postcard::from_bytes(v.value())?);
        }
        Ok(members)
    }

    pub fn delete_member(&self, node_id: &EndpointId) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CLASS_MEMBERS)?;
            let key = node_id.to_string();
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ── Group ──────────────────────────────────────────────────

    pub fn save_group(&self, group: &Group) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CLASS_GROUPS)?;
            let val = postcard::to_allocvec(group)?;
            table.insert(group.group_id.as_str(), val.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_group(&self, group_id: &str) -> Result<Option<Group>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_CLASS_GROUPS) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(group_id)? {
            Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn load_all_groups(&self) -> Result<Vec<Group>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_CLASS_GROUPS) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let first = match table.first()? {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let range = table.range(first.0.value()..)?;
        let mut groups = Vec::new();
        for entry in range {
            let (_, v) = entry?;
            groups.push(postcard::from_bytes(v.value())?);
        }
        Ok(groups)
    }

    pub fn delete_group(&self, group_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CLASS_GROUPS)?;
            table.remove(group_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ── PendingInvite ──────────────────────────────────────────

    pub fn save_pending_invite(&self, invite: &PendingInvite) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PENDING_INVITES)?;
            let val = postcard::to_allocvec(invite)?;
            table.insert(invite.invite_id.as_str(), val.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_pending_invite(&self, invite_id: &str) -> Result<Option<PendingInvite>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_PENDING_INVITES) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(invite_id)? {
            Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn load_all_pending_invites(&self) -> Result<Vec<PendingInvite>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_PENDING_INVITES) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let first = match table.first()? {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let range = table.range(first.0.value()..)?;
        let mut invites = Vec::new();
        for entry in range {
            let (_, v) = entry?;
            invites.push(postcard::from_bytes(v.value())?);
        }
        Ok(invites)
    }

    pub fn delete_pending_invite(&self, invite_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PENDING_INVITES)?;
            table.remove(invite_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ── Filtered member queries ─────────────────────────────────

    pub fn load_members_by_status(&self, status: MemberStatus) -> Result<Vec<ClassMember>> {
        let all = self.load_all_members()?;
        Ok(all.into_iter().filter(|m| m.status == status).collect())
    }

    // ── Outbox ──────────────────────────────────────────────────

    pub fn save_outbox_record(&self, record: &OutboxRecord) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_OUTBOX)?;
            let val = postcard::to_allocvec(record)?;
            table.insert(record.message_id.as_str(), val.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_outbox_record(&self, message_id: &str) -> Result<Option<OutboxRecord>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_OUTBOX) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(message_id)? {
            Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn load_pending_outbox(&self, now: u64) -> Result<Vec<OutboxRecord>> {
        let all = self.load_all_outbox()?;
        Ok(all
            .into_iter()
            .filter(|r| {
                matches!(
                    r.status,
                    crate::class_network::messaging::DeliveryStatus::Created
                        | crate::class_network::messaging::DeliveryStatus::Failed
                ) && r.next_retry_at <= now
                    && r.retry_count < r.max_retries
            })
            .collect())
    }

    fn load_all_outbox(&self) -> Result<Vec<OutboxRecord>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_OUTBOX) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let first = match table.first()? {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let range = table.range(first.0.value()..)?;
        let mut records = Vec::new();
        for entry in range {
            let (_, v) = entry?;
            records.push(postcard::from_bytes(v.value())?);
        }
        Ok(records)
    }

    // ── Audit log ───────────────────────────────────────────────

    pub fn save_audit_event(&self, event: &AuditEvent) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_AUDIT_LOG)?;
            let val = postcard::to_allocvec(event)?;
            table.insert(event.event_id.as_str(), val.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_all_audit_events(&self) -> Result<Vec<AuditEvent>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TABLE_AUDIT_LOG) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let first = match table.first()? {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let range = table.range(first.0.value()..)?;
        let mut events = Vec::new();
        for entry in range {
            let (_, v) = entry?;
            events.push(postcard::from_bytes(v.value())?);
        }
        Ok(events)
    }

    /// 便捷方法：写入审计事件
    pub fn audit(&self, actor: &str, action: &str, target: &str, result: &str) {
        let event = AuditEvent {
            event_id: format!(
                "audit-{}-{:04x}",
                crate::util::time_now(),
                rand::random::<u16>()
            ),
            actor: actor.to_string(),
            action: action.to_string(),
            target: target.to_string(),
            timestamp: crate::util::time_now(),
            result: result.to_string(),
        };
        let _ = self.save_audit_event(&event);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iroh::SecretKey;

    use super::*;
    use crate::{
        class_network::model::{ClassMember, ClassNetwork, Group, MemberStatus},
        domain::node::NodeRole,
    };

    fn make_store() -> (ClassStore, tempfile::NamedTempFile) {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let file = temp.reopen().unwrap();
        let db = Database::builder().create_file(file).unwrap();
        let db = Arc::new(db);
        (ClassStore::new(db), temp)
    }

    fn test_network(class_name: &str) -> ClassNetwork {
        ClassNetwork {
            class_id: iroh_gossip::TopicId::from_bytes(rand::random()),
            class_name: class_name.to_string(),
            teacher_node_id: SecretKey::generate(&mut rand::rng()).public(),
            teacher_domain: "teacher-node".to_string(),
            created_at: 42,
        }
    }

    fn test_member(node_id: EndpointId, role: NodeRole) -> ClassMember {
        ClassMember {
            node_id,
            display_name: "Test User".to_string(),
            role,
            status: MemberStatus::Approved,
            groups: vec![],
            joined_at: 42,
        }
    }

    #[test]
    fn network_save_load_delete() {
        let (store, _fd) = make_store();

        let network = test_network("CS101");
        store.save_class_network(&network).unwrap();

        let loaded = store.load_class_network().unwrap().unwrap();
        assert_eq!(loaded.class_id, network.class_id);
        assert_eq!(loaded.class_name, "CS101");
        assert_eq!(loaded.teacher_domain, "teacher-node");

        store.delete_class_network().unwrap();
        assert!(store.load_class_network().unwrap().is_none());
    }

    #[test]
    fn network_not_found_returns_none() {
        let (store, _fd) = make_store();
        assert!(store.load_class_network().unwrap().is_none());
    }

    #[test]
    fn network_save_overwrites() {
        let (store, _fd) = make_store();

        let net1 = test_network("First");
        store.save_class_network(&net1).unwrap();

        let net2 = test_network("Second");
        store.save_class_network(&net2).unwrap();

        let loaded = store.load_class_network().unwrap().unwrap();
        assert_eq!(loaded.class_name, "Second");
    }

    #[test]
    fn member_save_load_delete() {
        let (store, _fd) = make_store();

        let pk = SecretKey::generate(&mut rand::rng()).public();
        let member = test_member(pk, NodeRole::Student);
        store.save_member(&member).unwrap();

        let loaded = store.load_member(&pk).unwrap().unwrap();
        assert_eq!(loaded.node_id, pk);
        assert_eq!(loaded.role, NodeRole::Student);
        assert_eq!(loaded.status, MemberStatus::Approved);

        store.delete_member(&pk).unwrap();
        assert!(store.load_member(&pk).unwrap().is_none());
    }

    #[test]
    fn member_load_all() {
        let (store, _fd) = make_store();

        let ids: Vec<EndpointId> = (0..3)
            .map(|_| SecretKey::generate(&mut rand::rng()).public())
            .collect();

        for &id in &ids {
            store
                .save_member(&test_member(id, NodeRole::Student))
                .unwrap();
        }

        let all = store.load_all_members().unwrap();
        assert_eq!(all.len(), 3);

        for id in &ids {
            assert!(all.iter().any(|m| m.node_id == *id));
        }
    }

    #[test]
    fn member_load_all_empty() {
        let (store, _fd) = make_store();
        let all = store.load_all_members().unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn group_save_load_delete() {
        let (store, _fd) = make_store();

        let group = Group {
            group_id: "group-a".to_string(),
            group_name: "Group A".to_string(),
            members: vec![],
            created_at: 100,
        };
        store.save_group(&group).unwrap();

        let loaded = store.load_group("group-a").unwrap().unwrap();
        assert_eq!(loaded.group_name, "Group A");

        store.delete_group("group-a").unwrap();
        assert!(store.load_group("group-a").unwrap().is_none());
    }

    #[test]
    fn group_load_all() {
        let (store, _fd) = make_store();

        for i in 0..3 {
            store
                .save_group(&Group {
                    group_id: format!("group-{i}"),
                    group_name: format!("Group {i}"),
                    members: vec![],
                    created_at: 100,
                })
                .unwrap();
        }

        let all = store.load_all_groups().unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn group_load_all_empty() {
        let (store, _fd) = make_store();
        let all = store.load_all_groups().unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn store_shares_database_with_storage() {
        // Verify that ClassStore works with a database created via Storage::try_from
        let temp = tempfile::NamedTempFile::new().unwrap();
        let file = temp.reopen().unwrap();
        let storage = crate::storage::Storage::try_from(file).unwrap();
        let class_store = ClassStore::new(storage.db_clone());

        let network = test_network("Shared DB");
        class_store.save_class_network(&network).unwrap();
        assert!(class_store.load_class_network().unwrap().is_some());
    }

    #[test]
    fn pending_invite_save_load_delete() {
        let (store, _fd) = make_store();

        let invite = PendingInvite {
            invite_id: "inv-001".to_string(),
            display_name: "Alice".to_string(),
            shared_secret: "secret123".to_string(),
            created_at: 100,
            expires_at: 200,
        };
        store.save_pending_invite(&invite).unwrap();

        let loaded = store.load_pending_invite("inv-001").unwrap().unwrap();
        assert_eq!(loaded.invite_id, "inv-001");
        assert_eq!(loaded.display_name, "Alice");

        store.delete_pending_invite("inv-001").unwrap();
        assert!(store.load_pending_invite("inv-001").unwrap().is_none());
    }

    #[test]
    fn pending_invite_load_all() {
        let (store, _fd) = make_store();

        for i in 0..3 {
            store
                .save_pending_invite(&PendingInvite {
                    invite_id: format!("inv-{i}"),
                    display_name: format!("Student {i}"),
                    shared_secret: format!("secret{i}"),
                    created_at: 100,
                    expires_at: 200,
                })
                .unwrap();
        }

        let all = store.load_all_pending_invites().unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn load_members_by_status() {
        let (store, _fd) = make_store();

        let pk1 = SecretKey::generate(&mut rand::rng()).public();
        let pk2 = SecretKey::generate(&mut rand::rng()).public();

        let mut approved = test_member(pk1, NodeRole::Student);
        approved.status = MemberStatus::Approved;
        store.save_member(&approved).unwrap();

        let mut pending = test_member(pk2, NodeRole::Student);
        pending.status = MemberStatus::Pending;
        store.save_member(&pending).unwrap();

        let approved_list = store
            .load_members_by_status(MemberStatus::Approved)
            .unwrap();
        assert_eq!(approved_list.len(), 1);
        assert_eq!(approved_list[0].node_id, pk1);

        let pending_list = store.load_members_by_status(MemberStatus::Pending).unwrap();
        assert_eq!(pending_list.len(), 1);
        assert_eq!(pending_list[0].node_id, pk2);

        let invited_list = store.load_members_by_status(MemberStatus::Invited).unwrap();
        assert!(invited_list.is_empty());
    }
}
