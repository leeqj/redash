use anyhow::{Context, Result};
use russh::client::Handle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, oneshot};

use crate::session::client::ClientHandler;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum TunnelType {
    Local {
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    DynamicSocks5 {
        local_port: u16,
    },
}

impl TunnelType {
    pub fn local_port(&self) -> u16 {
        match self {
            TunnelType::Local { local_port, .. } => *local_port,
            TunnelType::DynamicSocks5 { local_port } => *local_port,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub tunnel_type: TunnelType,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum TunnelHealthState {
    Healthy { latency_ms: u32 },
    Degraded { reason: String },
    Unhealthy { error: String },
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelStats {
    pub tunnel_id: String,
    pub name: String,
    pub active_connections: usize,
    pub total_connections: u64,
    pub bytes_transferred: u64,
    pub last_heartbeat: Option<u64>,
    pub health: TunnelHealthState,
}

#[derive(Debug, Default)]
pub struct TunnelHealthTracker {
    pub active_connections: AtomicUsize,
    pub total_connections: AtomicU64,
    pub bytes_transferred: AtomicU64,
    pub last_heartbeat: AtomicU64,
    pub last_latency_ms: AtomicU32,
    pub is_unhealthy: AtomicBool,
    pub failure_reason: tokio::sync::RwLock<Option<String>>,
}

pub struct ActiveTunnel {
    pub config: TunnelConfig,
    stop_tx: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    pub tracker: Arc<TunnelHealthTracker>,
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        if let Some(stop) = self.stop_tx.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
struct ConnectionCount(Arc<TunnelHealthTracker>);
impl Drop for ConnectionCount {
    fn drop(&mut self) {
        self.0.active_connections.fetch_sub(1, Ordering::SeqCst);
    }
}

pub type ActiveTunnelsMap = Arc<RwLock<HashMap<String, ActiveTunnel>>>;

pub struct TunnelManager {
    active_tunnels: ActiveTunnelsMap,
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            active_tunnels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn is_active(&self, tunnel_id: &str) -> bool {
        let map = self.active_tunnels.read().await;
        map.contains_key(tunnel_id)
    }

    pub async fn list_active(&self) -> Vec<TunnelConfig> {
        let map = self.active_tunnels.read().await;
        map.values().map(|entry| entry.config.clone()).collect()
    }

    pub async fn get_tunnel_stats(&self, tunnel_id: &str) -> Option<TunnelStats> {
        let (config, tracker) = {
            let map = self.active_tunnels.read().await;
            let entry = map.get(tunnel_id)?;
            (entry.config.clone(), Arc::clone(&entry.tracker))
        };
        let active_conns = tracker.active_connections.load(Ordering::SeqCst);
        let total_conns = tracker.total_connections.load(Ordering::SeqCst);
        let bytes = tracker.bytes_transferred.load(Ordering::SeqCst);
        let hb = tracker.last_heartbeat.load(Ordering::SeqCst);
        let last_hb = if hb > 0 { Some(hb) } else { None };
        let lat = tracker.last_latency_ms.load(Ordering::SeqCst);

        let health = if tracker.is_unhealthy.load(Ordering::SeqCst) {
            let reason = tracker
                .failure_reason
                .read()
                .await
                .clone()
                .unwrap_or_else(|| "网络通道异常".to_string());
            TunnelHealthState::Unhealthy { error: reason }
        } else if hb == 0 {
            TunnelHealthState::Degraded {
                reason: "等待首次 SSH 心跳响应".into(),
            }
        } else if lat > 800 {
            TunnelHealthState::Degraded {
                reason: format!("心跳高延迟 ({}ms)", lat),
            }
        } else {
            TunnelHealthState::Healthy { latency_ms: lat }
        };

        Some(TunnelStats {
            tunnel_id: config.id.clone(),
            name: config.name.clone(),
            active_connections: active_conns,
            total_connections: total_conns,
            bytes_transferred: bytes,
            last_heartbeat: last_hb,
            health,
        })
    }

    pub async fn list_tunnel_stats(&self) -> Vec<TunnelStats> {
        let ids: Vec<_> = self.active_tunnels.read().await.keys().cloned().collect();
        let mut stats = Vec::new();
        for id in ids {
            if let Some(st) = self.get_tunnel_stats(&id).await {
                stats.push(st);
            }
        }
        stats
    }

    pub async fn check_health(&self, tunnel_id: &str) -> TunnelHealthState {
        if let Some(st) = self.get_tunnel_stats(tunnel_id).await {
            st.health
        } else {
            TunnelHealthState::Stopped
        }
    }

    pub async fn start_tunnel(
        &self,
        mut config: TunnelConfig,
        handle: Arc<Handle<ClientHandler>>,
    ) -> Result<()> {
        let mut map = self.active_tunnels.write().await;
        if map.contains_key(&config.id) {
            anyhow::bail!("Tunnel {} is already running", config.id);
        }

        let local_port = config.tunnel_type.local_port();
        let listener = TcpListener::bind(("127.0.0.1", local_port))
            .await
            .with_context(|| format!("Failed to bind to 127.0.0.1:{}", local_port))?;

        let bound_port = listener.local_addr()?.port();
        match &mut config.tunnel_type {
            TunnelType::Local { local_port, .. } | TunnelType::DynamicSocks5 { local_port } => {
                *local_port = bound_port
            }
        }
        let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
        let tracker = Arc::new(TunnelHealthTracker::default());
        let task_tracker = Arc::clone(&tracker);
        let tunnel_type = config.tunnel_type.clone();
        let weak_map = Arc::downgrade(&self.active_tunnels);
        let tunnel_id = config.id.clone();
        let task = tokio::spawn(async move {
            // This task owns every forwarding task and its heartbeat. No task owns the map.
            let mut children = tokio::task::JoinSet::new();
            let ping_tracker = Arc::clone(&task_tracker);
            let ping_handle = Arc::clone(&handle);
            children.spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let start = Instant::now();
                    match tokio::time::timeout(Duration::from_secs(3), ping_handle.send_ping())
                        .await
                    {
                        Ok(Ok(())) => {
                            ping_tracker.last_latency_ms.store(
                                start.elapsed().as_millis().min(u32::MAX as u128) as u32,
                                Ordering::SeqCst,
                            );
                            ping_tracker.last_heartbeat.store(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                Ordering::SeqCst,
                            );
                            ping_tracker.is_unhealthy.store(false, Ordering::SeqCst);
                            *ping_tracker.failure_reason.write().await = None;
                        }
                        _ => {
                            ping_tracker.is_unhealthy.store(true, Ordering::SeqCst);
                            *ping_tracker.failure_reason.write().await =
                                Some("SSH 心跳失败或超时".into());
                        }
                    }
                }
            });
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    _ = children.join_next(), if !children.is_empty() => {},
                    accepted = listener.accept() => {
                        let Ok((socket, peer)) = accepted else { break; };
                        let handle = Arc::clone(&handle);
                        let kind = tunnel_type.clone();
                        let tracker = Arc::clone(&task_tracker);
                        tracker.active_connections.fetch_add(1, Ordering::SeqCst);
                        tracker.total_connections.fetch_add(1, Ordering::SeqCst);
                        children.spawn(async move {
                            let _count = ConnectionCount(Arc::clone(&tracker));
                            if let Err(error) = Self::handle_connection(socket, peer, kind, handle, tracker).await {
                                log::debug!("Tunnel connection ended: {error:#}");
                            }
                        });
                    }
                }
            }
            drop(listener);
            children.shutdown().await;
            if let Some(map) = weak_map.upgrade() {
                let mut map = map.write().await;
                if map
                    .get(&tunnel_id)
                    .is_some_and(|entry| Arc::ptr_eq(&entry.tracker, &task_tracker))
                {
                    map.remove(&tunnel_id);
                }
            }
        });
        config.active = true;
        map.insert(
            config.id.clone(),
            ActiveTunnel {
                config,
                stop_tx: Some(stop_tx),
                task: Some(task),
                tracker,
            },
        );
        Ok(())
    }

    async fn handle_connection(
        mut socket: TcpStream,
        peer_addr: std::net::SocketAddr,
        tunnel_type: TunnelType,
        handle: Arc<Handle<ClientHandler>>,
        tracker: Arc<TunnelHealthTracker>,
    ) -> Result<()> {
        match tunnel_type {
            TunnelType::Local {
                remote_host,
                remote_port,
                ..
            } => {
                let channel = handle
                    .channel_open_direct_tcpip(
                        remote_host,
                        remote_port as u32,
                        peer_addr.ip().to_string(),
                        peer_addr.port() as u32,
                    )
                    .await
                    .context("Failed to open direct-tcpip channel")?;

                let mut channel_stream = channel.into_stream();
                let copy_res =
                    tokio::io::copy_bidirectional(&mut socket, &mut channel_stream).await;
                if let Ok((from_client, from_server)) = copy_res {
                    tracker
                        .bytes_transferred
                        .fetch_add(from_client + from_server, Ordering::SeqCst);
                }
            }
            TunnelType::DynamicSocks5 { .. } => {
                let (dest_host, dest_port) = match Self::socks5_handshake(&mut socket).await {
                    Ok(target) => target,
                    Err(e) => {
                        let _ = socket
                            .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                        return Err(e);
                    }
                };

                let channel_res = handle
                    .channel_open_direct_tcpip(
                        dest_host,
                        dest_port as u32,
                        peer_addr.ip().to_string(),
                        peer_addr.port() as u32,
                    )
                    .await;

                match channel_res {
                    Ok(channel) => {
                        let success_reply = [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
                        socket.write_all(&success_reply).await?;
                        let mut channel_stream = channel.into_stream();
                        let copy_res =
                            tokio::io::copy_bidirectional(&mut socket, &mut channel_stream).await;
                        if let Ok((from_client, from_server)) = copy_res {
                            tracker
                                .bytes_transferred
                                .fetch_add(from_client + from_server, Ordering::SeqCst);
                        }
                    }
                    Err(e) => {
                        let _ = socket
                            .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                        return Err(e.into());
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn socks5_handshake(socket: &mut TcpStream) -> Result<(String, u16)> {
        let mut header = [0u8; 2];
        socket.read_exact(&mut header).await?;
        if header[0] != 0x05 {
            anyhow::bail!("Unsupported SOCKS version: {}", header[0]);
        }
        let nmethods = header[1] as usize;
        let mut methods = vec![0u8; nmethods];
        socket.read_exact(&mut methods).await?;

        if !methods.contains(&0x00) {
            socket.write_all(&[0x05, 0xff]).await?;
            anyhow::bail!("SOCKS client does not support no-authentication mode");
        }
        // Respond with NO AUTHENTICATION REQUIRED (0x00)
        socket.write_all(&[0x05, 0x00]).await?;

        // Read request
        let mut req_header = [0u8; 4];
        socket.read_exact(&mut req_header).await?;
        if req_header[0] != 0x05 || req_header[1] != 0x01 {
            anyhow::bail!("Only SOCKS5 CONNECT (0x01) command is supported");
        }

        let atyp = req_header[3];
        let dest_host = match atyp {
            0x01 => {
                // IPv4
                let mut ip = [0u8; 4];
                socket.read_exact(&mut ip).await?;
                format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
            }
            0x03 => {
                // Domain name
                let mut len_buf = [0u8; 1];
                socket.read_exact(&mut len_buf).await?;
                let len = len_buf[0] as usize;
                let mut domain = vec![0u8; len];
                socket.read_exact(&mut domain).await?;
                String::from_utf8(domain)?
            }
            0x04 => {
                // IPv6
                let mut ip = [0u8; 16];
                socket.read_exact(&mut ip).await?;
                std::net::Ipv6Addr::from(ip).to_string()
            }
            _ => anyhow::bail!("Unsupported SOCKS5 address type: {}", atyp),
        };

        let mut port_buf = [0u8; 2];
        socket.read_exact(&mut port_buf).await?;
        let dest_port = u16::from_be_bytes(port_buf);

        Ok((dest_host, dest_port))
    }

    pub async fn stop_tunnel(&self, tunnel_id: &str) -> Result<()> {
        let entry = self.active_tunnels.write().await.remove(tunnel_id);
        let mut entry = entry.with_context(|| format!("Tunnel {tunnel_id} is not active"))?;
        if let Some(stop) = entry.stop_tx.take() {
            let _ = stop.send(());
        }
        if let Some(mut task) = entry.task.take()
            && tokio::time::timeout(Duration::from_secs(2), &mut task)
                .await
                .is_err()
        {
            task.abort();
            let _ = task.await;
        }
        Ok(())
    }
    pub async fn stop_all(&self) {
        let ids: Vec<_> = self.active_tunnels.read().await.keys().cloned().collect();
        for id in ids {
            let _ = self.stop_tunnel(&id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tunnel_config_serde() {
        let local_cfg = TunnelConfig {
            id: "tun-1".into(),
            name: "Postgres Forward".into(),
            tunnel_type: TunnelType::Local {
                local_port: 15432,
                remote_host: "127.0.0.1".into(),
                remote_port: 5432,
            },
            active: false,
        };

        let json = serde_json::to_string(&local_cfg).expect("serialize");
        let deserialized: TunnelConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(local_cfg, deserialized);
        assert_eq!(deserialized.tunnel_type.local_port(), 15432);

        let socks_cfg = TunnelConfig {
            id: "tun-2".into(),
            name: "SOCKS Proxy".into(),
            tunnel_type: TunnelType::DynamicSocks5 { local_port: 10808 },
            active: true,
        };

        let json = serde_json::to_string(&socks_cfg).expect("serialize");
        let deserialized: TunnelConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(socks_cfg, deserialized);
        assert_eq!(deserialized.tunnel_type.local_port(), 10808);
    }

    #[test]
    fn test_tunnel_stats_and_health_states() {
        let tracker = TunnelHealthTracker::default();
        tracker.active_connections.store(3, Ordering::SeqCst);
        tracker.total_connections.store(15, Ordering::SeqCst);
        tracker
            .bytes_transferred
            .store(1024 * 1024, Ordering::SeqCst);
        tracker.last_latency_ms.store(12, Ordering::SeqCst);

        let stats = TunnelStats {
            tunnel_id: "test_tun".into(),
            name: "Test SOCKS".into(),
            active_connections: tracker.active_connections.load(Ordering::SeqCst),
            total_connections: tracker.total_connections.load(Ordering::SeqCst),
            bytes_transferred: tracker.bytes_transferred.load(Ordering::SeqCst),
            last_heartbeat: Some(1727190000),
            health: TunnelHealthState::Healthy { latency_ms: 12 },
        };

        assert_eq!(stats.active_connections, 3);
        assert_eq!(stats.total_connections, 15);
        assert_eq!(stats.bytes_transferred, 1048576);
        assert_eq!(stats.health, TunnelHealthState::Healthy { latency_ms: 12 });
    }

    #[tokio::test]
    async fn test_socks5_handshake_ipv4_and_domain() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Skipping socks5 mock test due to sandbox restriction: {}", e);
                return;
            }
            Err(e) => panic!("failed to bind mock server: {}", e),
        };
        let port = listener.local_addr().unwrap().port();

        // Test IPv4 CONNECT
        let client_task = tokio::spawn(async move {
            let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

            let mut reply = [0u8; 2];
            client.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply, [0x05, 0x00]);

            let req = [0x05, 0x01, 0x00, 0x01, 1, 2, 3, 4, 0x00, 0x50];
            client.write_all(&req).await.unwrap();
        });

        let (mut server_stream, _) = listener.accept().await.unwrap();
        let (dest_host, dest_port) = TunnelManager::socks5_handshake(&mut server_stream)
            .await
            .unwrap();

        assert_eq!(dest_host, "1.2.3.4");
        assert_eq!(dest_port, 80);

        client_task.await.unwrap();

        // Test Domain CONNECT
        let client_task = tokio::spawn(async move {
            let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut reply = [0u8; 2];
            client.read_exact(&mut reply).await.unwrap();

            let mut req = vec![0x05, 0x01, 0x00, 0x03, 11];
            req.extend_from_slice(b"example.com");
            req.extend_from_slice(&443u16.to_be_bytes());
            client.write_all(&req).await.unwrap();
        });

        let (mut server_stream, _) = listener.accept().await.unwrap();
        let (dest_host, dest_port) = TunnelManager::socks5_handshake(&mut server_stream)
            .await
            .unwrap();

        assert_eq!(dest_host, "example.com");
        assert_eq!(dest_port, 443);

        client_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_tunnel_manager_lifecycle() {
        let mgr = TunnelManager::new();
        assert_eq!(mgr.list_active().await.len(), 0);
        assert!(!mgr.is_active("non-existent").await);
        assert_eq!(
            mgr.check_health("non-existent").await,
            TunnelHealthState::Stopped
        );
        assert!(mgr.stop_tunnel("non-existent").await.is_err());
    }
}
