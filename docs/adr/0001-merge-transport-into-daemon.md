# ADR 0001: 合并 agent-transport 进 daemon

## 状态

已采纳 (2026-06-25)

## 背景

当前 `p2p-ddns-agent-transport` 是独立二进制，与 `p2p-ddns` daemon 分开部署。它负责：

- 监听 HTTP `:39091`，提供 `/send`、`/contacts`、`/events`、`/inbox` 端点
- 通过 daemon admin API 查询节点信息（resolve node、query nodes）
- 转发消息到目标节点的 transport `/inbox`

教学内网消息系统改进计划（Phase 2-8）将在 daemon 内新增 ClassManager 模块，需要 transport 的 `/send` 路径经过 ClassManager 做权限检查、outbox 记录、ACK 追踪。transport 与 daemon 的耦合将显著加深。

## 决策

将 `p2p-ddns-agent-transport` 合并进 `p2p-ddns` daemon 进程。transport HTTP listener 成为 daemon 的可选组件（类似现有 admin HTTP listener）。通过新 CLI flag 启用：

```bash
p2p-ddns --agent-transport-bind 0.0.0.0:39091
```

## 理由

1. **消除 IPC 开销** — transport 每次 `/send` 需要查询 ClassManager（权限检查）+ class store（member 验证）。独立进程 = HTTP/admin API 往返；同进程 = 直接函数调用。

2. **简化部署** — 一个 systemd unit 替代两个。学生/教师部署少一个进程。启动顺序问题消失。

3. **状态一致性** — transport 和 class store 共享同一个 redb 实例。不需要考虑缓存失效或跨进程同步。

4. **配置简化** — sharedSecret、transportUrl 等不再需要在两个进程间复制。

## 替代方案

**保持独立进程，通过 admin API 通信** — transport `/send` handler 先调 daemon admin API 做 class 权限检查，再投递。

- 优点：进程隔离，可独立重启 transport
- 缺点：每次 `/send` 多一次 IPC 往返；admin API 需要新增大量 class 查询接口；配置同步复杂；启动顺序敏感

被否决。成本大于收益。教学场景对延迟不敏感，但 IPC 引入的故障模式（admin API 不可用、超时处理、重试语义）更复杂。

## 影响

- `src/agent_transport.rs` 不再编译为独立二进制，编译进 daemon
- `src/bin/p2p-ddns-agent-transport.rs` 移除
- systemd packaging 只需要一个 service unit
- OpenClaw plugin 的 `transportUrl` 默认指向 daemon 的 transport listener
- `src/main.rs` 新增 `--agent-transport-bind` flag 和对应 listener 启动逻辑
