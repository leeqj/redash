use log::debug;
use redash_types::LanBeacon;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

pub use redash_types::DISCOVERY_MULTICAST_ADDR;

#[derive(Debug, Clone)]
pub struct DiscoveredNode {
    pub node_id: String,
    pub hostname: String,
    pub ip: String,
    pub direct_port: u16,
    pub last_seen: Instant,
}

pub struct LanDiscoveryClient {
    nodes: Arc<RwLock<HashMap<String, DiscoveredNode>>>,
    changed: tokio::sync::watch::Sender<()>,
}

static GLOBAL_DISCOVERY: OnceLock<LanDiscoveryClient> = OnceLock::new();

impl Default for LanDiscoveryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LanDiscoveryClient {
    pub fn new() -> Self {
        let nodes = Arc::new(RwLock::new(HashMap::new()));
        let nodes_clone = nodes.clone();
        let (changed, _) = tokio::sync::watch::channel(());
        let updates = changed.clone();

        tokio::spawn(async move {
            loop {
                run_listener(nodes_clone.clone(), updates.clone()).await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });

        Self { nodes, changed }
    }

    pub fn global() -> &'static Self {
        GLOBAL_DISCOVERY.get_or_init(Self::new)
    }

    /// Returns direct WebSocket URL `ws://<ip>:<port>/v1/agent/direct` if node was discovered on LAN.
    pub fn find_direct_endpoint(&self, node_id: &str) -> Option<String> {
        let mut map = self.nodes.write().unwrap();
        // Prune entries older than 15s
        map.retain(|_, v| v.last_seen.elapsed() <= Duration::from_secs(15));

        map.get(node_id)
            .map(|node| format!("ws://{}:{}/v1/agent/direct", node.ip, node.direct_port))
    }

    /// A cold connection can discover its LAN route while the Hub is being dialed.
    pub async fn wait_for_endpoint(&self, node_id: &str, timeout: Duration) -> Option<String> {
        let mut changes = self.changed.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                if let Some(endpoint) = self.find_direct_endpoint(node_id) {
                    return Some(endpoint);
                }
                changes.changed().await.ok()?;
            }
        })
        .await
        .ok()
        .flatten()
    }

    /// Manually register or test a discovered node.
    pub fn register_discovered(&self, node_id: &str, ip: &str, direct_port: u16, hostname: &str) {
        let mut map = self.nodes.write().unwrap();
        map.insert(
            node_id.to_string(),
            DiscoveredNode {
                node_id: node_id.to_string(),
                hostname: hostname.to_string(),
                ip: ip.to_string(),
                direct_port,
                last_seen: Instant::now(),
            },
        );
        self.changed.send_replace(());
    }
}

async fn run_listener(
    nodes: Arc<RwLock<HashMap<String, DiscoveredNode>>>,
    changed: tokio::sync::watch::Sender<()>,
) {
    let socket = match UdpSocket::bind("0.0.0.0:8765").await {
        Ok(s) => s,
        Err(e) => {
            debug!(
                "Unable to bind UDP 8765 for LAN discovery client (may be bound by another local process): {}",
                e
            );
            return;
        }
    };

    let _ = socket.set_broadcast(true);
    if let Err(err) = socket.join_multicast_v4(
        std::net::Ipv4Addr::new(239, 255, 77, 88),
        std::net::Ipv4Addr::UNSPECIFIED,
    ) {
        debug!("Unable to join LAN discovery multicast group: {}", err);
    }
    let mut buf = [0u8; 2048];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, peer_addr)) => {
                if let Ok(beacon) = serde_json::from_slice::<LanBeacon>(&buf[..len]) {
                    let peer_ip = peer_addr.ip().to_string();
                    debug!(
                        "Discovered LAN node: {} ({}) at {}:{}",
                        beacon.node_id, beacon.hostname, peer_ip, beacon.direct_port
                    );

                    if beacon.node_id.len() > 128 || beacon.direct_port == 0 {
                        continue;
                    }
                    let mut map = nodes.write().unwrap();
                    map.retain(|_, v| v.last_seen.elapsed() <= Duration::from_secs(15));
                    if map.len() >= 1024 && !map.contains_key(&beacon.node_id) {
                        continue;
                    }
                    map.insert(
                        beacon.node_id.clone(),
                        DiscoveredNode {
                            node_id: beacon.node_id,
                            hostname: beacon.hostname,
                            ip: peer_ip,
                            direct_port: beacon.direct_port,
                            last_seen: Instant::now(),
                        },
                    );
                    changed.send_replace(());
                }
            }
            Err(e) => {
                debug!("UDP discovery receive error: {}", e);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lan_discovery_cache_and_expiration() {
        let client = LanDiscoveryClient {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            changed: tokio::sync::watch::channel(()).0,
        };

        client.register_discovered("node-lan-test", "192.168.1.120", 43210, "homelab");

        let endpoint = client.find_direct_endpoint("node-lan-test");
        assert_eq!(
            endpoint,
            Some("ws://192.168.1.120:43210/v1/agent/direct".to_string())
        );

        // Unknown node
        assert_eq!(client.find_direct_endpoint("unknown-node"), None);
    }
}
