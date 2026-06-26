# ADR 0003: Enrollment 与 Ticket 流程

## 状态

已采纳 (2026-06-25)

## 背景

现有 P2P 网络加入流程：学生拿到 Ticket → `p2p-ddns --ticket <TICKET>` → 立即成为网络成员，可以收发消息。教学场景要求：学生加入网络后，必须经过教师审批才能成为 ClassMember 并接收班级消息。

同时，当前部署需要人工配置 6 个独立项（ticket、sharedSecret、transportUrl、allowFrom、OpenClaw plugin、systemd/Docker），目标是人只确认 ticket/加入请求，程序自动配置其余。

## 决策

### 两层加入，一层审批

1. **P2P 层加入**（自动）— 学生凭 JoinTicket 中的 `p2p_ticket` 加入 P2P 网络。daemon 自动完成。
2. **Class 层 Enrollment**（需审批）— 学生发起 JoinRequest。教师 approve 后成为 ClassMember。

P2P 网络内的消息投递不受影响（学生间可直连），但 class 层消息（Broadcast、Multicast、direct message 的 class 权限检查）只对 approved ClassMember 开放。

### JoinTicket 结构

```json
{
  "class_id": "class-001",
  "teacher_node": "teacher-claw",
  "p2p_ticket": "<base64 encoded P2P Ticket>",
  "transport_port": 39091,
  "shared_secret": "<per-member secret>",
  "expires_at": "2026-06-26T00:00:00Z"
}
```

- `p2p_ticket` — 内嵌现有 P2P `Ticket` 的 base64 编码
- `shared_secret` — **per-member 生成**。教师为每个邀请单独生成。移除成员时只需吊销对应 secret
- `expires_at` — 控制 P2P ticket 有效期。过期后不能凭此 invite 加入。已加入 member 不受影响

### 角色传递

- 创建 ClassNetwork 的节点自动成为 `teacher`
- 学生凭 teacher 签发的 JoinTicket 获得 `student` role
- Role 编码在 P2P Ticket 的 invitor Node 中（作为 Node.role）。学生不可自声明为 teacher

### 自动配置

`p2p-ddnsctl class join --invite invite.json` 自动完成：
1. 解析 JoinTicket，提取 P2P Ticket
2. `add-node`（复用现有 P2P 加入逻辑）
3. 配置 transport listener（写入 daemon config）
4. 配置 sharedSecret（per-member）
5. 向教师发 JoinRequest
6. 输出 OpenClaw plugin 配置片段（供 Phase 4 脚本消费）

## 理由

1. **人工审批在正确的位置** — 教师控制谁能成为班级成员。P2P 层让步于教学需求。
2. **不变更现有 Ticket 机制** — 复用 `Ticket.display`/`parse`/`validate`/`refresh_with`。JoinTicket 是上层包装。
3. **per-member secret** — 移除学生不影响其他成员。安全粒度更细。
4. **自动配置** — daemon 只需写自己的 config（transport port、secret）。OpenClaw plugin 配置由脚本消费 JSON 片段完成，daemon 不越界写 OpenClaw 文件。

## 影响

- 新建 `JoinTicket` JSON 结构（定义在 `src/class_network/model.rs`）
- `p2p-ddnsctl` 新增 `class join`、`class invite create`、`class join-requests list`、`class join-requests approve` 子命令
- Admin API 新增 `ClassJoin`、`ClassInvite`、`ClassListJoinRequests`、`ClassApprove` 命令
- Class store 新增 `enrollments` table（redb）
- Ticket 的 invitor Node 需要携带 role 字段
