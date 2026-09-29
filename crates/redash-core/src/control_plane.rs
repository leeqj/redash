//! Shared native control-plane HTTP executor. Business failures remain typed ActionResults.
use anyhow::{Context, Result};
use redash_types::{ActionResult, SignedAction};
use std::time::Duration;

pub async fn dispatch_action(url: &str, action: &SignedAction) -> Result<ActionResult> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = authenticate_request(client.post(url).json(action))
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

/// The management credential is never derived from an Agent's enrollment token.
pub fn gateway_token() -> Option<String> {
    std::env::var("REDASH_GATEWAY_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
}
fn authenticate_request(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match gateway_token() {
        Some(token) => request.bearer_auth(token),
        None => request,
    }
}
pub async fn fetch_nodes(url: &str) -> Result<Vec<redash_types::ManagedNodeDetail>> {
    fetch_nodes_using(url, gateway_token().as_deref()).await
}
async fn fetch_nodes_using(
    url: &str,
    token: Option<&str>,
) -> Result<Vec<redash_types::ManagedNodeDetail>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let request = client.get(url);
    let request = match token {
        Some(token) => request.bearer_auth(token),
        None => request,
    };
    request
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("Invalid Hub node response")
}

#[cfg(test)]
mod gateway_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn node_fetch_authenticates_and_bounds_stalled_response_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = [0; 4096];
            let n = socket.read(&mut data).await.unwrap();
            assert!(
                String::from_utf8_lossy(&data[..n])
                    .to_lowercase()
                    .contains("authorization: bearer fixture-token")
            );
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nContent-Type: application/json\r\n\r\n[").await.unwrap();
            std::future::pending::<()>().await;
        });
        let response = tokio::time::timeout(
            Duration::from_secs(6),
            fetch_nodes_using(&format!("http://{addr}"), Some("fixture-token")),
        )
        .await;
        server.abort();
        let _ = server.await;
        assert!(
            response
                .expect("node request exceeded its total deadline")
                .is_err()
        );
    }
}
