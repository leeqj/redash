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

#[derive(serde::Deserialize)]
pub struct TestWebhookRequest {
    pub webhook_url: Option<String>,
}

pub async fn test_webhook(
    State(state): State<AppState>,
    Json(payload): Json<TestWebhookRequest>,
) -> impl IntoResponse {
    let saved_url = state.app_settings.read().await.alert_webhook_url.clone();
    let url = payload.webhook_url.filter(|u| !u.trim().is_empty()).or(saved_url);

    let Some(webhook_url) = url else {
        return Json(ApiResponse {
            success: false,
            data: None,
            message: Some("未配置 Webhook URL，请先在界面设置或保存 Webhook 地址".to_string()),
        });
    };

    let is_feishu = redash_core::config::alert::AlertDispatcher::is_feishu_webhook(&webhook_url);
    let event = redash_core::config::alert::AlertEvent {
        host_id: "server-node-01".to_string(),
        host_name: "ReDash-Monitor-Service".to_string(),
        alert_type: "test".to_string(),
        message: "这是一条来自 ReDash 监控中心的告警测试通知，指标监控与机器人通道运转正常。".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    match redash_core::config::alert::AlertDispatcher::send_webhook(&webhook_url, &event).await {
        Ok(()) => {
            let target = if is_feishu { "飞书群机器人" } else { "Webhook" };
            Json(ApiResponse {
                success: true,
                data: Some(format!("{} 测试消息推送成功！", target)),
                message: None,
            })
        }
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("推送失败: {:#}", e)),
        }),
    }
}

