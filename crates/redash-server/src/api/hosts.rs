use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use redash_core::config::{HostConfig, HostStore};
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
    let store = state.host_store.read().await;
    Json(ApiResponse {
        success: true,
        data: Some(store.hosts.clone()),
        message: None,
    })
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

    let mut store = state.host_store.write().await;
    let host_clone = host.clone();
    store.add_or_update(host);

    if let Err(e) = store.save_to_file(&HostStore::default_path()) {
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
    let mut store = state.host_store.write().await;
    match store.delete_host(&redash_core::config::HostId(id)) {
        Ok(Some(_)) => {
            let _ = store.save_to_file(&HostStore::default_path());
            Ok(Json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Host deleted successfully".to_string()),
            }))
        }
        _ => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
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

    match state.session_mgr.get_or_connect(&host).await {
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
