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
                let now = redash_ui_core::e2ee::now_secs();
                let kind = health_alert_kind(&node, now);
                for recovered in ["offline", "telemetry_stale"] {
                    if kind != Some(recovered) {
                        cooldowns
                            .lock()
                            .await
                            .remove(&(id.clone(), recovered.into()));
                    }
                }
                if !rule.notify_offline {
                    continue;
                }
                let Some(kind) = kind else { continue };
                dispatch_with_cooldown(
                    &cooldowns,
                    &rule,
                    AlertEvent {
                        host_id: id,
                        host_name: node.hostname.clone(),
                        alert_type: kind.into(),
                        message: if kind == "telemetry_stale" {
                            format!(
                                "受控节点 [{}] ({}) 心跳仍在线，但指标采集已过期",
                                node.node_id, node.hostname
                            )
                        } else {
                            format!(
                                "受控节点 [{}] ({}) 已断开连接或心跳超时",
                                node.node_id, node.hostname
                            )
                        },
                        timestamp: now,
                    },
                )
                .await;
            }
        }
    });
}

pub(super) fn health_alert_kind(
    node: &redash_types::ManagedNodeDetail,
    now: u64,
) -> Option<&'static str> {
    if node.status == NodeOnlineStatus::Offline {
        Some("offline")
    } else if node.telemetry_is_stale(now) {
        Some("telemetry_stale")
    } else {
        None
    }
}
