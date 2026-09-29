use redash_types::NodeOnlineStatus;
pub use redash_types::math::MeterLevel;

/// Lightweight summary designed specifically for ambient menu bar items,
/// system tray icons, and floating HUD micro-indicators.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientFleetSummary {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub stale_nodes: usize,
    pub offline_nodes: usize,
    pub max_cpu_pct: f32,
    pub max_mem_pct: f32,
    pub max_disk_pct: f32,
    pub overall_level: MeterLevel,
    pub led_color_hex: &'static str,
    pub summary_label: String,
    pub status_tooltip: String,
    pub worst_culprit_name: Option<String>,
}

impl AmbientFleetSummary {
    pub fn is_healthy(&self) -> bool {
        self.overall_level == MeterLevel::Normal && self.offline_nodes == 0
    }
}

use redash_types::{HostConfig, ManagedNodeDetail, NodeMetrics};

/// Computes an instant, zero-allocation ambient health summary across all monitored nodes.
pub fn evaluate_fleet_ambient_summary<'a, I>(
    control_plane_nodes: &[ManagedNodeDetail],
    hosts: &[HostConfig],
    metrics_iter: I,
) -> AmbientFleetSummary
where
    I: IntoIterator<Item = (&'a str, &'a NodeMetrics)>,
{
    let total_nodes = control_plane_nodes.len().max(hosts.len());
    let mut online_nodes = 0;
    let mut stale_nodes = 0;
    let mut offline_nodes = 0;

    let mut max_cpu = 0.0f32;
    let mut max_mem = 0.0f32;
    let mut max_disk = 0.0f32;
    let mut worst_culprit: Option<(String, String)> = None; // (NodeName, Reason)

    // Health includes telemetry freshness independently of transport liveness.
    let now = crate::e2ee::now_secs();
    for node in control_plane_nodes {
        let telemetry_stale = node.telemetry_is_stale(now);
        match node.status {
            NodeOnlineStatus::Online if telemetry_stale => {
                stale_nodes += 1;
                if worst_culprit.is_none() {
                    worst_culprit = Some((node.hostname.clone(), "指标采集已过期".into()));
                }
            }
            NodeOnlineStatus::Online => online_nodes += 1,
            NodeOnlineStatus::Stale => {
                stale_nodes += 1;
                if worst_culprit.is_none() {
                    worst_culprit = Some((node.hostname.clone(), "心跳延迟 (Stale)".to_string()));
                }
            }
            NodeOnlineStatus::Offline => {
                offline_nodes += 1;
                worst_culprit = Some((node.hostname.clone(), "失联离线 (Offline)".to_string()));
            }
        }

        if let Some(t) = node
            .latest_telemetry
            .as_ref()
            .filter(|_| !telemetry_stale && node.status != NodeOnlineStatus::Offline)
        {
            if t.validity.cpu && t.cpu_usage_pct > max_cpu {
                max_cpu = t.cpu_usage_pct;
                if t.cpu_usage_pct >= 90.0 {
                    worst_culprit = Some((
                        node.hostname.clone(),
                        format!("CPU {:.1}%", t.cpu_usage_pct),
                    ));
                }
            }
            let mem_pct = t.memory_usage_pct();
            if t.validity.memory && mem_pct > max_mem {
                max_mem = mem_pct;
                if mem_pct >= 90.0 {
                    worst_culprit = Some((node.hostname.clone(), format!("内存 {:.1}%", mem_pct)));
                }
            }
            let disk_pct = t.disk_usage_pct();
            if t.validity.disk && disk_pct > max_disk {
                max_disk = disk_pct;
                if disk_pct >= 90.0 {
                    worst_culprit = Some((node.hostname.clone(), format!("磁盘 {:.1}%", disk_pct)));
                }
            }
        }
    }

    // Also evaluate traditional SSH hosts if present
    for (host_id, m) in metrics_iter {
        if m.cpu.usage_percent > max_cpu {
            max_cpu = m.cpu.usage_percent;
        }
        if m.mem.usage_percent > max_mem {
            max_mem = m.mem.usage_percent;
        }
        for d in &m.disks {
            if d.usage_percent > max_disk {
                max_disk = d.usage_percent;
            }
        }
        if m.cpu.usage_percent >= 90.0 || m.mem.usage_percent >= 90.0 {
            let name = hosts
                .iter()
                .find(|h| h.id.0 == host_id)
                .map(|h| h.name.clone())
                .unwrap_or_else(|| host_id.to_string());
            worst_culprit = Some((name, "负载严重超标".to_string()));
        }
    }

    let overall_level =
        if offline_nodes > 0 || max_cpu >= 90.0 || max_mem >= 90.0 || max_disk >= 95.0 {
            MeterLevel::Critical
        } else if stale_nodes > 0 || max_cpu >= 75.0 || max_mem >= 80.0 || max_disk >= 85.0 {
            MeterLevel::Warning
        } else {
            MeterLevel::Normal
        };

    let led_color_hex = match overall_level {
        MeterLevel::Normal => "#10b981",   // Emerald green
        MeterLevel::Warning => "#f59e0b",  // Amber warning
        MeterLevel::Critical => "#ef4444", // Rose red alert
    };

    let (summary_label, status_tooltip) = match overall_level {
        MeterLevel::Normal => {
            let label = if total_nodes > 0 {
                format!("{} 节点全部正常", total_nodes)
            } else {
                "无受控节点".to_string()
            };
            let tip = format!(
                "全网正常 (最高 CPU: {:.0}%, 内存: {:.0}%)",
                max_cpu, max_mem
            );
            (label, tip)
        }
        MeterLevel::Warning => {
            let culprit = worst_culprit
                .as_ref()
                .map(|(n, r)| format!("{} ({})", n, r))
                .unwrap_or_else(|| "负载偏高".to_string());
            (
                format!("⚠️ 警告: {}", culprit),
                format!(
                    "存在异常波动 - 最高 CPU: {:.0}%, 内存: {:.0}%",
                    max_cpu, max_mem
                ),
            )
        }
        MeterLevel::Critical => {
            let culprit = worst_culprit
                .as_ref()
                .map(|(n, r)| format!("{} [{}]", n, r))
                .unwrap_or_else(|| "离线或重度过载".to_string());
            (
                format!("🚨 告警: {}", culprit),
                format!(
                    "严重告警: {} 台离线, 最高负载: {:.0}%",
                    offline_nodes,
                    max_cpu.max(max_mem)
                ),
            )
        }
    };

    AmbientFleetSummary {
        total_nodes,
        online_nodes,
        stale_nodes,
        offline_nodes,
        max_cpu_pct: max_cpu,
        max_mem_pct: max_mem,
        max_disk_pct: max_disk,
        overall_level,
        led_color_hex,
        summary_label,
        status_tooltip,
        worst_culprit_name: worst_culprit.map(|(n, _)| n),
    }
}

impl crate::state::AppStateMachine {
    /// Computes an instant, zero-allocation ambient health summary across all monitored nodes.
    pub fn evaluate_ambient_summary(&self) -> AmbientFleetSummary {
        evaluate_fleet_ambient_summary(
            &self.control_plane_nodes,
            &self.hosts,
            self.metrics.iter().map(|(k, v)| (k.as_str(), v)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redash_types::AgentTelemetry;

    #[test]
    fn test_ambient_fleet_summary_healthy() {
        let mut state = crate::state::AppStateMachine::new();
        let telemetry = AgentTelemetry {
            validity: redash_types::TelemetryValidity {
                cpu: true,
                memory: true,
                disk: true,
                network: true,
            },
            node_id: "n-1".to_string(),
            hostname: "nas".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            timestamp: crate::e2ee::now_secs(),
            uptime_secs: 1000,
            cpu_usage_pct: 15.0,
            cpu_cores: 4,
            mem_used_bytes: 1024 * 1024 * 200,
            mem_total_bytes: 1024 * 1024 * 1000,
            disk_used_bytes: 1024 * 1024 * 10,
            disk_total_bytes: 1024 * 1024 * 100,
            net_rx_rate: 10,
            net_tx_rate: 20,
            containers: vec![],
        };

        state
            .control_plane_nodes
            .push(redash_types::ManagedNodeDetail {
                telemetry_expires_at: crate::e2ee::now_secs() + 30,
                node_id: "n-1".to_string(),
                hostname: "nas".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                version: "0.2.1-beta".to_string(),
                remote_ip: "192.168.1.1".to_string(),
                status: NodeOnlineStatus::Online,
                connected_at: 1710000000,
                last_heartbeat_at: 1710000000,
                latest_telemetry: Some(telemetry),
            });

        let summary = state.evaluate_ambient_summary();
        assert!(summary.is_healthy());
        assert_eq!(summary.overall_level, MeterLevel::Normal);
        assert_eq!(summary.led_color_hex, "#10b981");
        assert!(summary.summary_label.contains("全部正常"));
    }

    #[test]
    fn online_but_expired_collector_is_warning_even_after_sample_is_hidden() {
        let node = ManagedNodeDetail {
            telemetry_expires_at: crate::e2ee::now_secs() - 1,
            node_id: "stale".into(),
            hostname: "collector".into(),
            os: "test".into(),
            arch: "test".into(),
            version: "2".into(),
            remote_ip: "local".into(),
            status: NodeOnlineStatus::Online,
            connected_at: 1,
            last_heartbeat_at: crate::e2ee::now_secs(),
            latest_telemetry: None,
        };
        let summary = evaluate_fleet_ambient_summary(&[node], &[], std::iter::empty());
        assert!(!summary.is_healthy());
        assert_eq!(summary.overall_level, MeterLevel::Warning);
        assert_eq!(summary.stale_nodes, 1);
        assert!(summary.summary_label.contains("指标采集已过期"));
    }

    #[test]
    fn test_ambient_fleet_summary_offline_alert() {
        let mut state = crate::state::AppStateMachine::new();
        state
            .control_plane_nodes
            .push(redash_types::ManagedNodeDetail {
                telemetry_expires_at: 0,
                node_id: "n-vps".to_string(),
                hostname: "vps-frankfurt".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                version: "0.2.0-beta".to_string(),
                remote_ip: "1.2.3.4".to_string(),
                status: NodeOnlineStatus::Offline,
                connected_at: 1710000000,
                last_heartbeat_at: 1710000000,
                latest_telemetry: None,
            });

        let summary = state.evaluate_ambient_summary();
        assert!(!summary.is_healthy());
        assert_eq!(summary.overall_level, MeterLevel::Critical);
        assert_eq!(summary.led_color_hex, "#ef4444");
        assert!(summary.summary_label.contains("告警"));
        assert!(summary.summary_label.contains("vps-frankfurt"));
        assert_eq!(
            summary.worst_culprit_name,
            Some("vps-frankfurt".to_string())
        );
    }
}
