use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use redash_core::config::alert::{AlertDispatcher, AlertEvent, AlertRule};
use redash_core::probe::ProbeScheduler;

use crate::state::AppState;

/// Autonomous Alert Monitoring Engine for ReDash Server.
///
/// Runs continuously in the background, polling hosts on schedule,
type AlertCooldownMap = Arc<Mutex<HashMap<(String, String), (Instant, bool)>>>;

/// Starts the autonomous background alert monitoring task,
/// refreshing the metrics cache, evaluating threshold rules,
/// and dispatching Webhook / Feishu alerts with cooldown protection.
pub fn start_alert_monitor(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting autonomous ReDash Server Alert Monitoring Engine in background...");
        let scheduler = ProbeScheduler::new(state.session_mgr.clone());
        let cooldowns: AlertCooldownMap = Arc::new(Mutex::new(HashMap::new()));

        loop {
            // 1. Fetch current probe interval and alert rules from saved settings
            let (interval_secs, rule) = {
                let s = state.app_settings.read().await;
                let interval = s.probe_interval_secs.clamp(1, 300);
                let rule = AlertRule {
                    cpu_threshold_percent: Some(s.alert_cpu_threshold),
                    mem_threshold_percent: Some(s.alert_mem_threshold),
                    disk_threshold_percent: Some(s.alert_disk_threshold),
                    notify_offline: s.alert_notify_offline,
                    macos_notification: false, // Headless server does not use local macOS UI
                    webhook_url: s.alert_webhook_url.clone(),
                };
                (interval, rule)
            };

            // 2. Fetch all configured hosts
            let hosts = {
                let store = state.host_store.read().await;
                store.hosts.clone()
            };

            if !hosts.is_empty() {
                for host in hosts {
                    let host_id_str = host.id.0.clone();
                    let host_name = host.name.clone();

                    let poll_res = scheduler
                        .poll_host_fleet(&host, &redash_core::config::AppSettings::default())
                        .await;
                    let events = match &poll_res {
                        Ok(metrics) => {
                            // Update server metrics cache for fast dashboard queries
                            {
                                let mut cache = state.metrics_cache.write().await;
                                cache.insert(host.id.clone(), metrics.clone());
                            }
                            AlertDispatcher::evaluate_metrics(
                                &rule,
                                &host_id_str,
                                &host_name,
                                metrics,
                            )
                        }
                        Err(e) => {
                            if rule.notify_offline {
                                vec![AlertEvent {
                                    host_id: host_id_str.clone(),
                                    host_name: host_name.clone(),
                                    alert_type: "offline".into(),
                                    message: format!("{host_name} 探针离线或指标采集失败：{e:#}"),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs(),
                                }]
                            } else {
                                Vec::new()
                            }
                        }
                    };

                    for event in events {
                        let key = (event.host_id.clone(), event.alert_type.clone());
                        let mut cd = cooldowns.lock().await;
                        cd.retain(|_, (last, _)| last.elapsed() < Duration::from_secs(600));

                        // Cooldown: 300s (5m) after success, 30s after failure
                        if cd.get(&key).is_some_and(|(last, success)| {
                            last.elapsed() < Duration::from_secs(if *success { 300 } else { 30 })
                        }) {
                            continue;
                        }

                        cd.insert(key.clone(), (Instant::now(), false));
                        drop(cd);

                        let cd_clone = Arc::clone(&cooldowns);
                        let rule_clone = rule.clone();
                        tokio::spawn(async move {
                            info!(
                                "Server Alert triggered for {}: {}",
                                event.host_name, event.message
                            );
                            let outcome = AlertDispatcher::dispatch(&rule_clone, &event).await;
                            if let Err(e) = &outcome {
                                error!(
                                    "Server Alert delivery failed for {}: {e:#}",
                                    event.host_name
                                );
                            } else {
                                info!(
                                    "Server Alert delivered successfully to webhook for {}",
                                    event.host_name
                                );
                            }
                            cd_clone
                                .lock()
                                .await
                                .insert(key, (Instant::now(), outcome.is_ok()));
                        });
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    })
}
