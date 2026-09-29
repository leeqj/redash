use futures::{SinkExt, StreamExt};
use log::{debug, info, warn};
use redash_types::LanBeacon;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::tty::TtyManager;

pub use redash_types::{DISCOVERY_BROADCAST_ADDR, DISCOVERY_MULTICAST_ADDR};

pub struct LanDiscoveryAgent {
    pub node_id: String,
    pub hostname: String,
    pub version: String,
    pub direct_port: u16,
    pub trusted_public_key: Option<String>,
}

impl LanDiscoveryAgent {
    pub async fn start(
        node_id: String,
        hostname: String,
        version: String,
        trusted_public_key: Option<String>,
        identity_private_key: String,
        tty_mgr: Arc<TtyManager>,
    ) -> anyhow::Result<(u16, tokio::task::JoinHandle<()>)> {
        anyhow::ensure!(trusted_public_key.as_deref().is_some_and(|key| !key.is_empty()), "LAN terminal requires a trusted client key");
        let nid_server = node_id.clone();
        // 1. Bind Direct TCP/WebSocket listener on an OS-assigned dynamic port
        let listener = TcpListener::bind("0.0.0.0:0").await?;
        let direct_port = listener.local_addr()?.port();
        info!(
            "⚡ Agent LAN direct listening endpoint online at 0.0.0.0:{}",
            direct_port
        );

        let tty_for_server = tty_mgr.clone();
        let pub_key_for_server = trusted_public_key.clone();

        // 2. Direct connection server task
        tokio::spawn(async move {
            while let Ok((stream, peer_addr)) = listener.accept().await {
                info!("Incoming direct LAN P2P connection from {}", peer_addr);
                let tty = tty_for_server.clone();
                let trusted_pk = pub_key_for_server.clone().unwrap();
                let identity = identity_private_key.clone();
                let nid = nid_server.clone();

                tokio::spawn(async move {
                    if let Err(e) = handle_direct_connection(stream, peer_addr, tty, trusted_pk, identity, nid).await {
                        warn!("Direct connection with {} terminated: {}", peer_addr, e);
                    }
                });
            }
        });

        // 3. UDP Multicast / Broadcast Beacon task
        let nid = node_id.clone();
        let hname = hostname.clone();
        let ver = version.clone();

        let beacon_task = tokio::spawn(async move {
            run_beacon_broadcaster(nid, hname, direct_port, ver).await;
        });

        Ok((direct_port, beacon_task))
    }
}

async fn run_beacon_broadcaster(
    node_id: String,
    hostname: String,
    direct_port: u16,
    version: String,
) {
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to bind UDP socket for LAN beacon: {}", e);
            return;
        }
    };

    let _ = socket.set_broadcast(true);

    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let beacon = LanBeacon {
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            direct_port,
            version: version.clone(),
            timestamp: now,
        };

        if let Ok(bytes) = serde_json::to_vec(&beacon) {
            let _ = socket.send_to(&bytes, DISCOVERY_MULTICAST_ADDR).await;
            let _ = socket.send_to(&bytes, DISCOVERY_BROADCAST_ADDR).await;
            debug!("Sent LAN discovery beacon for node {}", node_id);
        }

        sleep(Duration::from_secs(3)).await;
    }
}

pub async fn handle_direct_connection(
    stream: tokio::net::TcpStream,
    _peer_addr: SocketAddr,
    tty_mgr: Arc<TtyManager>,
    trusted_public_key: String,
    identity_private_key: String,
    node_id: String,
) -> anyhow::Result<()> {
    let mut ws = tokio::time::timeout(Duration::from_secs(5), accept_async(stream)).await??;
    let first = tokio::time::timeout(Duration::from_secs(5), ws.next()).await?
        .ok_or_else(|| anyhow::anyhow!("Missing handshake"))??;
    let Message::Text(text) = first else { anyhow::bail!("Expected authenticated handshake"); };
    let init: redash_types::E2eeHandshakeInit = serde_json::from_str(&text)?;
    let route = format!("lan-{}", uuid::Uuid::new_v4());
    let (tx, mut rx) = mpsc::channel(128);
    let ack = tty_mgr.accept_handshake(&init, &trusted_public_key, &identity_private_key, &node_id, &route, tx).await.map_err(anyhow::Error::msg)?;
    let result = async {
        ws.send(Message::Text(serde_json::to_string(&ack)?.into())).await?;
        loop {
            tokio::select! {
                outgoing = rx.recv() => {
                    let Some(redash_types::AgentToHubMessage::TtyEncrypted(env)) = outgoing else { break; };
                    ws.send(Message::Text(serde_json::to_string(&env)?.into())).await?;
                }
                incoming = ws.next() => {
                    match incoming {
                        Some(Ok(Message::Text(text))) => {
                            let env: redash_types::EncryptedEnvelope = serde_json::from_str(&text)?;
                            anyhow::ensure!(env.session_id == init.session_id, "Wrong session");
                            tty_mgr.receive(&env, &route).await.map_err(anyhow::Error::msg)?;
                        }
                        Some(Ok(Message::Ping(data))) => { ws.send(Message::Pong(data)).await?; }
                        _ => break,
                    }
                }
            }
        }
        Ok(())
    }.await;
    tty_mgr.close_route(&route).await;
    result
}
