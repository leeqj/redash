# 控制面协议 v2 与旧版本迁移

协议 v2 有意拒绝旧的匿名终端、明文帧、未签名 Open 和旧 E2EE 握手。Hub、Agent、桌面须一起升级。

## 身份与配置

三种凭据承担不同职责，不能互换：

1. **客户端 Ed25519 身份**：桌面保存 `control_plane_key.json`，新文件为 `version: 2`，由系统安全随机源生成，Unix 权限为 600。Agent 的 `REDASH_TRUSTED_KEY` 是它的公钥，用于验证治理动作和终端握手。
2. **每节点 enrollment token**：由桌面接入弹窗生成 32 字节随机 token，分别配置在 Agent 和 Hub。它只授予该节点连接身份，不能授权 shell。
3. **Agent Ed25519 身份**：接入弹窗为该节点生成随机密钥。私钥通过安装命令配发为 `REDASH_IDENTITY_KEY`；公钥在复制安装命令时固定到桌面的 `agent_keys.json`。客户端验证 Agent 对完整握手的签名，不能仅因 Hub 返回一个公钥就信任它。

默认配置文件位置与 `HostStore::default_path()` 位于同一目录。可以通过 `REDASH_ENROLLMENTS_FILE` 指定 Hub 注册文件，通过 `REDASH_AGENT_KEYS_FILE` 指定桌面信任文件，通过 `REDASH_HUB_URL` 指定桌面 Hub 地址。

Hub 注册文件示例（使用实际生成的值，不要复用示例占位符）：

```json
{
  "node-example": {
    "auth_token": "<该节点独立的随机 token，至少 32 个字符>",
    "trusted_public_key": "<桌面 Ed25519 公钥，64 个十六进制字符>"
  }
}
```

保存为权限 600 的 `agent_enrollments.json`，将接入弹窗生成的条目合并进现有文件，保留其他节点，重启 Hub 加载。空文件、缺失文件或无效 JSON 关闭注册，不启用默认 token。已配置的节点 ID 与 token 绑定；不能用一个节点的 token 注册其他 ID。一个节点同一时间只有一个有效连接。

桌面信任文件示例：

```json
{
  "node-example": "<配发给该 Agent 的身份公钥，64 个十六进制字符>"
}
```

Agent 环境变量：

```text
REDASH_HUB_URL=wss://hub.example.com/v1/agent/ws
REDASH_NODE_ID=node-example
REDASH_AUTH_TOKEN=<该节点的随机 token>
REDASH_TRUSTED_KEY=<桌面公钥>
REDASH_IDENTITY_KEY=<Agent 私钥>
```

Linux 安装器将这些值写到 `/etc/redash-agent.env`（600），systemd unit 只引用环境文件。不要公开安装命令、注册 token 或 Agent 私钥。macOS 安装器只安装二进制，运行/服务设置仍由操作员配置。

未提供可信客户端公钥时，Agent 可上报遥测，但所有远程治理和终端操作都拒绝。空字符串按未配置处理。空环境变量 node ID 会回退为主机名生成的 ID；显式空 CLI node ID 和非法 ID 拒绝。Compose 对未配置 token 给出明确配置错误。

## 旧密钥轮换

之前的无版本 `control_plane_key.json` 可能由时间戳/PID 构造。新版识别后拒绝用于签名，保留原文件并显示迁移提示，**不会自动覆盖用户身份文件**。

操作员应在保留所需备份后将旧密钥移出原路径，重启桌面生成 v2 随机身份，更新 Hub 注册文件的客户端公钥和所有 Agent 的 `REDASH_TRUSTED_KEY`，并重新配发节点身份与随机 token。旧身份不应继续作为信任锚使用。Agent 公钥轮换需明确更新本地 pin；复制命令不会覆盖已经存在的不同 pin。

## 会话状态机

`连接 → 带客户端签名的 Init → 带 Agent 身份签名的 Ack → 加密 Open → 加密 Input/Resize/Close`

- 客户端生成唯一 session ID，Hub 用同一个 ID 注册订阅，Agent 用同一个 ID 关联 cipher 与 PTY。
- 双方握手签名包含节点、版本、时间、nonce、session 和临时密钥；Agent 校验时间窗口并记录已验证 nonce。
- X25519 拒绝低阶公共点；HKDF-SHA256 绑定完整 transcript，派生双向独立 AES-256-GCM 密钥。
- Envelope 验证 session、方向、nonce、认证标签和严格递增序号；认证失败不推进序号。
- 所有控制帧都在 AEAD 内。顶层发送 Envelope，禁止序列化 `Result` 包装；无效 JSON、Binary 明文及旧控制帧不会进入 shell。
- 连接及握手有 5 秒超时；未 Open 的会话约 10 秒过期。匿名连接和竞速落败路径均不能创建 shell。
- PTY 断开或过期后清理；重新连接需要新握手和新 shell，尚无自动恢复旧进程。

## 2026-09-29 review 修复对照

| 报告项 | 修复与主要验证位置 |
| --- | --- |
| 1–2：LAN/Hub 未认证执行 | 统一 E2EE 控制帧；`control_plane_security` 真实匿名拒绝与双路径 PTY 测试 |
| 3：注册冒用与跨节点消息 | 持久化 enrollment 配置、连接实例绑定；注册/遥测/终端归属回归 |
| 4：弱随机密钥 | OS CSPRNG；v2 身份存储，旧密钥拒绝与迁移 |
| 5–6：伪造 Ack、重放、反射 | Agent pin、双向签名、HKDF、AEAD、方向密钥和单调序号；密码协议拒绝测试 |
| 7–8：session 不一致、Result 格式 | 单一 session ID、握手超时、明确处理 seal 错误；真实 Client→Hub→Agent 测试 |
| 9：WSS 缺失 TLS | Agent/core 显式启用 Rustls + WebPKI roots |
| 10：管道终端 | 系统 PTY、控制终端、resize ioctl、进程回收；test -t、stty size、Ctrl-C 测试 |
| 11：离线节点消失 | 保留节点及最后样本，断连立即 Offline，旧连接清理隔离 |
| 12：macOS 假指标 | 原生系统采样、warmup/缺失有效性标记，桌面不可用显示 |
| 13：Agent 阈值漏报 | 统一 AlertDispatcher 评估/发送；Agent-only 本地 webhook、冷却及恢复测试 |
| 14：空 Compose 配置 | 空值规范化、token 明确必填、node ID 校验 |
| 15：端口不可输入 | 焦点、数字输入、粘贴、选中、光标和删除处理 |
| 16：失败误报成功 | 共享 HTTP executor，超时、HTTP 状态及 ActionResult 身份/业务状态解析 |
| 17：固定时间戳 | 纯状态机显式注入 timestamp/nonce，桌面使用实时签名 helper |
| 18：nonce/动作 ID 冲突 | 随机 nonce 和包含完整 nonce 的 action ID；签名成功后才记 nonce，Hub 拒绝覆盖 pending action |

验收命令：`cargo test --workspace --locked`、`cargo fmt --all -- --check`、原生及 wasm32 的严格 Clippy。网络回归使用真正的本地监听、Hub、Agent、PtyChannel 和 webhook 接收器；不能监听时测试失败，不跳过。运行结果应以本次实际日志为准。
