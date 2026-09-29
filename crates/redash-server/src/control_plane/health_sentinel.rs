use crate::{
    alert_monitor::{AlertCooldownMap, dispatch_with_cooldown},
    control_plane::registry::ControlPlaneRegistry,
    state::AppState,
};
use redash_core::config::alert::{AlertEvent, AlertRule};
use redash_types::NodeOnlineStatus;
use std::{sync::Arc, time::Duration};

/// Disconnects and heartbeat timeouts share retained state and retryable alert delivery.
pub fn start_health_sentinel(state: AppState, registry: ControlPlaneRegistry) {
    tokio::spawn(async move {
        let cooldowns: AlertCooldownMap = Arc::default();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            registry.sweep_health();
            let rule = {
                let settings = state.app_settings.read().await;
                AlertRule {
                    notify_offline: settings.alert_notify_offline,
                    webhook_url: settings.alert_webhook_url.clone(),
                    ..Default::default()
                }
            };
            for node in registry.list_nodes() {
                let id = format!("agent:{}", node.node_id);
                if node.status != NodeOnlineStatus::Offline {
                    cooldowns.lock().await.remove(&(id, "offline".into()));
                    continue;
                }
                if !rule.notify_offline {
                    continue;
                }
                dispatch_with_cooldown(
                    &cooldowns,
                    &rule,
                    AlertEvent {
                        host_id: id,
                        host_name: node.hostname.clone(),
                        alert_type: "offline".into(),
                        message: format!(
                            "受控节点 [{}] ({}) 已断开连接或心跳超时",
                            node.node_id, node.hostname
                        ),
                        timestamp: redash_ui_core::e2ee::now_secs(),
                    },
                )
                .await;
            }
        }
    });
}
