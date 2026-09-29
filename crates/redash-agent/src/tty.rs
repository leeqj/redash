use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use redash_types::{AgentToHubMessage, E2eeHandshakeAck, E2eeHandshakeInit, EncryptedEnvelope, TtyClientFrame};
use redash_ui_core::e2ee::{AgentE2eeSession, now_secs};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, mpsc};

const MAX_SESSIONS: usize = 32;
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);

type PtyWriter = Arc<StdMutex<Box<dyn Write + Send>>>;
struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: PtyWriter,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}
impl Drop for Pty {
    fn drop(&mut self) {
        // Terminate foreground jobs as well as the session leader, then reap the shell.
        #[cfg(unix)]
        if let Some(fd) = self.master.as_raw_fd() {
            let pgid = unsafe { libc::tcgetpgrp(fd) };
            if pgid > 1 { unsafe { libc::kill(-pgid, libc::SIGKILL); } }
        }
        #[cfg(unix)]
        if let Some(pid) = self.child.process_id() {
            unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Session {
    cipher: AgentE2eeSession,
    pty: Option<Pty>,
    output: mpsc::Sender<AgentToHubMessage>,
    route: String,
    active: Instant,
}

#[derive(Default)]
pub struct TtyManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    seen: Mutex<HashMap<String, u64>>,
}

impl TtyManager {
    pub fn new() -> Self { Self::default() }

    /// Authenticate before allocating a session. No process is created until encrypted Open.
    pub async fn accept_handshake(&self, init: &E2eeHandshakeInit, trusted: &str, identity: &str, node_id: &str, route: &str, output: mpsc::Sender<AgentToHubMessage>) -> Result<E2eeHandshakeAck, String> {
        let (cipher, ack) = AgentE2eeSession::respond(init, trusted, identity, node_id)?;
        let mut seen = self.seen.lock().await;
        seen.retain(|_, ts| now_secs().saturating_sub(*ts) <= 120);
        if seen.contains_key(&init.nonce) || seen.len() >= 4096 { return Err("Repeated handshake or handshake limit reached".into()); }
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, s| s.active.elapsed() < if s.pty.is_some() { IDLE_TIMEOUT } else { Duration::from_secs(10) });
        if sessions.contains_key(&init.session_id) || sessions.len() >= MAX_SESSIONS { return Err("Session already exists or session limit reached".into()); }
        seen.insert(init.nonce.clone(), init.timestamp);
        sessions.insert(init.session_id.clone(), Session { cipher, pty: None, output, route: route.into(), active: Instant::now() });
        Ok(ack)
    }

    pub async fn receive(&self, envelope: &EncryptedEnvelope, route: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&envelope.session_id).ok_or("Unknown terminal session")?;
        if session.route != route { return Err("Session belongs to another transport".into()); }
        let plaintext = session.cipher.open(envelope)?;
        let frame: TtyClientFrame = serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
        session.active = Instant::now();
        match frame {
            TtyClientFrame::Open { rows, cols } => {
                if session.pty.is_some() { return Err("Terminal already open".into()); }
                let pair = native_pty_system().openpty(size(rows, cols)?).map_err(|e| e.to_string())?;
                let mut command = CommandBuilder::new(redash_types::default_system_shell());
                command.arg("-i");
                command.env("TERM", "xterm-256color");
                let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
                let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
                let child = pair.slave.spawn_command(command).map_err(|e| e.to_string())?;
                drop(pair.slave);
                session.pty = Some(Pty { master: pair.master, writer: Arc::new(StdMutex::new(writer)), child });
                let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
                tokio::task::spawn_blocking(move || {
                    let mut buf = [0; 4096];
                    while let Ok(n) = reader.read(&mut buf) {
                        if n == 0 || tx.blocking_send(buf[..n].to_vec()).is_err() { break; }
                    }
                });
                let map = self.sessions.clone();
                let sid = envelope.session_id.clone();
                tokio::spawn(async move {
                    while let Some(bytes) = rx.recv().await {
                        let outgoing = {
                            let mut map = map.lock().await;
                            let Some(s) = map.get_mut(&sid) else { break; };
                            match s.cipher.seal(&bytes) {
                                Ok(env) => (s.output.clone(), AgentToHubMessage::TtyEncrypted(env)),
                                Err(_) => break,
                            }
                        };
                        if tokio::time::timeout(Duration::from_secs(5), outgoing.0.send(outgoing.1)).await.is_ok_and(|r| r.is_ok()) == false { break; }
                    }
                    map.lock().await.remove(&sid);
                });
            }
            TtyClientFrame::Input { data } => {
                if data.len() > 1024 * 1024 { return Err("Input too large".into()); }
                let writer = session.pty.as_ref().ok_or("Terminal not open")?.writer.clone();
                drop(sessions);
                tokio::task::spawn_blocking(move || {
                    let mut w = writer.lock().map_err(|e| e.to_string())?;
                    w.write_all(&data).and_then(|_| w.flush()).map_err(|e| e.to_string())
                }).await.map_err(|e| e.to_string())??;
            }
            TtyClientFrame::Resize { rows, cols } => {
                session.pty.as_ref().ok_or("Terminal not open")?.master.resize(size(rows, cols)?).map_err(|e| e.to_string())?;
            }
            TtyClientFrame::Close => { sessions.remove(&envelope.session_id); }
        }
        Ok(())
    }

    pub async fn close_session(&self, sid: &str, route: &str) {
        let mut sessions = self.sessions.lock().await;
        if sessions.get(sid).is_some_and(|s| s.route == route) { sessions.remove(sid); }
    }
    pub async fn close_route(&self, route: &str) {
        self.sessions.lock().await.retain(|_, s| s.route != route);
    }
    pub async fn expire_sessions(&self) {
        self.sessions.lock().await.retain(|_, s| s.active.elapsed() < if s.pty.is_some() { IDLE_TIMEOUT } else { Duration::from_secs(10) });
    }
    pub async fn session_count(&self) -> usize { self.sessions.lock().await.len() }
}

fn size(rows: u16, cols: u16) -> Result<PtySize, String> {
    if rows == 0 || cols == 0 { return Err("Terminal dimensions must be nonzero".into()); }
    Ok(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
}
