# CONTEXT.md — p2p-ddns 教学内网消息系统

## 项目领域术语表

本文档定义教学场景消息系统的核心概念。所有代码、CLI、文档必须使用这些命名。

---

### 网络层概念

- **Node** — p2p-ddns 网络上的一个对等节点。由 `node_id`（iroh `EndpointId`）唯一标识。有 `domain`（机器名/主机名）、`addr`（当前网络地址）、`services`（注册的服务端口表）。定义在 `src/domain/node.rs`。

- **Ticket** — P2P 网络引导凭证。内含 `topic`（gossip 频道 ID）、`rnum`（随机数验证）、`invitor`（邀请者 Node 信息）。base64 编码传递。一个 Node 凭 Ticket 加入一个 P2P 网络。定义在 `src/domain/ticket.rs`。

- **Topic** — P2P gossip 网络频道。由 `iroh-gossip::TopicId` 标识。一个 Topic = 一个节点发现和同步域。

- **NodeRole** — 节点在教学场景中的角色标签。存储在 `Node.role` 字段。可选值：`teacher`、`student`。未来可扩展 `assistant`、`admin`。Role 由 Ticket 携带：创建 ClassNetwork 的节点自动成为 teacher；学生凭 teacher 签发的 Ticket 获得 student role。

- **ClientPermissions** — 节点的 P2P 层管理权限（`can_query`、`can_add_node`、`can_remove_node`、`can_control`）。独立于 NodeRole，管网络拓扑操作。定义在 `src/domain/client.rs`。

---

### 班级层概念

- **ClassNetwork** — 一个教学班级。与 P2P Topic **1:1 映射**：一个 ClassNetwork = 一个 P2P 网络。由 `class_id` 标识（源于 TopicId）。教师创建 ClassNetwork → 生成 P2P Topic + Ticket。

- **ClassMember** — 被批准加入 ClassNetwork 的学生或教师。存储 `node_id`、`display_name`（人类可读名，如 "张三"）、`role`（teacher/student）、`status`（审批状态）、`groups`（所属小组列表，`Vec<String>`）。教师也是 ClassMember（role=teacher）。

- **Enrollment** — 学生从受邀到成为 ClassMember 的完整生命周期状态机。状态：`invited` → `requested` → `approved` → `active` → `removed`。Enrollment 追踪整个过程，ClassMember 是 `approved` 后的最终稳态。

- **JoinTicket** — 教师签发的班级加入凭证。**包装** P2P Ticket，附加 class 级元数据：`class_id`、`teacher_node`、`p2p_ticket`（内嵌 Ticket）、`transport_port`、`shared_secret`、`expires_at`。JSON 格式，可导出为 invite.json。

- **JoinRequest** — 学生凭 JoinTicket 发起的加入申请。学生侧触发，教师侧审批。由 ClassNetwork 的 teacher 节点处理。

- **Group** — 班级内的小组。由 `group_id` 标识。教师创建和管理。一个 ClassMember 可属于多个 Group（`groups: Vec<String>`）。Group 状态以教师节点为权威源，学生节点缓存。

- **OpenClawGateway** — 消费者的概念：运行 OpenClaw 的平台。消费 transport 提供的消息 channel。**不是领域概念**，不进入 class 层数据模型。

---

### 消息层概念

- **DirectMessage** — 点到点消息。一个发送者 → 一个接收者。现有 transport `/send` `/inbox` 机制支持。

- **Broadcast** — 教师向全班发送的消息。1 条 Message + N 条 Delivery（每 ClassMember 一条）。实现为 fan-out：class manager 遍历全班 member，逐个投递。

- **Multicast** — 教师向一个 Group 发送的消息。1 条 Message + M 条 Delivery（仅该 Group 的 members）。实现为 fan-out with group filter。

- **Delivery** — 一条消息到一个接收者的投递记录。追踪 per-recipient 状态。用于 Broadcast/Multicast 的每个目标追踪。

- **DeliveryStatus** — Delivery 的生命周期状态：`created` → `sent` → `accepted` → `delivered` → `processed`。`failed` 和 `expired` 为终止态。`accepted` = 到达对方 transport（HTTP 200），`delivered` = 对方 class manager 确认收到，`processed` = 对方业务层确认处理（业务 ACK）。

- **Outbox** — 发送方 class manager 的持久化发送队列。消息发出前写入。支持重试。存储在 class manager 层（daemon 内，redb 新 table）。

- **Inbox** — 接收方 class manager 的持久化接收队列。消息收到后写入。防重复处理（message_id 去重）。

- **MessageId** — 全局唯一的消息标识符。格式：`{timestamp}-{counter}`。由发送方 transport/class manager 生成。

- **BusinessAck** — 独立的业务层确认。与 transport 层 ACK（HTTP 200）区分。接收方 class manager 发送 BusinessAck 后，Delivery 状态从 `delivered` 变为 `processed`。

- **AuditEvent** — 所有发送、接收、权限操作的日志记录。存储 `actor`、`action`、`target`、`timestamp`、`result`。用于审计和排错。

---

### 安全概念

- **sharedSecret** — transport HTTP API 的身份验证凭证。**per-member 生成**（每个学生/教师独立 secret）。JoinTicket 中携带。移除 ClassMember 时吊销对应 secret。支持 secret 轮换。

- **allowFrom** — OpenClaw plugin 层白名单。由 class manager 自动同步维护（teacher approve → 加入 allowFrom；remove → 移除）。是 class member status 的视图缓存，非独立权限源。

- **ErrorCode** — 权限拒绝时的语义错误码。HTTP 统一返回 `403`，body 中 `error_code` 字段区分：`MISSING_SECRET`、`INVALID_SECRET`、`NOT_APPROVED`、`REMOVED`、`ROLE_FORBIDDEN`、`NOT_CLASS_MEMBER`。

---

### 架构概念

- **ClassManager** — daemon 内的新模块（`src/class_network/`）。管理 ClassNetwork 存储、Enrollment 流程、Group 管理、Outbox/Inbox、Broadcast/Multicast、Audit。与现有 P2P 层、Storage 层独立。

- **TransportEndpoint** — daemon 的新可选 listener（合并自原 `p2p-ddns-agent-transport` 独立二进制）。监听 `0.0.0.0:39091`，提供 `/send`、`/contacts`、`/events`、`/inbox`、`/ack` HTTP endpoint。内部调用 ClassManager 做权限检查、outbox 记录、ACK 处理。

- **AdminAPI** — daemon 的现有管理接口（Unix socket + 可选 HTTP）。新增 class 子命令：`class create`、`class join`、`class members`、`class groups`、`class broadcast` 等。通过 `p2p-ddnsctl` CLI 访问。

---

### 关键设计决策

1. **Node 加 `role` 字段**（不复用 `services` map）。type-safe、显式。
2. **ClassNetwork : P2P Topic = 1:1**。复用现有 ticket 机制。
3. **JoinTicket 包装 Ticket**。P2P 层不感知 class 语义。
4. **ClassManager 在 daemon 进程内**（不独立二进制）。共享 storage、ticket、无 IPC 开销。
5. **Transport 合并进 daemon**。一个进程，一个 systemd unit（见 [ADR 0001](docs/adr/0001-merge-transport-into-daemon.md)）。
6. **per-member sharedSecret**。移除学生不需全班换 secret。
7. **两层协议**：P2P 层（iroh 协议，节点发现/同步）+ Class 层（transport HTTP，教学消息）。
8. **两层权限**：`ClientPermissions`（P2P 网络管理）+ `NodeRole`（教学操作）。
9. **domain ≠ display_name**。`domain` 是机器标识，`display_name` 是人类可读名。
10. **广播 = 1 Message，N Delivery**。per-recipient 状态追踪。
11. **Outbox/Retry 在 ClassManager**，transport 保持简单转发。
12. **双 ACK**：transport ACK（HTTP 200 = `delivered`）+ BusinessAck（`processed`）。
