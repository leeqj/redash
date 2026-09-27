use crate::session::client::ClientHandler;
use anyhow::{Context, Result};
use russh::{
    ChannelMsg, ChannelWriteHalf, Sig,
    client::{Handle, Msg},
};
use std::time::Duration;
use tokio::time::{Instant, timeout, timeout_at};

const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u32,
    pub duration_ms: u64,
}

// Dropping an execution (including an aborted batch task) still closes its channel.
struct ExecGuard(Option<ChannelWriteHalf<Msg>>);
impl ExecGuard {
    async fn finish(&mut self, terminate: bool) {
        if let Some(writer) = self.0.as_ref() {
            cleanup(writer, terminate).await;
        }
        self.0.take();
    }
}
async fn cleanup(writer: &ChannelWriteHalf<Msg>, terminate: bool) {
    let _ = timeout(Duration::from_secs(1), async {
        if terminate {
            let _ = writer.signal(Sig::TERM).await;
        }
        let _ = writer.eof().await;
        let _ = writer.close().await;
    })
    .await;
}
impl Drop for ExecGuard {
    fn drop(&mut self) {
        if let Some(writer) = self.0.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                cleanup(&writer, true).await;
            });
        }
    }
}

pub struct ExecChannel;
impl ExecChannel {
    pub async fn execute(
        handle: &Handle<ClientHandler>,
        command: &str,
        duration: Duration,
    ) -> Result<ExecResult> {
        let start = Instant::now();
        let deadline = start + duration;
        let channel = timeout_at(deadline, handle.channel_open_session())
            .await
            .context("Deadline exceeded opening command channel")??;
        let (mut reader, writer) = channel.split();
        let mut guard = ExecGuard(Some(writer));
        let result = timeout_at(deadline, async {
            guard.0.as_ref().context("Failed to acquire command channel writer")?.exec(true, command).await.context("Failed to request exec")?;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_code = None;
            while let Some(message) = reader.wait().await {
                match message {
                    ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                    ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                    ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                    ChannelMsg::ExitSignal { signal_name, .. } => anyhow::bail!("Command terminated by signal {signal_name:?}"),
                    ChannelMsg::Failure => anyhow::bail!("SSH server rejected command execution"),
                    ChannelMsg::Close => break,
                    _ => {},
                }
                anyhow::ensure!(stdout.len().saturating_add(stderr.len()) <= MAX_OUTPUT_BYTES, "Command output exceeded the 8 MiB limit; channel closed");
            }
            Ok(ExecResult { stdout: String::from_utf8_lossy(&stdout).into_owned(), stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_code: exit_code.context("SSH channel closed without an exit status; command outcome is unknown")?, duration_ms: start.elapsed().as_millis() as u64 })
        }).await.unwrap_or_else(|_| Err(anyhow::anyhow!("Command deadline exceeded; TERM and channel close requested. Detached remote processes may still be running")));
        guard.finish(result.is_err()).await;
        result
    }
}
