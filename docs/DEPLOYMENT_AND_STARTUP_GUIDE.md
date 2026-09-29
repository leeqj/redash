# ReDash 下一代多模态控制面：部署与启动实战指南

本文档全面指导 **ReDash** 全栈系统的构建、部署、配置与协同启动，包含 **管控中心 Hub (`redash-server`)**、**受控节点探针 Agent (`redash-agent`)** 以及 **极客桌面客户端 App (`redash-app`)** 的全链路实战。

---

## 一、系统架构与组件矩阵

```
┌─────────────────────────────────────────────────────────────┐
│                    ReDash 全链路协同架构                      │
└─────────────────────────────────────────────────────────────┘

    [受控节点 Agent 1] (边缘节点/NAS)
         │  (Inbound WebSocket: ws://hub:8080/v1/agent/ws)
         │  - 动态自适应遥测 (1s/5s)
         │  - Docker 容器生命周期管理
         │  - 零信任 ED25519 签名验证 + 滑动窗口防重放
         │  - 零端口应急反向 Shell (WebTTY)
         ▼
 ┌───────────────────────────────────────────────────────────┐
 │               ReDash Hub 网关 (redash-server)             │
 │                                                           │
 │  - 端口: 8080 (HTTP / WS / SSE)                           │
 │  - 受控节点心跳守护 (Health Sentinel, 15s/30s 离线熔断)    │
 │  - 自动化反向 WebTTY 隧道桥接 (TTY Bridge)                │
 │  - 120 FPS 纯 Wasm Web 仪表盘 (Canvas/GPU 加速)           │
 └───────────────────────────────────────────────────────────┘
         ▲                                   ▲
         │ (HTTP REST / SSE / WS)            │ (HTTP Web 访问)
         │                                   │
 ┌──────────────────────┐           ┌────────────────────────┐
 │ 桌面客户端 redash-app│           │ 现代 Web 浏览器         │
 │ (macOS / Linux GPUI) │           │ (Chrome / Safari / Edge│
 │                      │           └────────────────────────┘
 │ - 自动化受控节点发现  │
 │ - Ambient Pulse HUD  │
 │ - 零信任密钥本地签名 │
 │ - 1-Click 一键治理   │
 └──────────────────────┘
```

---

## 二、环境依赖与前置条件

1. **操作系统**：
   - Linux (Ubuntu 20.04+, Debian 11+, Rocky/RHEL 8+, Arch Linux)
   - macOS (12 Monterey 及以上，原生支持 Apple Silicon 与 Intel)
2. **编译构建工具**：
   - Rust 1.80+ (推荐使用 `rustup update stable`)
   - `cmake`, `pkg-config`, `freetype` (桌面端 GPUI 渲染库需要)
   - `curl` (用于一键脚本及治理指令分发)
3. **运行时依赖**（可选）：
   - `docker` (若使用容器化部署或受控节点启用 Docker 容器治理)

---

## 三、管控中心 Hub (`redash-server`) 部署

`redash-server` 负责汇聚所有受控节点的遥测心跳、维护控制面节点状态、分发签名治理动作，并提供零端口 WebTTY 桥接和 Web 控制台。

### 1. 编译构建
```bash
# 进入工程根目录
cd redash

# 编译高性能 release 二进制
cargo build --release -p redash-server

# 可执行文件输出路径：target/release/redash-server
```

### 2. 本地直接运行
```bash
# 默认监听 127.0.0.1:8080
./target/release/redash-server

# 监听全网卡 0.0.0.0，指定 8080 端口
./target/release/redash-server --host 0.0.0.0 --port 8080
```

### 3. 环境变量配置
| 变量名 | 说明 | 默认值 |
|---|---|---|
| `REDASH_CONFIG_DIR` | 自定义持久化配置路径（存储 `hosts.json`, `settings.json`） | `~/.local/share/redash/` |
| `RUST_LOG` | 日志级别 (`info`, `debug`, `warn`, `error`) | `info` |

### 4. Systemd 守护进程部署 (生产推荐)
创建服务文件 `/etc/systemd/system/redash-server.service`：
```ini
[Unit]
Description=ReDash Hub Control Gateway
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/var/lib/redash
ExecStart=/usr/local/bin/redash-server --host 0.0.0.0 --port 8080
Restart=always
RestartSec=5s
Environment="RUST_LOG=info"
Environment="REDASH_CONFIG_DIR=/var/lib/redash"
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```
启动并配置开机自启：
```bash
sudo mkdir -p /var/lib/redash
sudo cp target/release/redash-server /usr/local/bin/
sudo systemctl daemon-reload
sudo systemctl enable --now redash-server
sudo systemctl status redash-server
```

### 5. Docker 容器化部署
```bash
# 使用现成 Dockerfile 构建
docker build -t redash-hub -f crates/redash-server/Dockerfile .

# 启动容器并挂载配置目录
docker run -d \
  --name redash-hub \
  --restart always \
  -p 8080:8080 \
  -v /opt/redash/data:/root/.local/share/redash \
  -e RUST_LOG=info \
  redash-hub
```

---

## 四、受控节点探针 Agent (`redash-agent`) 部署

`redash-agent` 安装在各类被监控服务器、Homelab 节点、NAS（如群晖、Unraid、TrueNAS）、家庭工控机或 VPS 上。

> [!NOTE]
> **零端口暴露安全特性**：Agent 始终从节点内部**单向出站连接** Hub，即使节点位于家庭 NAT 宽带、内网穿透或无公网 IP 环境下，均无需在路由器上做任何端口映射！

### 1. 方式一：Docker 一键容器化部署 (推荐 Homelab 用户)
```bash
docker run -d \
  --name redash-agent \
  --restart always \
  --net host \
  --pid host \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  -e REDASH_HUB_URL="ws://<你的HUB_IP>:8080/v1/agent/ws" \
  -e REDASH_NODE_ID="nas-node" \
  -e REDASH_AUTH_TOKEN="your-secure-token" \
  -e REDASH_TRUSTED_PUBKEY="<可选: 客户端桌面导出的 ED25519 公钥HEX>" \
  redash-agent
```

### 2. 方式二：二进制与 Systemd 守护部署
```bash
# 1. 编译 Agent
cargo build --release -p redash-agent

# 2. 将 target/release/redash-agent 复制到目标节点
sudo cp target/release/redash-agent /usr/local/bin/

# 3. 创建配置文件 /etc/redash-agent.env
sudo tee /etc/redash-agent.env > /dev/null <<EOF
REDASH_HUB_URL=ws://<你的HUB_IP>:8080/v1/agent/ws
REDASH_NODE_ID=node-homelab-01
REDASH_AUTH_TOKEN=your-token
# 可选：配置仅受信任的桌面客户端可下发指令
# REDASH_TRUSTED_PUBKEY=70d0...
EOF

# 4. 创建系统服务 /etc/systemd/system/redash-agent.service
sudo tee /etc/systemd/system/redash-agent.service > /dev/null <<EOF
[Unit]
Description=ReDash Lightweight Node Agent
After=network.target docker.service

[Service]
Type=simple
User=root
EnvironmentFile=/etc/redash-agent.env
ExecStart=/usr/local/bin/redash-agent
Restart=always
RestartSec=3s
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

# 5. 启动服务
sudo systemctl daemon-reload
sudo systemctl enable --now redash-agent
```

注册及终端身份必须按 [协议 v2 迁移说明](CONTROL_PLANE_SECURITY.md) 配置：Hub 的 `agent_enrollments.json`、Agent 的 `REDASH_TRUSTED_KEY`/`REDASH_IDENTITY_KEY` 以及桌面的 `agent_keys.json`。旧示例里的固定 token 不再有效，环境文件应设置权限 600。

### 3. Agent 核心命令行参数
```bash
redash-agent [OPTIONS]

选项：
  -h, --hub <URL>        Hub WebSocket 连接地址 (默认: ws://127.0.0.1:8080/v1/agent/ws)
  -n, --node-id <ID>     节点唯一标识 (默认: 主机名)
  -t, --token <TOKEN>    按节点预置的随机注册凭证 (必填，至少 32 字符)
  -k, --trusted-key <PUBKEY>     受信任的桌面端 ED25519 签名公钥 (Hex 格式)
```

---

## 四、极客桌面端 App (`redash-app`) 启动与使用

### 1. 编译与启动桌面应用
```bash
# 确保安装了基础编译依赖，然后启动 GPUI 极客大盘
cargo run --release -p redash-app
```

### 2. 核心特性交互操作指南

#### 1) 接入向导与密钥对
- 点击大盘右上角 `+ 接入受控节点 (Agent)`。
- 系统自动填充基于当前 Hub 地址生成的接入指令（支持 Bash 与 Docker 两种格式），并内置当前客户端唯一的 **ED25519 公钥**。
- 点击 **“复制命令”**，终端会即时显示气泡反馈，在受控主机执行即可完成秒级握手接入。

#### 2) 全局脉搏微指示器 (Ambient Pulse HUD)
- 顶部导航栏右侧常驻 Ambient Pulse 指示胶囊（绿色全部正常 / 黄色预警 / 红色告警）。
- **极客联动**：当发生节点告警时，**直接点击** 该 HUD 指示胶囊，大盘立即自动过滤聚焦到故障节点；再次点击自动恢复全局视图。

#### 3) 一键现场治理 (1-Click Remediation)
- **释放冲突端口**：若服务提示端口占用，点击卡片下方 `释放端口`，Agent 自动定位占用 PID 并完成释放。
- **清理日志超限**：点击 `清理日志`，Agent 自动执行 journalctl 限额裁剪（默认 50MB）。
- **清理废弃容器**：若节点检测到异常 Exited 容器，卡片上方自动高亮浮现 `⚠️ 发现异常退出的容器`，轻点 `🧹 立即清理` 即可调用 `docker prune`。
- **单容器重启**：在容器列表点击 `↺ 重启` 即可完成无损重启。

#### 4) 零端口反向应急终端 (Reverse WebTTY)
- 点击受控节点卡片上的 `⚡ 应急终端` 按钮。
- 应用自动开辟独立的 TTY 标签页，通过 Hub 反向隧道无缝直连节点 Shell。
- **全链路动态缩放**：拖动窗口大小，远程 Shell 的行宽列高动态自适应（`export COLUMNS=... LINES=...`），彻底避免 vim/top/less 界面排版乱码。

---

## 五、Web 控制台直接访问

启动 `redash-server` 后，可以直接通过任何现代 Web 浏览器访问：

```text
http://<HUB_IP>:8080/
```

- **全功能 Wasm 仪表盘**：无需安装任何客户端软件，即可在平板、手机或临时 PC 浏览器中实时监控集群状态与图表流。
- **实时事件推送**：通过 `/v1/control/telemetry/sse` 获取微秒级数据更新。

---

## 六、常见排错排查指南 (Troubleshooting)

### Q1: Agent 提示 `Failed to connect to Hub WebSocket`
- **检查项 1**：确认 Hub 服务已正常运行并监听对应端口（`ss -lptn 'sport = :8080'`）。
- **检查项 2**：确认防火墙开放了 8080 端口（如 `sudo ufw allow 8080/tcp`）。
- **检查项 3**：确认协议使用的是 `ws://`（或启用了反向代理 TLS 时使用 `wss://`），而非 `http://`。

### Q2: 治理指令提示 `Security verification failed: Action rejected: timestamp expired`
- **原因**：Agent 启用了时钟防攻击保护，指令时戳与节点本地系统时间偏差超过 60 秒。
- **解决**：在受控节点上同步 NTP 时间：
  ```bash
  sudo timedatectl set-ntp true
  ```

### Q3: 治理指令提示 `duplicate nonce detected (replay attack thwarted)`
- **原因**：同一条重放的治理动作被再次下发，被滑动窗口防重放缓存成功拦截。
- **解决**：这是正常安全防御行为，每次治理下发均会自动生成唯一的新随机数。

### Q4: 无法读取或治理 Docker 容器
- **原因**：Agent 进程没有读取 `/var/run/docker.sock` 的权限。
- **解决**：
  - 二进制运行模式：将 Agent 运行用户加入 docker 组：`sudo usermod -aG docker $USER`。
  - 容器运行模式：确保启动时包含 `-v /var/run/docker.sock:/var/run/docker.sock:ro`。
