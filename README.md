<p align="center">
  <img src="assets/icon.png" width="128" height="128" alt="ReDash Logo" />
</p>

# ReDash

ReDash 是使用 Rust、GPUI、russh 和 alacritty_terminal 构建的桌面服务器工作台。当前版本为 `0.1.0-beta`，包括 SSH 终端、主机监控、SFTP、批量命令和端口转发。性能尚未做正式基准测试。

## 构建与验证

使用仓库固定的 Rust **1.96.0** 工具链。本次验证平台为 macOS；Linux 和 Windows 桌面端仍需各平台的构建依赖与实机验证。CI 在 macOS 上检查格式、Clippy 和测试。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --bin redash
```

集成测试使用临时目录、内存 SSH/SFTP 服务及本机临时端口，不需要真实服务器或用户凭据。GPUI 回归测试使用测试窗口和隔离配置，覆盖删除、保存失败与凭据恢复入口。限制本地监听的沙箱可能需要放行这些测试。依赖已缓存时可给 Cargo 命令加 `--offline`。

macOS 应用打包：

```sh
./scripts/bundle_mac.sh
open target/release/ReDash.app
```

## 连接与凭据

- 连接前校验 `~/.ssh/known_hosts`，支持指定端口和哈希主机名。未知、变化或被撤销的密钥会被拒绝，错误中提供指纹。请通过可信渠道核验指纹，再使用系统 `ssh` 交互式信任目标主机；ReDash 不自动信任首次出现的密钥。主机证书和 SSH CA 信任尚未支持。
- 密码和私钥口令只持久化到系统凭据库：macOS Keychain、Linux Secret Service 或 Windows Credential Manager。后端失败会明确报错，不降级为文件或仅内存“保存成功”。
- 旧版 `credentials.enc` 只用于兼容迁移。全部凭据成功写入、验证后才删除旧文件；文件损坏、旧环境无法解密、系统凭据不可用或新旧值冲突时保留原件。已有系统凭据仍可读取；请先解决提示的迁移问题再保存新凭据。
- 编辑主机时，密码或口令留空表示保留已有引用。修改连接地址、用户、认证或跳板配置后，受影响的工作台会关闭，批量目标与连接缓存会更新；下一次操作重新验证连接目标。
- 采集提示“登录凭据缺失”时，可点卡片的“编辑凭据”重新填写密码/私钥口令，或改用正确的认证方式。已知缺失的凭据不允许留空保存；丢失的秘密无法仅靠主机配置中的引用恢复。卡片只显示简短提示，完整错误在“详情”中查看。
- ProxyJump 最多支持 8 台主机组成的路由，在获取连接锁前检查环路。连接与认证有完整期限。断线后可重新打开工作台或使用“重新连接当前终端”。PTY 不承诺自动恢复原进程。

首次启动显示空主机列表，合法空配置不会被填入示例。主机配置读取失败会显示错误并禁止覆盖写入；修复文件后重启。设置页的示例重置属于显式操作。

## 功能范围

| 功能 | 当前行为与边界 |
| --- | --- |
| 监控 | Linux `/proc/stat` 差分 CPU 采样、macOS `top` CPU 采样；load average 单独保留。内存、磁盘、接口累计字节、两次采集间网络速率、SSH RTT。 |
| 采集失败 | 显示失败原因与最后成功时间，不生成模拟指标。成功数据与未采集字段分开处理。Windows/Unknown 远端的自动监控明确不支持；SSH/SFTP 可独立使用。 |
| 资源面板 | 采集进程及 Docker 列表/统计；Linux 监听端口依赖 `ss` 或 `netstat`。工具缺失、权限不足或超时会标为未采集。接口累计流量不是月度账单用量。 |
| 监控设置 | 自动刷新、采集间隔、超时、历史长度生效。尚未实现的发光和紧凑布局开关已撤下。 |
| 终端 | 分屏、搜索、剪贴板、可配置字体/光标/滚动历史/选中复制；回传终端查询响应，处理应用光标模式和括号粘贴。输入队列溢出会显示错误，输出使用有界队列。 |
| Agent 信息 | 从当前终端标题与输出启发式识别，状态与 token/cost 显示不代表权威计费或完整进程生命周期。Y/N 快捷响应只在当前可见提示明确为 Y/N 时可用；不提供通用“始终允许”映射。 |
| SFTP | 浏览、预览、重命名、权限修改、排他新建和删除确认。预览最多读取前 128 KiB；完整 UTF-8 编辑上限 2 MiB，保留统一 CRLF/LF。超限或无效 UTF-8 不允许全量保存。 |
| SFTP 保存 | 仅替换普通文件，保留 POSIX 属主、属组与权限，使用 `posix-rename@openssh.com`。不支持该扩展时拒绝编辑保存。重命名失败保留原件与完整临时副本，并在错误中给出恢复路径；不自动删除原文件。SFTP v3 无法保证 ACL/xattr 等扩展属性保留。 |
| 批量命令 | 校验退出码、信号退出和请求拒绝；超时覆盖连接和执行。单条命令输出上限 8 MiB，超限会失败并请求关闭信道。 |
| 取消与超时 | 停止本地子任务，并请求远端 TERM/信道关闭。脱离 SSH 会话的后台进程仍可能运行，不能把“停止等待”视为远端已确认终止。 |
| 隧道 | 本地转发、SOCKS5、真实 SSH 心跳 RTT。管理对象持有接收/转发任务；关闭工作台会释放其隧道，停止操作等待监听任务退出。 |
| 告警 | CPU/内存/磁盘阈值及采集不可用通知；macOS 桌面通知、通用 JSON Webhook。HTTP 非 2xx 视为失败；失败记录日志，30 秒后可重试，成功冷却 5 分钟。 |

Webhook 发送 `AlertEvent` JSON（`host_id`、`host_name`、`alert_type`、`message`、`timestamp`）。飞书、钉钉、Telegram 等专有机器人接口需要适配这个格式，不能直接假定兼容。

## 代码结构

- `crates/redash-core`：配置与凭据、SSH/PTY/隧道、SFTP、探针、告警、批量作业。无需 GUI 即可测试。
- `crates/redash-app`：GPUI 窗口与视图、终端渲染、输入和界面状态。
- `crates/redash-core/tests`：内存协议服务和异常路径回归测试。
- `.github/workflows/ci.yml`：格式、严格 Clippy、全量测试。

详细审查见 [CODE_REVIEW.md](CODE_REVIEW.md)，对应修复与验证记录见 [FIXES.md](FIXES.md)。生产使用前仍需对实际 SSH/SFTP 服务器、系统凭据库和桌面交互进行联调。
