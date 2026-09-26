use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use redash_core::config::{AppSettings, AppSettingsExt};

use super::hosts::ApiResponse;
use crate::state::AppState;

pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    let settings = state.app_settings.read().await;
    Json(ApiResponse {
        success: true,
        data: Some(settings.clone()),
        message: None,
    })
}

pub async fn save_settings(
    State(state): State<AppState>,
    Json(new_settings): Json<AppSettings>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let mut settings = state.app_settings.write().await;
    *settings = new_settings.clone();

    if let Err(e) = new_settings.save_to_file(&AppSettings::default_path()) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to save settings: {}", e)),
            }),
        ));
    }

    Ok(Json(ApiResponse {
        success: true,
        data: Some(new_settings),
        message: Some("Settings saved successfully".to_string()),
    }))
}
