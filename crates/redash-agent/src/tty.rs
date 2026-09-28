use log::{debug, error, info};
use redash_types::AgentToHubMessage;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc};

pub struct TtyManager {
    sessions: Arc<Mutex<HashMap<String, ChildStdin>>>,
    outbound_tx: mpsc::Sender<AgentToHubMessage>,
}

impl TtyManager {
    pub fn new(outbound_tx: mpsc::Sender<AgentToHubMessage>) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            outbound_tx,
        }
    }

    /// Spawns a shell process and pipes its output back into the WebSocket stream.
    pub async fn open_session(&self, session_id: String, _rows: u16, _cols: u16) {
        info!("Opening emergency reverse shell session: {}", session_id);

        let shell = if std::path::Path::new("/bin/bash").exists() {
            "/bin/bash"
        } else {
            "/bin/sh"
        };

        let mut cmd = Command::new(shell);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("TERM", "xterm-256color");

        let mut child: Child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn shell for session {}: {}", session_id, e);
                return;
            }
        };

        let stdin = child.stdin.take().expect("Child stdin must be piped");
        let mut stdout = child.stdout.take().expect("Child stdout must be piped");
        let mut stderr = child.stderr.take().expect("Child stderr must be piped");

        self.sessions.lock().await.insert(session_id.clone(), stdin);

        let sid_out = session_id.clone();
        let tx_out = self.outbound_tx.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx_out
                            .send(AgentToHubMessage::TtyOutput {
                                session_id: sid_out.clone(),
                                data: buf[..n].to_vec(),
                            })
                            .await;
                    }
                    Err(e) => {
                        debug!("TTY stdout read error {}: {}", sid_out, e);
                        break;
                    }
                }
            }
        });

        let sid_err = session_id.clone();
        let tx_err = self.outbound_tx.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx_err
                            .send(AgentToHubMessage::TtyOutput {
                                session_id: sid_err.clone(),
                                data: buf[..n].to_vec(),
                            })
                            .await;
                    }
                    Err(e) => {
                        debug!("TTY stderr read error {}: {}", sid_err, e);
                        break;
                    }
                }
            }
        });
    }

    /// Feeds incoming keystrokes/data from the web/desktop client into the shell's stdin.
    pub async fn write_input(&self, session_id: &str, data: &[u8]) {
        let mut sessions = self.sessions.lock().await;
        if let Some(stdin) = sessions.get_mut(session_id) {
            let _ = stdin.write_all(data).await;
            let _ = stdin.flush().await;
        }
    }

    /// Resizes a running session viewport.
    pub async fn resize_session(&self, session_id: &str, rows: u16, cols: u16) {
        info!("Resizing emergency shell session {}: {}x{}", session_id, cols, rows);
        let mut sessions = self.sessions.lock().await;
        if let Some(stdin) = sessions.get_mut(session_id) {
            let resize_cmd = format!("export COLUMNS={} LINES={}\n", cols, rows);
            let _ = stdin.write_all(resize_cmd.as_bytes()).await;
            let _ = stdin.flush().await;
        }
    }

    /// Closes a running session.
    pub async fn close_session(&self, session_id: &str) {
        info!("Closing emergency shell session: {}", session_id);
        self.sessions.lock().await.remove(session_id);
    }
}
