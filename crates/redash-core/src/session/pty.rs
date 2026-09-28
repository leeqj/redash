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
        _cols: u32,
        _rows: u32,
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Self> {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::connect_async;
        use tokio_tungstenite::tungstenite::Message;

        let (ws_stream, _) = connect_async(ws_url)
            .await
            .context("Failed to connect to Reverse WebTTY WebSocket bridge")?;

        let (mut ws_sink, mut ws_reader) = ws_stream.split();
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PtyCommand>(256);
        let channel_id = None;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(PtyCommand::Data(data)) => {
                                if ws_sink.send(Message::Binary(data.into())).await.is_err() {
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
                                if output_tx.send(data.to_vec()).await.is_err() {
                                    break;
                                }
                            }
                            Some(Ok(Message::Text(text))) => {
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

        Ok(Self { cmd_tx, channel_id })
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
