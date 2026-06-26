# OpenClaw 教学内网消息系统改进计划初稿

## 0. 开发原则

本次改造不能一次性实现完整 IM 系统。目标是把现有 `p2p-ddns + p2p-ddns-agent-transport + openclaw-p2p-ddns-lan` 从“节点级单播消息通道”逐步演进为“教学场景下的班级内网消息系统”。

开发原则：

1. 每一步必须是可运行、可测试、可回滚的 vertical slice。
2. 每个 phase 都必须包含测试或手动验证命令。
3. 先补领域模型和自动配置，再做广播/组播。
4. 不先做 UI，先做 CLI/API 和稳定底层能力。
5. 保持模块边界清晰：发现层、传输层、班级管理层、OpenClaw 插件层不要混在一起。

---

# Phase 1：项目领域模型与共享语言

## 目标

先建立教学场景的核心概念，避免后续代码里出现混乱命名。

## 新增或整理的领域概念

* `TeacherClaw`
* `StudentClaw`
* `ClassNetwork`
* `ClassId`
* `ClassMember`
* `JoinRequest`
* `JoinTicket`
* `Group`
* `Broadcast`
* `Multicast`
* `DeliveryStatus`
* `Enrollment`
* `NodeRole`

## 产出

新增文档：

```text
CONTEXT.md
docs/adr/0001-class-network-domain-model.md
docs/adr/0002-enrollment-and-ticket-flow.md
```

## 验收标准

* 文档中明确区分：

  * p2p-ddns node
  * OpenClaw gateway
  * teacher/student role
  * class member
  * group member
* Claude Code 后续新增代码必须使用这些命名，不能随意发明新的同义词。

---

# Phase 2：班级网络数据模型与本地存储

## 目标

增加班级、成员、分组的本地状态存储。先不做广播，只做状态管理。

## 建议实现

在 `p2p-ddns-agent-transport` 或新增 class manager 模块中增加本地存储。第一版可以用 JSON 文件或 SQLite。推荐 SQLite，但为了最小改动，可以先用 JSON。

建议目录：

```text
src/class_network/
  mod.rs
  model.rs
  store.rs
  api.rs
```

## 核心数据结构

```json
{
  "class_id": "class-001",
  "class_name": "AI Course",
  "teacher": {
    "node_id": "teacher-claw",
    "domain": "teacher"
  },
  "members": [
    {
      "node_id": "student-a",
      "display_name": "Alice",
      "role": "student",
      "status": "approved",
      "joined_at": "..."
    }
  ],
  "groups": [
    {
      "group_id": "group-a",
      "group_name": "Group A",
      "members": ["student-a", "student-b"]
    }
  ]
}
```

## 新增 CLI 或 HTTP API

```bash
class create
class info
class members list
class groups list
```

或者 HTTP：

```text
GET  /class
POST /class
GET  /class/members
GET  /class/groups
```

## 验收标准

* 教师节点可以创建班级。
* 学生节点可以保存自己所属班级。
* 重启服务后班级状态不丢失。
* 有单元测试覆盖 class store 的增删改查。

---

# Phase 3：自动加入与人工确认 ticket 流程

## 目标

保留 ticket 人工确认，但其他配置自动完成。

当前问题是：ticket、sharedSecret、transportUrl、allowFrom、OpenClaw plugin、systemd、Docker 网络配置都需要人工处理。目标是：

```text
人只确认 ticket / 加入请求
程序自动配置其余部分
```

## 教师侧流程

教师 Claw 创建班级后生成加入邀请：

```bash
openclaw-class invite create --class class-001
```

输出：

```json
{
  "class_id": "class-001",
  "teacher_node": "teacher-claw",
  "p2p_ticket": "...",
  "transport_port": 39091,
  "shared_secret": "...",
  "expires_at": "..."
}
```

## 学生侧流程

学生执行：

```bash
openclaw-class join --invite invite.json --student-name Alice
```

学生侧自动完成：

* 加入 p2p-ddns 网络
* 配置 `p2p-ddns-agent-transport`
* 配置 OpenClaw plugin
* 配置 `transportUrl`
* 配置 `sharedSecret`
* 向教师提交 join request

## 教师确认

教师查看：

```bash
openclaw-class join-requests list
```

教师批准：

```bash
openclaw-class join-requests approve --student student-a
```

## 验收标准

* 学生加入时只需要输入或导入教师提供的 invite。
* 教师必须人工 approve，学生才成为 class member。
* approve 后教师与学生可以互相 direct message。
* 失败时能输出明确诊断：ticket 错、secret 错、transport 不通、OpenClaw plugin 未启用。

---

# Phase 4：OpenClaw 插件与部署自动化

## 目标

把当前复杂部署过程封装成脚本或命令。

## 新增脚本

```text
scripts/install-teacher-node.sh
scripts/install-student-node.sh
scripts/configure-openclaw-plugin.sh
scripts/diagnose-openclaw-p2p.sh
```

## 自动检测内容

脚本需要自动检测：

* OpenClaw 是宿主机部署还是 Docker 部署
* Docker 是否是 host network
* 如果是 bridge network，自动使用 `host.docker.internal`
* Docker compose 是否已有 `extra_hosts`
* OpenClaw config 挂载目录在哪里
* 插件是否已经 install / enable
* transport 是否监听 `0.0.0.0:39091`
* sharedSecret 是否一致

## 验收标准

一条命令可以配置教师节点：

```bash
./scripts/install-teacher-node.sh --node teacher-claw --class "AI Course"
```

一条命令可以配置学生节点：

```bash
./scripts/install-student-node.sh --node student-a --invite invite.json
```

诊断命令可以输出：

```text
p2p-ddns.service: active
p2p-ddns-agent-transport.service: active
OpenClaw container: running
OpenClaw plugin: enabled
transportUrl: reachable
contacts API: OK
directory peers: OK
```

---

# Phase 5：分组管理

## 目标

支持教师在班级内创建小组，并维护小组成员。

## 教师侧命令

```bash
openclaw-class group create --class class-001 --name "Group A"
openclaw-class group add-member --group group-a --student student-a
openclaw-class group remove-member --group group-a --student student-a
openclaw-class group list
openclaw-class group members --group group-a
```

## 规则

* 只有教师可以创建和修改分组。
* 学生可以查询自己所在小组。
* 一个学生可以属于多个小组，但第一版可以先限制为一个主小组。
* 分组状态以教师节点为权威状态，学生节点只缓存。

## 验收标准

* 教师可以创建多个小组。
* 教师可以把学生加入或移出小组。
* 学生节点可以收到自己的分组状态更新。
* 重启后分组状态不丢失。

---

# Phase 6：班级广播与小组组播

## 目标

教师可以向全班广播，也可以向某个小组组播消息。

第一版不要实现复杂 multicast 协议，直接 fan-out：

```text
broadcast = 对班级成员逐个 direct send
multicast = 对小组成员逐个 direct send
```

## 新增命令

```bash
openclaw-class broadcast --class class-001 --message "今天作业已发布"
openclaw-class multicast --group group-a --message "A组准备展示"
openclaw-class send --student student-a --message "请单独检查实验"
```

## 返回结果

```text
Class broadcast: class-001
Total: 30
Sent: 28
Failed: 2

Failed:
- student-k: timeout
- student-m: forbidden
```

## 验收标准

* 教师能向全班发送消息。
* 教师能向指定小组发送消息。
* 学生不能调用 broadcast/multicast。
* 发送结果必须显示每个目标的成功/失败状态。
* 小组组播不能发给非该组成员。

---

# Phase 7：可靠消息、ACK、重试和审计

## 目标

把当前 best-effort direct send 改造成教学场景可接受的可靠消息机制。

## 新增能力

* 全局 `message_id`
* `outbox`
* `inbox`
* ack
* retry
* delivery status
* audit log

## 状态模型

```text
created
sent
accepted
delivered
processed
failed
expired
```

## 建议新增表或文件

```text
messages
delivery_attempts
inbox
outbox
audit_events
```

## 重试策略

```text
1st retry: 5s
2nd retry: 15s
3rd retry: 60s
then exponential backoff up to 5min
max retries configurable
```

## 新增命令

```bash
openclaw-class message status --message-id xxx
openclaw-class message retry --message-id xxx
openclaw-class audit list --class class-001
```

## 验收标准

* 重复消息不会重复处理。
* 发送失败会进入 outbox。
* 网络恢复后可以自动重试。
* 教师可以查看一条广播中每个学生的投递状态。
* 所有发送和接收事件都有审计日志。

---

# Phase 8：权限安全与角色治理

## 目标

把 sharedSecret 和 allowFrom 从临时安全策略升级为面向班级的权限模型。

## 角色

```text
teacher
student
assistant
admin
```

第一版只需要：

```text
teacher
student
```

## 权限规则

教师可以：

* 创建班级
* 审批学生加入
* 创建/修改分组
* 广播全班
* 组播小组
* 移除学生

学生可以：

* 加入班级
* 接收班级消息
* 接收小组消息
* 给教师发送 direct message
* 查询自己的班级和分组状态

学生不能：

* 广播全班
* 修改分组
* 审批其他学生
* 伪造 teacher message

## 安全增强

* 强制配置 sharedSecret
* 支持 secret 轮换
* 节点身份和 class member 绑定
* 从班级移除学生后自动吊销权限
* 所有敏感操作写入 audit log

## 验收标准

* 学生调用 broadcast 会被拒绝。
* 未批准学生不能接收班级广播。
* 被移除学生不能继续接收消息。
* 权限错误返回明确错误码，不只是 `Forbidden`。

---

# 建议给 Claude Code 的执行顺序

不要一次性执行所有 Phase。建议按下面顺序逐步交给 Claude Code：

## Step 1

只做 Phase 1：领域模型文档和 ADR。不要改业务代码。

## Step 2

只做 Phase 2：班级网络 store 和测试。不要接 OpenClaw。

## Step 3

只做 Phase 3：teacher invite、student join、teacher approve 的最小闭环。

## Step 4

只做 Phase 4：部署自动化和诊断脚本。

## Step 5

只做 Phase 5：分组管理。

## Step 6

只做 Phase 6：广播和组播 fan-out。

## Step 7

只做 Phase 7：可靠消息、ACK、重试、审计。

## Step 8

只做 Phase 8：权限、安全和角色治理。

每一步完成后必须运行测试和 smoke test，不允许跨 phase 同时重构大量代码。