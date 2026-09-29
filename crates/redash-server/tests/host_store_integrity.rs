use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use redash_core::config::{HostConfig, HostStore};
use redash_server::{auth::GatewayAuth, build_router, state::AppState};
use tower::ServiceExt;

const TOKEN: &str = "integrity-test-management-token-32-characters";
struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("redash-host-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn authorized(mut state: AppState) -> AppState {
    state.gateway_auth = GatewayAuth::new(TOKEN, vec![]).unwrap();
    state
}
async fn call(state: &AppState, method: &str, path: &str, host: Option<&HostConfig>) -> StatusCode {
    build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Authorization", format!("Bearer {TOKEN}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    host.map(|h| serde_json::to_vec(h).unwrap())
                        .unwrap_or_default(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}
#[tokio::test]
async fn corrupted_configuration_remains_visible_and_cannot_be_overwritten() {
    let dir = TempDir::new();
    let path = dir.0.join("hosts.json");
    let original = b"{malformed but valuable host configuration";
    std::fs::write(&path, original).unwrap();
    let state = authorized(AppState::from_host_path(&path));
    assert_eq!(
        call(&state, "GET", "/api/hosts", None).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let host = HostConfig::new("new host", "localhost", "test");
    assert_eq!(
        call(&state, "POST", "/api/hosts", Some(&host)).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(state.host_store.read().unwrap().hosts.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
#[tokio::test]
async fn failed_save_and_delete_leave_the_committed_store_unchanged() {
    let dir = TempDir::new();
    let path = dir.0.join("hosts.json");
    // A directory where the destination file should be makes atomic rename fail.
    std::fs::create_dir(&path).unwrap();
    let mut store = HostStore::with_path(&path);
    let host = HostConfig::new("existing", "localhost", "test");
    store.hosts.push(host.clone());
    let state = authorized(AppState::with_host_store(store));
    let mut modified = host.clone();
    modified.name = "unsaved".into();
    assert_eq!(
        call(&state, "POST", "/api/hosts", Some(&modified)).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(state.get_host(&host.id.0).await.unwrap(), host);
    assert_eq!(
        call(&state, "DELETE", &format!("/api/hosts/{}", host.id.0), None).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(state.get_host(&host.id.0).await.unwrap(), host);
}
#[tokio::test]
async fn gateway_proxy_resolver_reads_successful_edits_and_deletions() {
    let dir = TempDir::new();
    let state = authorized(AppState::with_host_store(HostStore::with_path(
        dir.0.join("hosts.json"),
    )));
    let mut jump = HostConfig::new("jump", "localhost", "test");
    let mut target = HostConfig::new("target", "localhost", "test");
    target.proxy_jump_id = Some(jump.id.clone());
    jump.proxy_jump_id = Some(target.id.clone());
    for host in [&target, &jump] {
        assert_eq!(
            call(&state, "POST", "/api/hosts", Some(host)).await,
            StatusCode::OK
        );
    }
    // Resolving both committed records reaches cycle detection before any network IO.
    let error = state
        .session_mgr
        .get_or_connect(&target)
        .await
        .err()
        .unwrap();
    assert!(
        format!("{error:#}").contains("ProxyJump cycle"),
        "{error:#}"
    );
    assert_eq!(
        call(&state, "DELETE", &format!("/api/hosts/{}", jump.id.0), None).await,
        StatusCode::OK
    );
    let error = state
        .session_mgr
        .get_or_connect(&target)
        .await
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("未找到"), "{error:#}");
    assert!(
        HostStore::load_from_file(&dir.0.join("hosts.json"))
            .unwrap()
            .find(&jump.id)
            .is_none()
    );
}
