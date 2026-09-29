use crate::session::client::ClientHandler;
use anyhow::{Context, Result};
use russh::client::Handle;
use russh::{ChannelId, ChannelMsg};
use tokio::sync::mpsc;

pub enum PtyCommand {
    Data(Vec<u8>),
    Open(redash_types::TtyClientFrame),
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
                            Some(PtyCommand::Open(_)) => break,
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

        Ok(Self {
            cmd_tx,
            channel_id: Some(channel_id),
        })
    }

    pub async fn new_reverse_ws(
        ws_url: &str,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Self> {
        Self::new_happy_eyeballs_ws(None, ws_url, cols, rows, output_tx, None).await
    }

    pub async fn new_happy_eyeballs_ws(
        direct_url: Option<&str>,
        hub_url: &str,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
        identity: Option<&TerminalIdentity>,
    ) -> Result<Self> {
        let identity = identity
            .context("Terminal requires a client identity and a pinned Agent public key")?;
        let (stream, session) = if let Some(direct) = direct_url {
            let lan = connect_reverse_ws(direct, identity, None);
            let hub = async {
                tokio::time::sleep(std::time::Duration::from_millis(75)).await;
                connect_reverse_ws(hub_url, identity, identity.gateway_token.as_deref()).await
            };
            tokio::pin!(lan, hub);
            tokio::select! {
                res = &mut lan => match res { Ok(pair) => pair, Err(_) => hub.await? },
                res = &mut hub => match res { Ok(pair) => pair, Err(_) => lan.await? },
            }
        } else {
            connect_reverse_ws(hub_url, identity, identity.gateway_token.as_deref()).await?
        };
        Self::open_authenticated_stream(stream, session, cols, rows, output_tx).await
    }

    pub async fn new_discovering_ws(
        hub_url: &str,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
        identity: &TerminalIdentity,
    ) -> Result<Self> {
        let lan = async {
            let endpoint = crate::discovery::LanDiscoveryClient::global()
                .wait_for_endpoint(&identity.node_id, std::time::Duration::from_millis(3500))
                .await
                .context("No LAN Agent discovered")?;
            connect_reverse_ws(&endpoint, identity, None).await
        };
        let hub = async {
            tokio::time::sleep(std::time::Duration::from_millis(75)).await;
            connect_reverse_ws(hub_url, identity, identity.gateway_token.as_deref()).await
        };
        tokio::pin!(lan, hub);
        let (stream, session) = tokio::select! {
            res = &mut lan => match res { Ok(pair) => pair, Err(_) => hub.await? },
            res = &mut hub => match res { Ok(pair) => pair, Err(hub_error) => lan.await.with_context(|| format!("Hub connection failed: {hub_error:#}"))? },
        };
        Self::open_authenticated_stream(stream, session, cols, rows, output_tx).await
    }

    async fn open_authenticated_stream(
        stream: ReverseWsStream,
        session: redash_ui_core::e2ee::ClientE2eeSession,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Self> {
        let channel = from_ws_stream(stream, session, output_tx);
        let open = redash_types::TtyClientFrame::Open {
            rows: rows.clamp(1, u16::MAX as u32) as u16,
            cols: cols.clamp(1, u16::MAX as u32) as u16,
        };
        channel
            .cmd_tx
            .send(PtyCommand::Open(open))
            .await
            .context("Failed to open terminal")?;
        Ok(channel)
    }

    pub async fn new_reverse_ws_with_auth(
        ws_url: &str,
        cols: u32,
        rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
        identity: Option<&TerminalIdentity>,
    ) -> Result<Self> {
        Self::new_happy_eyeballs_ws(None, ws_url, cols, rows, output_tx, identity).await
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
        anyhow::ensure!(
            data.len() <= 1024 * 1024,
            "Input exceeds the 1 MiB paste limit"
        );
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

#[derive(Clone)]
pub struct TerminalIdentity {
    pub node_id: String,
    pub client_private_key: String,
    pub agent_public_key: String,
    pub gateway_token: Option<String>,
}

async fn connect_reverse_ws(
    ws_url: &str,
    identity: &TerminalIdentity,
    gateway_token: Option<&str>,
) -> Result<(ReverseWsStream, redash_ui_core::e2ee::ClientE2eeSession)> {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = ws_url.into_client_request()?;
        if let Some(token) = gateway_token {
            request
                .headers_mut()
                .insert("Authorization", format!("Bearer {token}").parse()?);
        }
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await?;
        let sid = format!("tty-{}", uuid::Uuid::new_v4());
        let (mut session, init) = redash_ui_core::e2ee::ClientE2eeSession::initiate(
            &sid,
            &identity.node_id,
            &identity.client_private_key,
            &identity.agent_public_key,
        )
        .map_err(anyhow::Error::msg)?;
        ws.send(Message::Text(serde_json::to_string(&init)?.into()))
            .await?;
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let ack = serde_json::from_str::<redash_types::E2eeHandshakeAck>(&text)?;
                    session
                        .complete_handshake(&ack)
                        .map_err(anyhow::Error::msg)?;
                    return Ok((ws, session));
                }
                Some(Ok(Message::Ping(data))) => {
                    ws.send(Message::Pong(data)).await?;
                }
                _ => anyhow::bail!("Invalid or closed E2EE handshake"),
            }
        }
    })
    .await
    .context("Terminal authentication timed out")?
}

fn from_ws_stream(
    ws: ReverseWsStream,
    mut session: redash_ui_core::e2ee::ClientE2eeSession,
    output_tx: mpsc::Sender<Vec<u8>>,
) -> PtyChannel {
    use futures::{SinkExt, StreamExt};
    use redash_types::TtyClientFrame;
    use tokio_tungstenite::tungstenite::Message;
    let (mut sink, mut reader) = ws.split();
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<PtyCommand>(256);
    tokio::spawn(async move {
        let result: Result<()> = async {
            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        let frame = match cmd {
                            Some(PtyCommand::Open(frame)) => frame,
                            Some(PtyCommand::Data(data)) => TtyClientFrame::Input { data },
                            Some(PtyCommand::Resize { cols, rows }) => TtyClientFrame::Resize { rows: rows.clamp(1, u16::MAX as u32) as u16, cols: cols.clamp(1, u16::MAX as u32) as u16 },
                            Some(PtyCommand::Close) | None => TtyClientFrame::Close,
                        };
                        let env = session.seal(&serde_json::to_vec(&frame)?).map_err(anyhow::Error::msg)?;
                        // Serialize the Envelope, never its Result wrapper.
                        sink.send(Message::Text(serde_json::to_string(&env)?.into())).await?;
                        if matches!(frame, TtyClientFrame::Close) { break; }
                    }
                    msg = reader.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                let env = serde_json::from_str::<redash_types::EncryptedEnvelope>(&text)?;
                                let data = session.open(&env).map_err(anyhow::Error::msg)?;
                                if output_tx.send(data).await.is_err() { break; }
                            }
                            Some(Ok(Message::Ping(data))) => { sink.send(Message::Pong(data)).await?; }
                            Some(Ok(Message::Close(_))) | None => break,
                            Some(Err(err)) => return Err(err.into()),
                            _ => anyhow::bail!("Invalid terminal protocol frame"),
                        }
                    }
                }
            }
            Ok(())
        }.await;
        if let Err(err) = result {
            log::warn!("Encrypted terminal closed: {}", err);
        }
        let _ = sink.close().await;
    });
    PtyChannel {
        cmd_tx,
        channel_id: None,
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
