//! Shared native control-plane HTTP executor. Business failures remain typed ActionResults.
use anyhow::{Context, Result};
use redash_types::{ActionResult, SignedAction};
use std::time::Duration;

pub async fn dispatch_action(url: &str, action: &SignedAction) -> Result<ActionResult> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client
        .post(url)
        .json(action)
        .send()
        .await
        .context("Control-plane request failed")?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(
        status.is_success(),
        "Hub returned HTTP {}: {}",
        status,
        body
    );
    let result: ActionResult =
        serde_json::from_str(&body).context("Hub returned an invalid ActionResult")?;
    anyhow::ensure!(
        result.node_id == action.node_id && result.action_id == action.action_id,
        "Hub returned a result for another action"
    );
    Ok(result)
}

/// Pin only keys delivered through the local provisioning workflow, never through the Hub.
pub fn pin_agent_identity(path: &std::path::Path, node_id: &str, public_key: &str) -> Result<()> {
    let mut keys: std::collections::HashMap<String, String> = match std::fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).context("Invalid Agent identity store")?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Default::default(),
        Err(err) => return Err(err.into()),
    };
    if let Some(existing) = keys.get(node_id) {
        anyhow::ensure!(
            existing == public_key,
            "A different Agent identity is already pinned for {node_id}"
        );
    }
    keys.insert(node_id.into(), public_key.into());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, serde_json::to_vec_pretty(&keys)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

pub fn agent_identity_path() -> std::path::PathBuf {
    std::env::var_os("REDASH_AGENT_KEYS_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            crate::config::HostStore::default_path().with_file_name("agent_keys.json")
        })
}
