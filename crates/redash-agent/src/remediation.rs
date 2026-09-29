use crate::collector::docker::DockerClient;
use ed25519_dalek::{Signature, VerifyingKey};
use log::{error, info, warn};
use redash_types::{ActionResult, RemediationAction, SignedAction};
use std::process::Stdio;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

pub struct RemediationEngine {
    trusted_public_key: Option<VerifyingKey>,
    docker_client: DockerClient,
    seen_nonces: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl RemediationEngine {
    pub fn new(trusted_public_key_hex: Option<&str>) -> anyhow::Result<Self> {
        let key = if let Some(hex_str) = trusted_public_key_hex {
            let bytes = hex::decode(hex_str)?;
            let key_bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid public key length, expected 32 bytes"))?;
            Some(VerifyingKey::from_bytes(&key_bytes)?)
        } else {
            None
        };

        Ok(Self {
            trusted_public_key: key,
            docker_client: DockerClient::new(),
            seen_nonces: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Verifies cryptographic signature and execution timestamp freshness.
    pub fn verify_signature(&self, action: &SignedAction) -> Result<(), String> {
        // 1. Timestamp freshness check: must be within 60 seconds of current time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if action.timestamp > now + 30 || now.saturating_sub(action.timestamp) > 60 {
            return Err(format!(
                "Action rejected: timestamp {} expired (current time {})",
                action.timestamp, now
            ));
        }

        // 3. Cryptographic signature check if trusted key is configured
        let trusted_key = self.trusted_public_key.as_ref().ok_or("Remote commands disabled: no trusted client key configured")?;
        {
            let canonical_bytes = SignedAction::canonical_signable_bytes(
                &action.action_id,
                &action.node_id,
                &action.action,
                action.timestamp,
                &action.nonce,
            )
            .map_err(|e| format!("Serialization error: {}", e))?;

            let sig_bytes = hex::decode(&action.signature_hex)
                .map_err(|e| format!("Invalid signature hex: {}", e))?;

            let sig_array: [u8; 64] = sig_bytes
                .try_into()
                .map_err(|_| "Signature must be 64 bytes".to_string())?;

            let signature = Signature::from_bytes(&sig_array);

            trusted_key
                .verify_strict(&canonical_bytes, &signature)
                .map_err(|e| format!("Cryptographic signature verification failed: {}", e))?;
        }

        if action.nonce.is_empty() || action.nonce.len() > 128 { return Err("Invalid nonce".into()); }
        // 2. Sliding window Nonce replay attack defense
        {
            let mut nonces = self
                .seen_nonces
                .lock()
                .map_err(|e| format!("Lock failure: {}", e))?;

            // Prune expired nonces older than 120 seconds
            nonces.retain(|_, ts| now.saturating_sub(*ts) <= 120);

            if nonces.contains_key(&action.nonce) {
                return Err(format!(
                    "Action rejected: duplicate nonce '{}' detected (replay attack thwarted)",
                    action.nonce
                ));
            }

            nonces.insert(action.nonce.clone(), action.timestamp);
        }

        Ok(())
    }

    /// Executes the validated remediation action.
    pub async fn execute(&self, action: SignedAction) -> ActionResult {
        let start = Instant::now();
        let action_id = action.action_id.clone();
        let node_id = action.node_id.clone();

        info!(
            "Executing remediation action {} (type: {:?})",
            action_id, action.action
        );

        if let Err(err) = self.verify_signature(&action) {
            warn!(
                "Security verification failed for action {}: {}",
                action_id, err
            );
            return ActionResult {
                action_id,
                node_id,
                success: false,
                exit_code: Some(403),
                stdout: String::new(),
                stderr: err,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }

        let (success, exit_code, stdout, stderr) = match action.action {
            RemediationAction::RestartContainer { container_id } => {
                match self.docker_client.restart_container(&container_id).await {
                    Ok(out) => (
                        true,
                        Some(0),
                        format!("Container {} restarted successfully: {}", container_id, out),
                        String::new(),
                    ),
                    Err(e) => (
                        false,
                        Some(1),
                        String::new(),
                        format!("Failed to restart container {}: {}", container_id, e),
                    ),
                }
            }
            RemediationAction::StopContainer { container_id } => {
                match self.docker_client.stop_container(&container_id).await {
                    Ok(out) => (
                        true,
                        Some(0),
                        format!("Container {} stopped successfully: {}", container_id, out),
                        String::new(),
                    ),
                    Err(e) => (
                        false,
                        Some(1),
                        String::new(),
                        format!("Failed to stop container {}: {}", container_id, e),
                    ),
                }
            }
            RemediationAction::PruneContainers => {
                match self.docker_client.prune_containers().await {
                    Ok(out) => (
                        true,
                        Some(0),
                        format!("Containers pruned successfully: {}", out),
                        String::new(),
                    ),
                    Err(e) => (
                        false,
                        Some(1),
                        String::new(),
                        format!("Failed to prune containers: {}", e),
                    ),
                }
            }
            RemediationAction::VacuumLogs { max_size_mb } => {
                let cmd_str = format!("journalctl --vacuum-size={}M", max_size_mb);
                run_shell_cmd(&cmd_str).await
            }
            RemediationAction::KillProcess { pid, signal } => {
                let my_pid = std::process::id();
                let ppid = unsafe { libc::getppid() as u32 };
                if pid == 0 || pid == 1 || pid == my_pid || pid == ppid {
                    (
                        false,
                        Some(403),
                        String::new(),
                        format!(
                            "Self-preservation alert: Refusing to kill protected PID {} (current agent PID={}, parent PID={})",
                            pid, my_pid, ppid
                        ),
                    )
                } else {
                    let res = unsafe { libc::kill(pid as libc::pid_t, signal) };
                    if res == 0 {
                        (
                            true,
                            Some(0),
                            format!("Signal {} sent to PID {}", signal, pid),
                            String::new(),
                        )
                    } else {
                        let err = std::io::Error::last_os_error();
                        (
                            false,
                            Some(res),
                            String::new(),
                            format!("Failed to signal PID {}: {}", pid, err),
                        )
                    }
                }
            }
            RemediationAction::ExecuteRecipe { name, script } => {
                info!("Executing custom recipe '{}'", name);
                run_shell_cmd(&script).await
            }
            RemediationAction::RestartService { service_name } => {
                info!("Restarting system service '{}'", service_name);
                let cmd = format!("systemctl restart {}", service_name);
                run_shell_cmd(&cmd).await
            }
            RemediationAction::DiagnosePort { port } => {
                info!("Diagnosing port {}", port);
                let cmd = format!("lsof -nP -i :{} || ss -lptn 'sport = :{}'", port, port);
                run_shell_cmd(&cmd).await
            }
            RemediationAction::KillPortConflict { port } => {
                info!("Releasing port conflict on port {}", port);
                if port == 0 {
                    (false, Some(400), String::new(), "Invalid port 0".to_string())
                } else if port == 22 {
                    (
                        false,
                        Some(403),
                        String::new(),
                        "Self-preservation alert: Port 22 is reserved for SSH remote management and protected against termination".to_string(),
                    )
                } else {
                    let my_pid = std::process::id();
                    let ppid = unsafe { libc::getppid() as u32 };
                    let cmd = format!(
                        "PIDS=$(lsof -ti :{port} 2>/dev/null || ss -lptn 'sport = :{port}' 2>/dev/null | grep -o 'pid=[0-9]*' | cut -d= -f2 | sort -u); \
                         if [ -z \"$PIDS\" ]; then echo 'No process found occupying port {port}'; exit 0; fi; \
                         KILLED=0; \
                         for p in $PIDS; do \
                           if [ \"$p\" -eq 1 ] || [ \"$p\" -eq {my_pid} ] || [ \"$p\" -eq {ppid} ]; then \
                             echo \"Skipping protected PID $p\"; \
                           else \
                             kill -9 \"$p\" 2>/dev/null && KILLED=$((KILLED + 1)); \
                           fi; \
                         done; \
                         echo \"Released port {port} by terminating $KILLED process(es)\""
                    );
                    run_shell_cmd(&cmd).await
                }
            }
            RemediationAction::TtyOpen { .. }
            | RemediationAction::TtyInput { .. }
            | RemediationAction::TtyResize { .. }
            | RemediationAction::TtyClose { .. } => (
                false,
                Some(1),
                String::new(),
                "TTY frames must be handled by TTY manager".to_string(),
            ),
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        ActionResult {
            action_id,
            node_id,
            success,
            exit_code,
            stdout,
            stderr,
            duration_ms,
        }
    }
}

async fn run_shell_cmd(cmd: &str) -> (bool, Option<i32>, String, String) {
    match Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) => {
            let success = output.status.success();
            let exit_code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            (success, exit_code, stdout, stderr)
        }
        Err(e) => {
            error!("Failed to spawn command '{}': {}", cmd, e);
            (
                false,
                None,
                String::new(),
                format!("Execution failure: {}", e),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_self_preservation_safeguards() {
        let engine = RemediationEngine::new(None).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 1. Test killing PID 1 is blocked
        let res_pid1 = engine.execute(SignedAction {
            action_id: "test-pid-1".to_string(),
            node_id: "node-1".to_string(),
            action: RemediationAction::KillProcess { pid: 1, signal: 9 },
            timestamp: now,
            nonce: "nonce-pid-1".to_string(),
            public_key_hex: String::new(),
            signature_hex: String::new(),
        }).await;
        assert_eq!(res_pid1.exit_code, Some(403));
        assert!(!res_pid1.success);
        assert!(res_pid1.stderr.contains("Refusing to kill protected PID"));

        // 2. Test killing self PID is blocked
        let my_pid = std::process::id();
        let res_self = engine.execute(SignedAction {
            action_id: "test-self-pid".to_string(),
            node_id: "node-1".to_string(),
            action: RemediationAction::KillProcess { pid: my_pid, signal: 9 },
            timestamp: now,
            nonce: "nonce-self-pid".to_string(),
            public_key_hex: String::new(),
            signature_hex: String::new(),
        }).await;
        assert_eq!(res_self.exit_code, Some(403));
        assert!(!res_self.success);
        assert!(res_self.stderr.contains("Refusing to kill protected PID"));

        // 3. Test killing port 22 is blocked
        let res_port22 = engine.execute(SignedAction {
            action_id: "test-port-22".to_string(),
            node_id: "node-1".to_string(),
            action: RemediationAction::KillPortConflict { port: 22 },
            timestamp: now,
            nonce: "nonce-port-22".to_string(),
            public_key_hex: String::new(),
            signature_hex: String::new(),
        }).await;
        assert_eq!(res_port22.exit_code, Some(403));
        assert!(!res_port22.success);
        assert!(res_port22.stderr.contains("Port 22 is reserved for SSH"));
    }
}

