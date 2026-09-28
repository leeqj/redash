use crate::control_plane::registry::ControlPlaneRegistry;
use crate::state::AppState;
use log::{error, info, warn};
use redash_core::config::alert::{AlertDispatcher, AlertEvent};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// Spawns the autonomous health sentinel loop.
/// Periodically audits active agent connections and triggers out-of-band alerts upon node drops.
pub fn start_health_sentinel(state: AppState, registry: ControlPlaneRegistry) {
    tokio::spawn(async move {
        info!("Started Control-Plane Health Sentinel (out-of-band physical drop watcher)");

        loop {
            sleep(Duration::from_secs(5)).await;

            let newly_offline = registry.sweep_health();
            for node_id in newly_offline {
                warn!("SENTINEL TRIGGER: Node '{}' lost connection (physical drop/offline)", node_id);

                let webhook_url = {
                    let s = state.app_settings.read().await;
                    s.alert_webhook_url.clone()
                };

                if let Some(url) = webhook_url
                    && !url.is_empty()
                {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    let event = AlertEvent {
                        host_id: node_id.clone(),
                        host_name: node_id.clone(),
                        alert_type: "offline".to_string(),
                        message: format!("🚨 【受控节点失联】节点 [{}] 失去长连接心跳，可能遭遇物理断电或断网故障！", node_id),
                        timestamp: now,
                    };

                    if let Err(e) = AlertDispatcher::send_webhook(&url, &event).await {
                        error!("Failed to dispatch webhook alert to {}: {}", url, e);
                    }
                }
            }
        }
    });
}
