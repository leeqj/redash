# ReDash 审查修复记录

日期：2026-09-26。对应 [CODE_REVIEW.md](CODE_REVIEW.md) 中的 27 项问题（10 项 P1、17 项 P2）。这些问题均已落实代码修复；本机格式检查、严格 Clippy 和 158 个测试通过。协议与持久化边界使用 mock、临时目录及本机临时端口验证，GUI 和真实服务器兼容性仍需联调。

保留原有 `redash-core` / `redash-app` 分层，复用 russh、russh-sftp、keyring、Alacritty 和 Tokio。没有引入新的运行时框架；新增的 `rand` 仅用于测试生成 SSH 主机密钥。

## 逐项对应

| 编号 | 问题 | 已实现的修复 | 主要位置 |
| --- | --- | --- | --- |
| 1 · P1 | 失败时生成模拟监控数据 | 移除失败分支的模拟指标；记录错误与最后成功时间，保留真实历史数据，生成采集不可用告警；未采集不显示为 0。 | `redash-app/src/main.rs`、`views/fleet_view.rs`、`views/workbench.rs` |
| 2 · P1 | 无条件接受 SSH 主机密钥 | 使用 `~/.ssh/known_hosts` 验证主机名、端口与密钥；未知、变化、撤销密钥及未支持的主机证书均拒绝。直接连接和跳板目标使用同一验证器。 | `redash-core/src/session/client.rs` |
| 3 · P1 | 不安全的文件凭据副本 | 新凭据只写系统凭据库；移除生产代码的文件加密写入和内存降级。旧格式仅供迁移，全部写入并回读验证后才删除原文件。 | `redash-core/src/config/credentials.rs` |
| 4 · P1 | 编辑文件截断 | 编辑使用完整读取，读到上限加 1 字节以检测超限；2 MiB 限制和 UTF-8 校验通过才进入可写编辑器。预览仍为独立只读路径；统一 CRLF/LF 得以保留。 | `redash-core/src/sftp/mod.rs`、`redash-app/src/views/sftp_view.rs` |
| 5 · P1 | 保存失败删除原文件 | 删除 unlink 后重试逻辑；应用保存使用服务器明确支持的 `posix-rename@openssh.com`。替换失败保留原件与完整临时副本，并提供恢复路径。 | `redash-core/src/sftp/mod.rs`、`session/manager.rs` |
| 6 · P1 | 文件权限、属主丢失 | 仅替换普通文件；临时文件先以 0600 创建，恢复属主、属组、权限并校验后再替换；符号链接和缺少必要元数据的文件拒绝保存。 | `redash-core/src/sftp/mod.rs` |
| 7 · P1 | 同名新建清空已有文件 | 改用 `CREATE | EXCLUDE | WRITE` 排他创建，由服务端原子拒绝重名。 | `redash-core/src/sftp/mod.rs` |
| 8 · P1 | 批量取消后子任务残留 | 使用 `JoinSet` 管理子任务；取消或视图释放时中止作业，执行 guard 请求关闭信道；界面按作业版本忽略旧事件。 | `redash-core/src/batch/mod.rs`、`session/exec.rs`、`redash-app/src/views/batch_view.rs` |
| 9 · P1 | 损坏配置被演示数据覆盖 | 区分不存在、合法空配置及读取/解析错误；错误时保留原文件、显示原因并禁止写入。示例只经显式重置创建。 | `redash-core/src/config/mod.rs`、`redash-app/src/main.rs` |
| 10 · P1 | 配置变更后连接旧目标 | 缓存比对整条连接路由的地址、端口、用户、认证与跳板配置；认证前后校验当前配置。更新注册表并关闭受影响工作台、停止相关批量任务、清理指标历史；新凭据使用新引用。 | `redash-core/src/session/manager.rs`、`config/host.rs`、`redash-app/src/main.rs` |
| 11 · P2 | 异常结束显示成功 | 必须收到有效退出状态；信号退出、exec 请求拒绝、无退出状态的关闭均返回失败或结果未知。 | `redash-core/src/session/exec.rs` |
| 12 · P2 | 超时不关闭远端信道 | 截止时间覆盖连接和执行；超时、取消、异常路径请求 TERM、EOF 与 close，清理自身也有期限。提示明确说明脱离会话的远端后台进程可能继续运行。 | `redash-core/src/session/exec.rs`、`session/manager.rs` |
| 13 · P2 | 跳板环路及整批阻塞 | 获取锁前检查完整路由、环路及最多 8 台主机；连接/认证有期限。监控按主机限时并发采集，结果独立更新。 | `redash-core/src/session/manager.rs`、`redash-app/src/main.rs` |
| 14 · P2 | 普通编辑清空跳板信息 | 编辑保留未由表单修改的跳板和凭据引用；密码/口令留空保留原引用。导入验证全批数据，成功持久化后统一同步 resolver 与监控注册表。 | `redash-app/src/components/host_modal.rs`、`main.rs` |
| 15 · P2 | 凭据持久化失败被隐藏 | 原生凭据错误向上传递；旧文件读取、解密、解析或与现有凭据冲突时保留原件；迁移串行化，不覆盖损坏文件或冲突的原生凭据。 | `redash-core/src/config/credentials.rs` |
| 16 · P2 | HostStore 保存、删除不一致 | 先构造候选状态并持久化，再替换内存，最后清理不再引用的凭据。失败保留当前状态和秘密；界面显示保存、删除、导入等错误。 | `redash-core/src/config/mod.rs`、`redash-app/src/main.rs` |
| 17 · P2 | 终端协议响应丢失 | 接收 Alacritty 的 `PtyWrite`、颜色查询和文本区域查询事件，并经 PTY 回传。 | `redash-app/src/terminal/emulator.rs`、`terminal/view.rs` |
| 18 · P2 | 断开的 PTY 仍显示连接 | 输出流结束即清除 PTY 并展示断开；连接失败可见，提供重新连接当前终端操作。输入发送失败明确反馈。 | `redash-app/src/terminal/view.rs`、`views/workbench.rs` |
| 19 · P2 | load 被误当 CPU 使用率 | Linux 使用 `/proc/stat` 两次计数差；macOS 使用 `top` 两次采样后的 CPU idle。load 独立保留，不再换算为使用率。 | `redash-core/src/probe/linux.rs`、`probe/darwin.rs` |
| 20 · P2 | 指标缺采却显示空值/0 | 接入进程、容器、Linux 监听端口及 SSH ping；按单调时间与计数差计算网络速率。字段有可用标记与采集错误；区分成功空列表与失败，明确 Windows/Unknown 监控不支持。 | `redash-core/src/probe/mod.rs`、`probe/metrics.rs`、`redash-app/src/views/*` |
| 21 · P2 | 设置不生效 | 接入自动刷新、采集间隔/超时、历史长度、字体、光标、滚动历史、选中复制和 SFTP 删除确认；撤下未实现的发光/紧凑布局开关。 | `redash-app/src/main.rs`、`views/settings_view.rs`、`terminal/*`、`views/sftp_view.rs` |
| 22 · P2 | 工作台关闭后隧道残留 | 隧道拥有停止信号、接收任务和转发子任务；后台仅弱引用活动表。停止等待任务退出，释放管理对象时取消接收和子任务。 | `redash-core/src/session/tunnel.rs` |
| 23 · P2 | 隧道统计重入读锁死锁 | 先取活动隧道和跟踪器快照，释放 map 锁后再异步查询，移除嵌套读锁等待。 | `redash-core/src/session/tunnel.rs` |
| 24 · P2 | 伪造心跳 RTT | 使用有截止时间的真实 SSH ping；未收到心跳时展示等待或降级，不生成固定 1 ms 健康值。 | `redash-core/src/session/tunnel.rs` |
| 25 · P2 | 目录响应乱序覆盖 | 初始目录、导航和刷新统一使用请求版本，只提交当前请求的路径与列表；重命名目标从文件本身路径计算。 | `redash-app/src/views/sftp_view.rs` |
| 26 · P2 | HTTP 失败误判通知成功 | curl 启用 HTTP 失败检测并要求 2xx；协议限制与执行期限生效。派发错误记录日志，成功冷却 5 分钟、失败 30 秒后可再尝试。 | `redash-core/src/config/alert.rs`、`redash-app/src/main.rs` |
| 27 · P2 | SFTP 重试不能识别包装错误 | 沿错误链检查 typed cause；连接类读错误重建 SFTP 子系统后重试一次，不因单个 SFTP 错误断开共享 SSH。 | `redash-core/src/sftp/mod.rs`、`session/manager.rs`、`redash-app/src/views/sftp_view.rs` |

表内源码路径均相对于 `crates/`。

## 配套改进

- 命令输出限制为 8 MiB，PTY 输入和输出使用有界队列，避免无限堆积；输入拥塞向用户显示错误。
- 终端应用光标模式、括号粘贴、隐藏光标、滚动历史及 Unicode 搜索列位置修正。Agent 信息仅从当前终端识别，采用最近的显式状态信号；费用/token 汇总不再充当完成信号。Y/N 按钮要求当前可见末行存在对应提示，取消通用“始终允许”按钮。
- 固定 Rust 1.96.0，声明 `rust-version`，统一 rustfmt，并加入 macOS 上的 fmt/test/Clippy CI。CI 配置已加入仓库，本次没有触发远端 GitHub Actions。
- README 收敛未经验证的性能、平台支持、断线恢复及第三方 Webhook 兼容性说明，补充主机信任、旧凭据迁移和安全保存边界。

## 自动验证

本机 macOS，rustc/cargo 1.96.0；依赖离线缓存。最终执行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```

| 检查 | 结果 |
| --- | --- |
| rustfmt | 通过 |
| Clippy，全工作区和所有 target，warnings 作为错误 | 通过 |
| App 单元测试 | 81 通过，0 失败 |
| Core 单元测试 | 68 通过，0 失败 |
| SFTP 集成测试 | 4 通过，0 失败 |
| SSH / 作业 / 隧道集成测试 | 5 通过，0 失败 |
| Doctest | 0 个，无失败 |

新增的正式集成测试位于 [sftp_safety.rs](crates/redash-core/tests/sftp_safety.rs) 和 [ssh_lifecycle.rs](crates/redash-core/tests/ssh_lifecycle.rs)。覆盖完整大文件读取、权限保持、保存失败的恢复副本、排他创建、符号链接拒绝、错误链分类、已知/未知/变化/撤销主机密钥、退出状态、超时/取消的信道关闭、批量子任务回收、隧道监听端口释放及并发状态查询。

单元回归还覆盖配置写入失败保留状态和凭据、损坏配置/凭据保留、迁移冲突、原生存储失败传播、跳板环路、编辑保留路由与凭据引用、CPU 计数与 macOS 采样、Webhook HTTP 500、终端 DSR 响应、终端模式和 Unicode 搜索。凭据测试使用测试内存后端，不读写用户系统凭据。

依赖 `block 0.1.6` 仍有 Cargo future-incompat 提示；它未导致本次 Clippy 或测试失败。升级 GPUI 依赖链时需复查。

## 验证边界与操作变化

- 本次未启动会连接用户已配置主机的 GUI，未执行真实系统凭据迁移，未连接生产 SSH/SFTP 服务。UI 异步状态更新经过代码检查和编译，尚无完整 GUI 自动化回归。
- 未知主机密钥现在会拒绝连接。需要先通过可信渠道核验指纹，再使用系统 `ssh` 写入信任记录；ReDash 不执行无确认的首次信任。
- 远程保存要求 OpenSSH 原子重命名扩展及可恢复的 POSIX 权限、属主、属组；不满足时返回错误。SFTP v3 无法保证 ACL/xattr 保留；保存也未提供多编辑者冲突合并。
- 取消会停止本地任务并请求远端关闭，但不能证明已脱离 SSH 会话的后台进程被终止。
- 系统凭据库失败不会再显示保存成功。旧文件损坏或新旧凭据冲突时保留原件并要求处理，避免静默丢失。
- 仍需真实 OpenSSH/SFTP、系统凭据库、断网场景和桌面交互联调；Linux/Windows 桌面构建与性能基准不在本次验证范围内。

仓库开始时尚无 Git 提交，源码均未跟踪。修复前快照保存在 `/tmp/redash-before-fixes-20260926.tar.gz`；它是本机临时备份，不替代正式版本控制。
