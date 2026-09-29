use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use redash_core::config::HostConfig;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub async fn list_hosts(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.host_store.read().unwrap();
    if let Some(error) = &store.load_error {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()> {
                success: false,
                data: None,
                message: Some(error.clone()),
            }),
        )
            .into_response();
    }
    Json(ApiResponse {
        success: true,
        data: Some(store.hosts.clone()),
        message: None,
    })
    .into_response()
}

pub async fn get_host(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<HostConfig>>)> {
    match state.get_host(&id).await {
        Some(host) => Ok(Json(ApiResponse {
            success: true,
            data: Some(host),
            message: None,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
            }),
        )),
    }
}

pub async fn save_host(
    State(state): State<AppState>,
    Json(host): Json<HostConfig>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    if let Err(e) = host.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Invalid host configuration: {}", e)),
            }),
        ));
    }

    let host_clone = host.clone();
    let store = state.host_store.clone();
    let saved = tokio::task::spawn_blocking(move || {
        store
            .write()
            .unwrap()
            .save_host(host)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    if let Err(e) = saved {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to save host to disk: {}", e)),
            }),
        ));
    }

    Ok(Json(ApiResponse {
        success: true,
        data: Some(host_clone),
        message: Some("Host saved successfully".to_string()),
    }))
}

pub async fn delete_host(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let host_id = redash_core::config::HostId(id);
    let store = state.host_store.clone();
    let id = host_id.clone();
    let deleted = tokio::task::spawn_blocking(move || {
        store
            .write()
            .unwrap()
            .delete_host(&id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    match deleted {
        Ok(Some(_)) => {
            state.session_mgr.disconnect(&host_id).await;
            state.metrics_cache.write().await.remove(&host_id);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Host deleted successfully".to_string()),
            }))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
            }),
        )),
        Err(error) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(error),
            }),
        )),
    }
}

pub async fn test_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let host = state.get_host(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
            }),
        )
    })?;

    match state.session_mgr.test_connection(&host).await {
        Ok(_) => Ok(Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Connection test successful".to_string()),
        })),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("SSH connection failed: {}", e)),
            }),
        )),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct TestDraftHostRequest {
    pub hostname: String,
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
}

pub async fn test_draft_connection(
    State(state): State<AppState>,
    Json(req): Json<TestDraftHostRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let hostname = req.hostname.trim().to_string();
    if hostname.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("主机名/IP地址不能为空 (Hostname cannot be empty)".to_string()),
            }),
        ));
    }
    let port = if req.port == 0 { 22 } else { req.port };
    let user = if req.user.trim().is_empty() {
        "root".to_string()
    } else {
        req.user.trim().to_string()
    };

    let auth = if let Some(_pwd) = req.password.as_deref().filter(|p| !p.is_empty()) {
        redash_core::config::AuthMethod::Password {
            credential_id: "draft_pwd".to_string(),
        }
    } else if let Some(key) = req.key_path.as_deref().filter(|k| !k.is_empty()) {
        redash_core::config::AuthMethod::PrivateKey {
            key_path: std::path::PathBuf::from(key),
            passphrase_id: req.passphrase.as_ref().map(|_| "draft_pass".to_string()),
        }
    } else {
        redash_core::config::AuthMethod::Agent
    };

    let host = redash_core::config::HostConfig {
        id: redash_core::config::HostId::new(),
        name: "Test Draft".to_string(),
        hostname,
        port,
        user,
        auth,
        group: "Default".to_string(),
        tags: vec![],
        target_os: redash_core::config::TargetOs::Linux,
        jump_host: None,
        proxy_jump_id: None,
        bandwidth_limit_gb: None,
        bandwidth_reset_day: None,
    };

    match state
        .session_mgr
        .test_connection_with_credentials(&host, req.password.as_deref(), req.passphrase.as_deref())
        .await
    {
        Ok(duration) => Ok(Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some(format!(
                "Connection test successful ({}ms)",
                duration.as_millis()
            )),
        })),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("SSH connection failed: {}", e)),
            }),
        )),
    }
}
