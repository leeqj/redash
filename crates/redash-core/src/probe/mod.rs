pub mod agent;
pub mod darwin;
pub mod docker;
pub mod linux;
pub mod metrics;
pub mod network;
pub mod process;

pub use agent::*;
pub use darwin::DarwinProbe;
pub use docker::*;
pub use linux::LinuxProbe;
pub use metrics::*;
pub use network::*;
pub use process::*;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::config::{AppSettings, HostConfig, HostId, TargetOs};
use crate::session::SessionManager;

const MAX_HISTORY_POINTS: usize = 60;

#[derive(Debug, Clone, Default)]
pub struct HostHistory {
    pub metrics_history: VecDeque<NodeMetrics>,
    sampled_at: Option<std::time::Instant>,
    connection: Option<HostConfig>,
}

impl HostHistory {
    pub fn push(&mut self, metrics: NodeMetrics) {
        if self.metrics_history.len() >= MAX_HISTORY_POINTS {
            self.metrics_history.pop_front();
        }
        self.metrics_history.push_back(metrics);
    }

    pub fn latest(&self) -> Option<&NodeMetrics> {
        self.metrics_history.back()
    }
}

pub struct ProbeScheduler {
    session_mgr: Arc<SessionManager>,
    histories: Arc<RwLock<HashMap<HostId, HostHistory>>>,
}

impl ProbeScheduler {
    pub fn new(session_mgr: Arc<SessionManager>) -> Self {
        Self {
            session_mgr,
            histories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn poll_host(&self, host: &HostConfig) -> anyhow::Result<NodeMetrics> {
        self.poll_host_with_settings(host, &AppSettings::default())
            .await
    }

    pub async fn poll_host_with_settings(
        &self,
        host: &HostConfig,
        settings: &AppSettings,
    ) -> anyhow::Result<NodeMetrics> {
        let (cmd, darwin) = match host.target_os {
            TargetOs::Darwin => (DarwinProbe::command(), true),
            TargetOs::Linux => (LinuxProbe::command(), false),
            _ => anyhow::bail!(
                "当前平台不支持自动指标采集，请选择 Linux 或 macOS；终端和 SFTP 可独立使用"
            ),
        };
        let duration = Duration::from_secs(settings.probe_timeout_secs.clamp(1, 120));
        let process_cmd = ProcessManager::list_cmd("cpu", 0);
        let ports_cmd = if darwin {
            "false"
        } else {
            NetworkDiagnostics::listening_ports_cmd()
        };
        let (basic, ports, processes, containers, stats, ping) = tokio::join!(
            self.session_mgr.exec(host, cmd, duration),
            self.session_mgr.exec(host, ports_cmd, duration),
            self.session_mgr.exec(host, &process_cmd, duration),
            self.session_mgr
                .exec(host, DockerManager::list_containers_cmd(), duration),
            self.session_mgr
                .exec(host, DockerManager::stats_cmd(), duration),
            tokio::time::timeout(duration, self.session_mgr.ping_host(host)),
        );
        let basic = basic?;
        anyhow::ensure!(
            basic.exit_code == 0,
            "Probe command failed ({}): {}",
            basic.exit_code,
            basic.stderr
        );
        let mut metrics = if darwin {
            DarwinProbe::parse(&host.id, &basic.stdout)?
        } else {
            LinuxProbe::parse(&host.id, &basic.stdout)?
        };
        metrics.rtt_ms = ping
            .ok()
            .and_then(Result::ok)
            .map(|elapsed| elapsed.as_millis().min(u32::MAX as u128) as u32);
        match ports {
            Ok(result) if result.exit_code == 0 => {
                metrics.listening_ports = NetworkDiagnostics::parse_listening_ports(&result.stdout);
                metrics.listening_ports_available = true;
            }
            _ => metrics
                .collection_errors
                .push("监听端口未采集（工具不可用或权限不足）".into()),
        }
        match processes {
            Ok(result) if result.exit_code == 0 => {
                metrics.processes_detail = ProcessManager::parse_processes(&result.stdout)
                    .into_iter()
                    .take(200)
                    .collect();
                metrics.processes_available = true;
            }
            _ => metrics.collection_errors.push("进程信息未采集".into()),
        }
        match (containers, stats) {
            (Ok(list), Ok(stats)) if list.exit_code == 0 && stats.exit_code == 0 => {
                metrics.containers_detail =
                    DockerManager::parse_containers(&list.stdout, &stats.stdout);
                metrics.containers_available = true;
            }
            _ => metrics
                .collection_errors
                .push("容器信息未采集（Docker 不可用或权限不足）".into()),
        }
        let now = std::time::Instant::now();
        let mut histories = self.histories.write().await;
        let history = histories.entry(host.id.clone()).or_default();
        if history
            .connection
            .as_ref()
            .is_some_and(|old| !old.same_connection(host))
        {
            *history = HostHistory::default();
        }
        if let (Some(previous), Some(sampled_at)) = (history.latest(), history.sampled_at)
            && previous.net_available
            && metrics.net_available
            && metrics.uptime_secs >= previous.uptime_secs
        {
            let elapsed = now.duration_since(sampled_at).as_secs_f64();
            if let (Some(rx), Some(tx)) = (
                metrics
                    .net
                    .total_rx_bytes
                    .checked_sub(previous.net.total_rx_bytes),
                metrics
                    .net
                    .total_tx_bytes
                    .checked_sub(previous.net.total_tx_bytes),
            ) && elapsed > 0.0
            {
                metrics.net.rx_bytes_per_sec = (rx as f64 / elapsed) as u64;
                metrics.net.tx_bytes_per_sec = (tx as f64 / elapsed) as u64;
                metrics.net_rates_available = true;
            }
        }
        history.sampled_at = Some(now);
        history.connection = Some(host.clone());
        history.metrics_history.push_back(metrics.clone());
        let limit = settings.history_points.clamp(1, 3600);
        while history.metrics_history.len() > limit {
            history.metrics_history.pop_front();
        }
        Ok(metrics)
    }

    pub async fn forget_hosts(&self, ids: &[HostId]) {
        let mut histories = self.histories.write().await;
        for id in ids {
            histories.remove(id);
        }
    }

    pub async fn get_latest(&self, host_id: &HostId) -> Option<NodeMetrics> {
        let map = self.histories.read().await;
        map.get(host_id).and_then(|h| h.latest()).cloned()
    }

    pub async fn get_history(&self, host_id: &HostId) -> Option<Vec<NodeMetrics>> {
        let map = self.histories.read().await;
        map.get(host_id)
            .map(|h| h.metrics_history.iter().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_history_ring_buffer() {
        let mut history = HostHistory::default();
        let host_id = HostId("test-node".into());

        for i in 0..70 {
            let m = NodeMetrics {
                host_id: host_id.clone(),
                uptime_secs: i,
                ..Default::default()
            };
            history.push(m);
        }

        assert_eq!(history.metrics_history.len(), MAX_HISTORY_POINTS);
        assert_eq!(history.metrics_history.front().unwrap().uptime_secs, 10);
        assert_eq!(history.metrics_history.back().unwrap().uptime_secs, 69);
        assert_eq!(history.latest().unwrap().uptime_secs, 69);
    }
}
