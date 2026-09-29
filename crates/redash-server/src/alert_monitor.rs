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
pub(crate) type AlertCooldownMap = Arc<Mutex<HashMap<(String, String), (Instant, bool)>>>;

/// Starts the autonomous background alert monitoring task,
/// refreshing the metrics cache, evaluating threshold rules,
/// and dispatching Webhook / Feishu alerts with cooldown protection.
pub fn start_alert_monitor(state: AppState) -> tokio::task::JoinHandle<()> {
    let cooldowns: AlertCooldownMap = Arc::new(Mutex::new(HashMap::new()));
    let agent = AbortOnDrop(start_agent_alert_monitor(state.clone(), cooldowns.clone()));
    tokio::spawn(async move {
        let _agent = agent;
        info!("Starting autonomous ReDash Server Alert Monitoring Engine in background...");
        let scheduler = ProbeScheduler::new(state.session_mgr.clone());

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
                let store = state.host_store.read().unwrap();
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
                        dispatch_with_cooldown(&cooldowns, &rule, event).await;
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    })
}

fn start_agent_alert_monitor(
    state: AppState,
    cooldowns: AlertCooldownMap,
) -> tokio::task::JoinHandle<()> {
    // Subscribe before spawning so the first Agent-only sample cannot be missed.
    let mut samples = state.control_plane.subscribe_telemetry();
    tokio::spawn(async move {
        loop {
            let sample = match samples.recv().await {
                Ok(sample) => sample,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            let rule = {
                let s = state.app_settings.read().await;
                AlertRule {
                    cpu_threshold_percent: Some(s.alert_cpu_threshold),
                    mem_threshold_percent: Some(s.alert_mem_threshold),
                    disk_threshold_percent: Some(s.alert_disk_threshold),
                    webhook_url: s.alert_webhook_url.clone(),
                    ..Default::default()
                }
            };
            let events = AlertDispatcher::evaluate_agent_telemetry(&rule, &sample);
            // A valid recovery clears cooldown so a new incident is observable immediately.
            for (kind, valid) in [
                ("cpu", sample.validity.cpu),
                ("mem", sample.validity.memory),
                ("disk", sample.validity.disk),
            ] {
                if valid && !events.iter().any(|e| e.alert_type == kind) {
                    cooldowns
                        .lock()
                        .await
                        .remove(&(format!("agent:{}", sample.node_id), kind.into()));
                }
            }
            for event in events {
                dispatch_with_cooldown(&cooldowns, &rule, event).await;
            }
        }
    })
}

pub(crate) async fn dispatch_with_cooldown(
    cooldowns: &AlertCooldownMap,
    rule: &AlertRule,
    event: AlertEvent,
) {
    let key = (event.host_id.clone(), event.alert_type.clone());
    let mut cd = cooldowns.lock().await;
    cd.retain(|_, (last, _)| last.elapsed() < Duration::from_secs(600));
    if cd.get(&key).is_some_and(|(last, success)| {
        last.elapsed() < Duration::from_secs(if *success { 300 } else { 30 })
    }) {
        return;
    }
    let attempt = Instant::now();
    cd.insert(key.clone(), (attempt, false));
    drop(cd);
    let cooldowns = cooldowns.clone();
    let rule = rule.clone();
    tokio::spawn(async move {
        let result = AlertDispatcher::dispatch(&rule, &event).await;
        if let Err(err) = &result {
            error!("Alert delivery failed for {}: {}", event.host_name, err);
        }
        let mut cooldowns = cooldowns.lock().await;
        if cooldowns
            .get(&key)
            .is_some_and(|(time, _)| *time == attempt)
        {
            cooldowns.insert(key, (attempt, result.is_ok()));
        }
    });
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
