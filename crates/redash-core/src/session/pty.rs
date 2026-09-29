use crate::session::client::ClientHandler;
use anyhow::{Context, Result};
use russh::client::Handle;
use russh::{ChannelId, ChannelMsg};
use tokio::sync::mpsc;

pub enum PtyCommand {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

pub struct PtyChannel {
    cmd_tx: mpsc::Sender<PtyCommand>,
    pub channel_id: Option<ChannelId>,
}

impl PtyChannel {
    pub async fn new(
        handle: &Handle<ClientHandler>,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Self> {
        let channel = handle
            .channel_open_session()
            .await
            .context("Failed to open SSH session channel")?;

        channel
            .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
            .await
            .context("Failed to request PTY")?;

        channel
            .request_shell(false)
            .await
            .context("Failed to request remote shell")?;

        let channel_id = channel.id();
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PtyCommand>(256);

        // Dedicated background actor task managing Channel ownership
        tokio::spawn(async move {
            let mut channel = channel;
            loop {
                tokio::select! {
                    biased;
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(PtyCommand::Data(data)) => {
                                if channel.data(data.as_slice()).await.is_err() {
                                    break;
                                }
                            }
                            Some(PtyCommand::Resize { cols, rows }) => {
                                if channel.window_change(cols, rows, 0, 0).await.is_err() {
                                    break;
                                }
                            }
                            Some(PtyCommand::Close) | None => {
                                let _ = channel.close().await;
                                break;
                            }
                        }
                    }
                    msg = channel.wait() => {
                        match msg {
                            Some(ChannelMsg::Data { ref data }) => {
                                if output_tx.send(data.to_vec()).await.is_err() {
                                    break;
                                }
                            }
                            Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                                if output_tx.send(data.to_vec()).await.is_err() {
                                    break;
                                }
                            }
                            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            let _ = channel.close().await;
        });

        Ok(Self { cmd_tx, channel_id: Some(channel_id) })
    }

    pub async fn new_reverse_ws(
        ws_url: &str,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Self> {
        Self::new_reverse_ws_with_auth(ws_url, cols, rows, output_tx, None).await
    }

    pub async fn new_happy_eyeballs_ws(
        direct_url: Option<&str>,
        hub_url: &str,
        _cols: u32,
        _rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
        client_signing_key_hex: Option<&str>,
    ) -> Result<Self> {
        use std::time::Duration;

        if let Some(lan_url) = direct_url {
            log::info!("⚡ Happy Eyeballs: Racing LAN direct ({}) vs Hub relay ({})", lan_url, hub_url);

            let lan_fut = connect_reverse_ws(lan_url, output_tx.clone(), client_signing_key_hex);
            let hub_out_tx = output_tx.clone();
            let hub_fut = async move {
                tokio::time::sleep(Duration::from_millis(75)).await;
                connect_reverse_ws(hub_url, hub_out_tx, client_signing_key_hex).await
            };

            tokio::pin!(lan_fut);
            tokio::pin!(hub_fut);

            tokio::select! {
                biased;
                lan_res = &mut lan_fut => {
                    match lan_res {
                        Ok((stream, e2ee)) => {
                            log::info!("⚡ Happy Eyeballs: Won by LAN direct path: {} (0 relay bandwidth, lowest latency)", lan_url);
                            Ok(from_ws_stream(stream, e2ee, output_tx))
                        }
                        Err(e) => {
                            log::debug!("LAN direct failed ({}); immediately falling back to Hub {}", e, hub_url);
                            let (stream, e2ee) = connect_reverse_ws(hub_url, output_tx.clone(), client_signing_key_hex).await?;
                            log::info!("☁️ Connected via Hub relay: {}", hub_url);
                            Ok(from_ws_stream(stream, e2ee, output_tx))
                        }
                    }
                }
                hub_res = &mut hub_fut => {
                    match hub_res {
                        Ok((stream, e2ee)) => {
                            log::info!("☁️ Happy Eyeballs: Won by Hub relay: {}", hub_url);
                            Ok(from_ws_stream(stream, e2ee, output_tx))
                        }
                        Err(e) => {
                            log::warn!("Hub relay failed in race ({}). Waiting for LAN...", e);
                            let (stream, e2ee) = lan_fut.await?;
                            Ok(from_ws_stream(stream, e2ee, output_tx))
                        }
                    }
                }
            }
        } else {
            let (stream, e2ee) = connect_reverse_ws(hub_url, output_tx.clone(), client_signing_key_hex).await?;
            Ok(from_ws_stream(stream, e2ee, output_tx))
        }
    }

    pub async fn new_reverse_ws_with_auth(
        ws_url: &str,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
        client_signing_key_hex: Option<&str>,
    ) -> Result<Self> {
        Self::new_happy_eyeballs_ws(None, ws_url, cols, rows, output_tx, client_signing_key_hex).await
    }

    pub fn is_closed(&self) -> bool {
        self.cmd_tx.is_closed()
    }
    pub fn try_send_data(&self, data: &[u8]) -> Result<()> {
        anyhow::ensure!(
            data.len() <= 1024 * 1024,
            "Input exceeds the 1 MiB paste limit"
        );
        self.cmd_tx
            .try_send(PtyCommand::Data(data.to_vec()))
            .context("PTY input queue is full or closed; input was not sent")?;
        Ok(())
    }

    pub async fn send_data(&self, data: &[u8]) -> Result<()> {
        self.cmd_tx
            .send(PtyCommand::Data(data.to_vec()))
            .await
            .context("Failed to send data to SSH PTY channel actor")?;
        Ok(())
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        self.cmd_tx
            .send(PtyCommand::Resize { cols, rows })
            .await
            .context("Failed to send resize to SSH PTY channel actor")?;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        let _ = self.cmd_tx.send(PtyCommand::Close).await;
        Ok(())
    }
}

type ReverseWsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect_reverse_ws(
    ws_url: &str,
    output_tx: mpsc::Sender<Vec<u8>>,
    client_signing_key_hex: Option<&str>,
) -> Result<(ReverseWsStream, Option<redash_ui_core::e2ee::ClientE2eeSession>)> {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let (mut ws_stream, _) = connect_async(ws_url)
        .await
        .with_context(|| format!("Failed to connect to WebSocket: {}", ws_url))?;

    let e2ee_session = if let Some(priv_hex) = client_signing_key_hex {
        let session_id = format!("e2ee-{}", uuid::Uuid::new_v4());
        match redash_ui_core::e2ee::ClientE2eeSession::initiate(&session_id, priv_hex) {
            Ok((mut session, init)) => {
                if let Ok(init_json) = serde_json::to_string(&init) {
                    ws_stream
                        .send(Message::Text(init_json.into()))
                        .await
                        .context("Failed to send E2EE handshake init")?;
                    let mut handshake_ok = false;
                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(ack) =
                                    serde_json::from_str::<redash_types::E2eeHandshakeAck>(&text)
                                {
                                    if let Err(e) = session.complete_handshake(&ack) {
                                        log::warn!("E2EE handshake rejected on {}: {}", ws_url, e);
                                    } else {
                                        log::info!(
                                            "E2EE encryption established for session {} on {}",
                                            session_id,
                                            ws_url
                                        );
                                        handshake_ok = true;
                                    }
                                    break;
                                }
                            }
                            Ok(Message::Binary(bin)) => {
                                let _ = output_tx.send(bin.to_vec()).await;
                            }
                            _ => break,
                        }
                    }
                    if handshake_ok {
                        Some(session)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Err(e) => {
                log::warn!("Failed to initiate E2EE session for {}: {}", ws_url, e);
                None
            }
        }
    } else {
        None
    };

    Ok((ws_stream, e2ee_session))
}

fn from_ws_stream(
    ws_stream: ReverseWsStream,
    mut e2ee_session: Option<redash_ui_core::e2ee::ClientE2eeSession>,
    output_tx: mpsc::Sender<Vec<u8>>,
) -> PtyChannel {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (mut ws_sink, mut ws_reader) = ws_stream.split();
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<PtyCommand>(256);
    let channel_id = None;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(PtyCommand::Data(data)) => {
                            if let Some(session) = e2ee_session.as_mut() {
                                let env = session.seal(&data);
                                if let Ok(json) = serde_json::to_string(&env)
                                    && ws_sink.send(Message::Text(json.into())).await.is_err()
                                {
                                    break;
                                }
                            } else if ws_sink.send(Message::Binary(data.into())).await.is_err() {
                                break;
                            }
                        }
                        Some(PtyCommand::Resize { cols, rows }) => {
                            let payload = serde_json::json!({
                                "type": "resize",
                                "cols": cols,
                                "rows": rows,
                            });
                            if ws_sink.send(Message::Text(payload.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        Some(PtyCommand::Close) | None => {
                            let _ = ws_sink.close().await;
                            break;
                        }
                    }
                }
                msg = ws_reader.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            if let Some(session) = e2ee_session.as_mut()
                                && let Ok(env) = serde_json::from_slice::<redash_types::EncryptedEnvelope>(&data)
                            {
                                match session.open(&env) {
                                    Ok(plaintext) => {
                                        if output_tx.send(plaintext).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("E2EE decryption error: {}", e);
                                    }
                                }
                                continue;
                            }
                            if output_tx.send(data.to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Text(text))) => {
                            if let Some(session) = e2ee_session.as_mut()
                                && let Ok(env) = serde_json::from_str::<redash_types::EncryptedEnvelope>(&text)
                            {
                                match session.open(&env) {
                                    Ok(plaintext) => {
                                        if output_tx.send(plaintext).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("E2EE decryption error: {}", e);
                                    }
                                }
                                continue;
                            }
                            if output_tx.send(text.as_bytes().to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        let _ = ws_sink.close().await;
    });

    PtyChannel {
        cmd_tx,
        channel_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_happy_eyeballs_lan_endpoint_race_structure() {
        let (output_tx, _output_rx) = mpsc::channel(16);

        // When neither endpoint is listening, connection returns Err gracefully
        let result = PtyChannel::new_happy_eyeballs_ws(
            Some("ws://127.0.0.1:59998/v1/agent/direct"),
            "ws://127.0.0.1:59999/v1/control/tty/test",
            80,
            24,
            output_tx,
            None,
        )
        .await;

        assert!(result.is_err());
    }
}

