use anyhow::{Context, Result};
use russh::client::{self, Handle};
use russh_sftp::client::SftpSession;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};

use crate::config::{AuthMethod, CredentialVault, HostConfig, HostId};
use crate::session::client::ClientHandler;
use crate::session::exec::{ExecChannel, ExecResult};
use crate::session::pty::PtyChannel;

pub type HostResolver = Arc<dyn Fn(&HostId) -> Option<HostConfig> + Send + Sync>;

struct CachedSession {
    route: Vec<HostConfig>,
    handle: Arc<Handle<ClientHandler>>,
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<HostId, CachedSession>>>,
    connect_locks: Arc<tokio::sync::Mutex<HashMap<HostId, Arc<tokio::sync::Mutex<()>>>>>,
    host_resolver: Arc<std::sync::RwLock<Option<HostResolver>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            connect_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            host_resolver: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub fn with_host_resolver(self, resolver: HostResolver) -> Self {
        *self.host_resolver.write().unwrap() = Some(resolver);
        self
    }

    pub fn set_host_resolver(&self, resolver: HostResolver) {
        *self.host_resolver.write().unwrap() = Some(resolver);
    }

    fn connection_route(&self, host: &HostConfig) -> Result<Vec<HostConfig>> {
        let resolver = self.host_resolver.read().unwrap().clone();
        let mut route = Vec::new();
        let mut seen = HashSet::new();
        let mut current = host.clone();
        loop {
            current.validate()?;
            anyhow::ensure!(
                seen.insert(current.id.clone()),
                "ProxyJump cycle detected at {}",
                current.name
            );
            anyhow::ensure!(route.len() < 8, "ProxyJump supports at most 8 hosts");
            let proxy_id = current.proxy_id().cloned();
            route.push(current);
            let Some(proxy_id) = proxy_id else {
                break;
            };
            let resolver = resolver
                .as_ref()
                .context("未配置 HostResolver，无法解析跳板机")?;
            current =
                resolver(&proxy_id).with_context(|| format!("跳板机 ID '{proxy_id}' 未找到"))?;
        }
        Ok(route)
    }

    pub async fn get_or_connect(&self, host: &HostConfig) -> Result<Arc<Handle<ClientHandler>>> {
        // Validate the whole route before taking any per-host locks.
        if let Some(resolver) = self.host_resolver.read().unwrap().clone() {
            let current = resolver(&host.id).context("Host configuration was removed")?;
            anyhow::ensure!(
                current.same_connection(host),
                "Host connection configuration changed; reopen this operation"
            );
        }
        let route = self.connection_route(host)?;
        tokio::time::timeout(Duration::from_secs(30), self.connect_route(&route))
            .await
            .context("SSH connection and authentication timed out (30s)")?
    }

    async fn connect_route(&self, route: &[HostConfig]) -> Result<Arc<Handle<ClientHandler>>> {
        let host = &route[0];
        let host_lock = {
            let mut locks = self.connect_locks.lock().await;
            Arc::clone(
                locks
                    .entry(host.id.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        let _guard = host_lock.lock().await;
        {
            let map = self.sessions.read().await;
            if let Some(cached) = map.get(&host.id)
                && !cached.handle.is_closed()
                && cached.route.len() == route.len()
                && cached
                    .route
                    .iter()
                    .zip(route)
                    .all(|(old, new)| old.same_connection(new))
            {
                return Ok(Arc::clone(&cached.handle));
            }
        }
        self.disconnect(&host.id).await;
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(25)),
            keepalive_max: 3,
            ..Default::default()
        });
        let handler = ClientHandler::new(&host.hostname, host.port)?;
        let mut handle = if route.len() > 1 {
            let proxy = Box::pin(self.connect_route(&route[1..])).await?;
            let channel = proxy
                .channel_open_direct_tcpip(&host.hostname, u32::from(host.port), "127.0.0.1", 0)
                .await?;
            client::connect_stream(config, channel.into_stream(), handler).await?
        } else {
            client::connect(
                config,
                (host.hostname.trim_matches(['[', ']']), host.port),
                handler,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to SSH server {}:{}",
                    host.hostname, host.port
                )
            })?
        };
        Self::authenticate_handle(&mut handle, host).await?;
        if let Some(resolver) = self.host_resolver.read().unwrap().clone() {
            anyhow::ensure!(
                route
                    .iter()
                    .all(|old| resolver(&old.id)
                        .is_some_and(|current| old.same_connection(&current))),
                "Connection configuration changed during authentication"
            );
        }
        let handle = Arc::new(handle);
        self.sessions.write().await.insert(
            host.id.clone(),
            CachedSession {
                route: route.to_vec(),
                handle: Arc::clone(&handle),
            },
        );
        Ok(handle)
    }

    async fn authenticate_handle(
        handle: &mut Handle<ClientHandler>,
        host: &HostConfig,
    ) -> Result<()> {
        Self::authenticate_handle_with_override(handle, host, None, None).await
    }

    async fn authenticate_handle_with_override(
        handle: &mut Handle<ClientHandler>,
        host: &HostConfig,
        password_override: Option<&str>,
        passphrase_override: Option<&str>,
    ) -> Result<()> {
        let authenticated = tokio::time::timeout(Duration::from_secs(15), async {
            let authed: Result<bool> = async {
                match &host.auth {
                    AuthMethod::Password { credential_id } => {
                        let password = if let Some(pwd) = password_override {
                            pwd.to_string()
                        } else {
                            CredentialVault::get_secret(credential_id).with_context(|| {
                                format!(
                                    "Failed to get password for credential id: {}",
                                    credential_id
                                )
                            })?
                        };
                        let auth_res = handle
                            .authenticate_password(&host.user, password)
                            .await
                            .context("SSH password authentication failed")?;
                        Ok(auth_res.success())
                    }
                    AuthMethod::PrivateKey {
                        key_path,
                        passphrase_id,
                    } => {
                        let passphrase = if let Some(pp) = passphrase_override {
                            if pp.is_empty() {
                                None
                            } else {
                                Some(pp.to_string())
                            }
                        } else if let Some(pid) = passphrase_id {
                            Some(CredentialVault::get_secret(pid)?)
                        } else {
                            None
                        };

                        let key = russh::keys::load_secret_key(key_path, passphrase.as_deref())
                            .with_context(|| {
                                format!("Failed to load private key at {:?}", key_path)
                            })?;
                        let key_with_alg =
                            russh::keys::key::PrivateKeyWithHashAlg::new(Arc::new(key), None);

                        let auth_res = handle
                            .authenticate_publickey(&host.user, key_with_alg)
                            .await
                            .context("SSH public key authentication failed")?;
                        Ok(auth_res.success())
                    }
                    AuthMethod::Agent => {
                        let mut authed = false;
                        #[cfg(unix)]
                        if let Ok(sock_path) = std::env::var("SSH_AUTH_SOCK")
                            && let Ok(stream) = tokio::net::UnixStream::connect(&sock_path).await
                        {
                            let mut agent =
                                russh::keys::agent::client::AgentClient::connect(stream);
                            if let Ok(identities) = agent.request_identities().await {
                                for id in identities {
                                    let res = match id {
                                        russh::keys::agent::AgentIdentity::PublicKey {
                                            key,
                                            ..
                                        } => {
                                            handle
                                                .authenticate_publickey_with(
                                                    &host.user, key, None, &mut agent,
                                                )
                                                .await
                                        }
                                        russh::keys::agent::AgentIdentity::Certificate {
                                            certificate,
                                            ..
                                        } => {
                                            handle
                                                .authenticate_certificate_with(
                                                    &host.user,
                                                    certificate,
                                                    None,
                                                    &mut agent,
                                                )
                                                .await
                                        }
                                    };
                                    if let Ok(auth_res) = res
                                        && auth_res.success()
                                    {
                                        authed = true;
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(authed)
                    }
                }
            }
            .await;
            authed
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "认证超时 (15s): 主机 {} 认证未能在规定时间内完成",
                host.name
            )
        })??;

        if !authenticated {
            return Err(anyhow::anyhow!(
                "SSH authentication rejected for user {}",
                host.user
            ));
        }

        Ok(())
    }

    /// Test connection and authentication to a host with optional in-memory password/passphrase overrides.
    ///
    /// Useful for verifying connectivity and credentials when creating or editing a host before saving.
    /// Returns the connection latency if successful.
    pub async fn test_connection_with_credentials(
        &self,
        host: &HostConfig,
        password_override: Option<&str>,
        passphrase_override: Option<&str>,
    ) -> Result<Duration> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(15);
        tokio::time::timeout(timeout, async {
            let route = self.connection_route(host)?;
            let config = Arc::new(client::Config {
                inactivity_timeout: None,
                keepalive_interval: Some(Duration::from_secs(10)),
                keepalive_max: 2,
                ..Default::default()
            });
            let handler = ClientHandler::new(&host.hostname, host.port)?;
            let mut handle = if route.len() > 1 {
                let proxy = Box::pin(self.connect_route(&route[1..])).await?;
                let channel = proxy
                    .channel_open_direct_tcpip(&host.hostname, u32::from(host.port), "127.0.0.1", 0)
                    .await?;
                client::connect_stream(config, channel.into_stream(), handler).await?
            } else {
                client::connect(
                    config,
                    (host.hostname.trim_matches(['[', ']']), host.port),
                    handler,
                )
                .await
                .with_context(|| {
                    format!(
                        "无法连接至目标 SSH 地址 {}:{}",
                        host.hostname, host.port
                    )
                })?
            };

            Self::authenticate_handle_with_override(
                &mut handle,
                host,
                password_override,
                passphrase_override,
            )
            .await?;

            // Open an exec session channel to ensure command capability
            let channel = handle
                .channel_open_session()
                .await
                .context("SSH 会话通道打开失败")?;
            channel
                .exec(true, "true")
                .await
                .context("SSH 验证指令执行失败")?;

            Ok(start.elapsed())
        })
        .await
        .map_err(|_| anyhow::anyhow!("SSH 连接或认证超时 (15s)"))?
    }

    /// Convenience method to test an already saved host's connection.
    pub async fn test_connection(&self, host: &HostConfig) -> Result<Duration> {
        self.test_connection_with_credentials(host, None, None).await
    }

    pub async fn open_pty(
        &self,
        host: &HostConfig,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<PtyChannel> {
        let handle = self.get_or_connect(host).await?;
        PtyChannel::new(&handle, cols, rows, output_tx).await
    }

    pub async fn exec(
        &self,
        host: &HostConfig,
        command: &str,
        timeout_dur: Duration,
    ) -> Result<ExecResult> {
        let deadline = tokio::time::Instant::now() + timeout_dur;
        let handle = tokio::time::timeout_at(deadline, self.get_or_connect(host))
            .await
            .context("Command deadline exceeded while connecting")??;
        ExecChannel::execute(
            &handle,
            command,
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )
        .await
    }

    /// A short-lived channel for OpenSSH's atomic replacement extension.
    /// The high-level russh-sftp session does not expose arbitrary extensions.
    pub async fn open_sftp_atomic(
        &self,
        host: &HostConfig,
    ) -> Result<russh_sftp::client::RawSftpSession> {
        tokio::time::timeout(Duration::from_secs(30), async {
            let handle = self.get_or_connect(host).await?;
            let channel = handle.channel_open_session().await?;
            channel.request_subsystem(true, "sftp").await?;
            let raw = russh_sftp::client::RawSftpSession::new(channel.into_stream());
            let version = raw.init().await?;
            if version.extensions.get("posix-rename@openssh.com").is_none_or(|value| value != "1") {
                let _ = raw.close_session();
                anyhow::bail!("服务器不支持原子文件替换扩展；已保留原文件，请使用支持 posix-rename 的 SFTP 服务器");
            }
            Ok(raw)
        }).await.context("Opening atomic SFTP session timed out")?
    }

    pub async fn open_sftp(&self, host: &HostConfig) -> Result<SftpSession> {
        let handle = self.get_or_connect(host).await?;
        let open_res = async {
            let channel =
                tokio::time::timeout(Duration::from_secs(10), handle.channel_open_session())
                    .await
                    .map_err(|_| anyhow::anyhow!("打开 SFTP 会话通道超时 (10s)"))?
                    .context("Failed to open channel for SFTP")?;

            tokio::time::timeout(
                Duration::from_secs(10),
                channel.request_subsystem(true, "sftp"),
            )
            .await
            .map_err(|_| anyhow::anyhow!("请求 SFTP 子系统超时 (10s)"))?
            .context("Failed to request SFTP subsystem")?;

            let sftp = tokio::time::timeout(
                Duration::from_secs(10),
                SftpSession::new(channel.into_stream()),
            )
            .await
            .map_err(|_| anyhow::anyhow!("初始化 SFTP 会话流超时 (10s)"))?
            .context("Failed to initialize SFTP session")?;

            Result::<SftpSession>::Ok(sftp)
        }
        .await;

        // A subsystem failure must not tear down terminals sharing this SSH connection.
        open_res
    }

    /// Checks if a session is actively connected and valid.
    pub async fn is_connected(&self, host_id: &HostId) -> bool {
        let map = self.sessions.read().await;
        if let Some(handle) = map.get(host_id) {
            !handle.handle.is_closed()
        } else {
            false
        }
    }

    /// Measures the live round-trip latency to the SSH server using a keepalive ping.
    /// If the connection is severed, automatically disconnects the stale handle.
    pub async fn ping_host(&self, host: &HostConfig) -> Result<Duration> {
        let handle = self.get_or_connect(host).await?;
        let start = std::time::Instant::now();
        let ping_res = tokio::time::timeout(Duration::from_secs(5), handle.send_ping()).await;

        match ping_res {
            Ok(Ok(())) => Ok(start.elapsed()),
            Ok(Err(e)) => {
                log::warn!("SSH ping failed for {}: {}, disconnecting...", host.name, e);
                self.disconnect(&host.id).await;
                Err(anyhow::anyhow!("SSH ping failed: {}", e))
            }
            Err(_) => {
                log::warn!(
                    "SSH ping timed out (5s) for {}, disconnecting...",
                    host.name
                );
                self.disconnect(&host.id).await;
                Err(anyhow::anyhow!("SSH ping timed out"))
            }
        }
    }

    pub async fn disconnect(&self, host_id: &HostId) {
        let removed = self.sessions.write().await.remove(host_id);
        if let Some(cached) = removed {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                cached.handle.disconnect(
                    russh::Disconnect::ByApplication,
                    "Configuration changed or disconnected",
                    "",
                ),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_manager_initial_state() {
        let mgr = SessionManager::new();
        let host_id = HostId("non_existent_host".to_string());
        assert!(!mgr.is_connected(&host_id).await);
    }

    #[tokio::test]
    async fn test_disconnect_non_existent() {
        let mgr = SessionManager::new();
        let host_id = HostId("non_existent_host".to_string());
        // Should not panic
        mgr.disconnect(&host_id).await;
    }

    #[tokio::test]
    async fn test_proxy_resolver_integration() {
        let proxy_id = HostId("jump-1".to_string());
        let mut proxy = HostConfig::new("jump-1", "127.0.0.1", "root");
        proxy.id = proxy_id.clone();
        proxy.port = 1;

        let mut target = HostConfig::new("internal-db", "10.0.0.5", "dbuser");
        target.proxy_jump_id = Some(proxy_id.clone());

        // Test 1: No resolver configured
        let mgr_no_res = SessionManager::new();
        let err1 = match mgr_no_res.get_or_connect(&target).await {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unconfigured resolver"),
        };
        assert!(err1.to_string().contains("未配置 HostResolver"));

        // Test 2: Resolver configured, returns proxy
        let proxy_clone = proxy.clone();
        let target_clone = target.clone();
        let mgr = SessionManager::new().with_host_resolver(Arc::new(move |id| {
            if id == &proxy_id {
                Some(proxy_clone.clone())
            } else if id == &target_clone.id {
                Some(target_clone.clone())
            } else {
                None
            }
        }));

        let err2 = match mgr.get_or_connect(&target).await {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unreachable proxy"),
        };
        // Failed because proxy at 127.0.0.1:1 is unreachable, showing proxy resolution succeeded and triggered connection
        assert!(
            err2.to_string().contains("连接超时") || err2.to_string().contains("Failed to connect")
        );
    }
    #[tokio::test]
    async fn proxy_cycles_fail_before_connecting() {
        let mut a = HostConfig::new("a", "localhost", "test");
        let mut b = HostConfig::new("b", "localhost", "test");
        a.proxy_jump_id = Some(b.id.clone());
        b.proxy_jump_id = Some(a.id.clone());
        let all = [a.clone(), b];
        let manager = SessionManager::new().with_host_resolver(Arc::new(move |id| {
            all.iter().find(|h| &h.id == id).cloned()
        }));
        let result = tokio::time::timeout(Duration::from_millis(100), manager.get_or_connect(&a))
            .await
            .unwrap();
        assert!(result.err().unwrap().to_string().contains("cycle"));
    }
}
