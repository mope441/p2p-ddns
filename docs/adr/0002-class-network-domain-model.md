# ADR 0002: 班级网络领域模型

## 状态

已采纳 (2026-06-25)

## 背景

现有 `p2p-ddns` 的领域模型是纯 P2P 网络的：`Node`（对等节点）、`Ticket`（网络引导凭证）、`Message`（P2P 协议消息，如 Invited/SyncRequest/Heartbeat）。教学场景引入了新概念：班级、教师、学生、小组、广播。

需要决定这批新概念如何映射到现有模型 — 是在现有类型上加字段，还是新建独立类型。

## 决策

分层模型。三层独立：

### 第 1 层：P2P 网络层（不改）

- `Node` — 加 `role: Option<NodeRole>` 字段（`teacher` | `student`）。`domain` 保留为机器标识。
- `Ticket` — 不改。保持 `topic + rnum + invitor` 语义。
- `Message` enum — 不改。保持现有 P2P 协议消息类型。

### 第 2 层：班级层（新建 `src/class_network/`）

- `ClassNetwork` — 班级，1:1 映射到 P2P Topic
- `ClassMember` — 班级成员。`node_id` + `display_name` + `role` + `status` + `groups`
- `Enrollment` — 加入生命周期状态机。`invited → requested → approved → active → removed`
- `Group` — 班级内小组
- `JoinTicket` — 包装 `Ticket`，加 class 元数据。JSON 格式
- `JoinRequest` — 学生申请加入

### 第 3 层：消息层（新建，class_network 子模块）

- `ClassMessage` — 教学消息。`DirectMessage` | `Broadcast` | `Multicast`
- `Delivery` — per-recipient 投递状态
- `Outbox` / `Inbox` — 持久化消息队列
- `AuditEvent` — 审计日志

### 关键关系

- `ClassNetwork` 与其 P2P Topic **1:1 绑定**
- `JoinTicket` **包装** `Ticket`（不是替代）
- `ClassMember.role`（教学权限）与 `ClientPermissions`（P2P 管理权限）**独立**
- `Node.domain`（机器名）与 `ClassMember.display_name`（人类名）**独立**

## 理由

1. **不改 P2P 层** — 保持 NetDev/DDNS 场景向后兼容。现有用户不加 `--class` flag 不受影响。
2. **分层清晰** — 每层有明确职责，不交叉污染。Class 层不知道 iroh gossip 细节，P2P 层不知道"班级"存在。
3. **类型安全** — `NodeRole` 是 enum 而非 services map 里的 string。编译器 enforce 合法值。
4. **JoinTicket 包装而非替代** — 复用现有 Ticket 的序列化/验证/刷新逻辑。

## 影响

- `Node` struct 加 `role: Option<NodeRole>`
- 新建 `src/class_network/` 模块目录
- 新建 `ClassMessage` 和 `DeliveryStatus` 类型（与 `src/domain/message.rs` 并存但不合并）
- CONTEXT.md 收录全部术语
