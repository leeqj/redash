use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use redash_core::sftp::{PagedFileResult, SftpManager};
use serde::Deserialize;

use super::hosts::ApiResponse;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub path: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub filter: Option<String>,
}

pub async fn list_directory(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let host = state.get_host(&host_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
            }),
        )
    })?;

    let sftp = state.session_mgr.open_sftp(&host).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to open SFTP session: {}", e)),
            }),
        )
    })?;

    let target_path = query.path.unwrap_or_else(|| ".".to_string());
    let items = SftpManager::list_dir(&sftp, &target_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: Some(format!("Failed to list directory: {}", e)),
                }),
            )
        })?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(100);
    let paged = PagedFileResult::paginate(items, page, page_size, query.filter.as_deref());

    Ok(Json(ApiResponse {
        success: true,
        data: Some(paged),
        message: None,
    }))
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub path: String,
}

pub async fn read_file_content(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let host = state.get_host(&host_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
            }),
        )
    })?;

    let sftp = state.session_mgr.open_sftp(&host).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to open SFTP session: {}", e)),
            }),
        )
    })?;

    let bytes = SftpManager::read_full_file(&sftp, &query.path, 2 * 1024 * 1024)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: Some(format!("Failed to read file: {}", e)),
                }),
            )
        })?;

    let text = String::from_utf8(bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Binary file preview not supported via text endpoint".to_string()),
            }),
        )
    })?;

    Ok(Json(ApiResponse {
        success: true,
        data: Some(text),
        message: None,
    }))
}

#[derive(Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

async fn write_file_helper(
    sftp: &russh_sftp::client::SftpSession,
    path: &str,
    data: &[u8],
) -> anyhow::Result<()> {
    if sftp.symlink_metadata(path).await.is_ok() {
        SftpManager::write_file_atomic(sftp, path, data).await
    } else {
        use russh_sftp::protocol::{FileAttributes, OpenFlags};
        use tokio::io::AsyncWriteExt;
        let mut file = sftp
            .open_with_flags_and_attributes(
                path,
                OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE,
                FileAttributes {
                    permissions: Some(0o644),
                    ..FileAttributes::default()
                },
            )
            .await?;
        file.write_all(data).await?;
        file.flush().await?;
        file.close().await?;
        Ok(())
    }
}

pub async fn write_file_content(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Json(payload): Json<WriteFileRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let host = state.get_host(&host_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some("Host not found".to_string()),
            }),
        )
    })?;

    let sftp = state.session_mgr.open_sftp(&host).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to open SFTP session: {}", e)),
            }),
        )
    })?;

    write_file_helper(&sftp, &payload.path, payload.content.as_bytes())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: Some(format!("Failed to write file: {}", e)),
                }),
            )
        })?;

    Ok(Json(ApiResponse {
        success: true,
        data: Some(()),
        message: Some("File saved successfully".to_string()),
    }))
}
