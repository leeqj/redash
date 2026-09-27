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

    /// Tier 1: Lightweight background monitoring for Fleet overview and Alerts.
    ///
    /// Executes a single, minimal command querying ONLY lightweight kernel virtual file
    /// interfaces (/proc/stat, /proc/meminfo, /proc/loadavg, /proc/uptime, /proc/net/dev, df -k /)
    /// plus a single ping RTT measurement over 1 SSH channel.
    ///
    /// Completely avoids high-overhead process scans (`ps`), container statistics (`docker stats`),
    /// container listings (`docker ps`), and network port scans (`ss`).
    pub async fn poll_host_fleet(
        &self,
        host: &HostConfig,
        settings: &AppSettings,
    ) -> anyhow::Result<NodeMetrics> {
        let (cmd, darwin) = match host.target_os {
            TargetOs::Darwin => (DarwinProbe::fleet_command(), true),
            TargetOs::Linux => (LinuxProbe::fleet_command(), false),
            _ => anyhow::bail!(
                "当前平台不支持自动指标采集，请选择 Linux 或 macOS；终端和 SFTP 可独立使用"
            ),
        };
        let duration = Duration::from_secs(settings.probe_timeout_secs.clamp(1, 120));
        let (basic, ping) = tokio::join!(
            self.session_mgr.exec(host, cmd, duration),
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

        // Tier 1 explicitly leaves heavy panels as not queried
        metrics.listening_ports_available = false;
        metrics.containers_available = false;
        metrics.processes_available = false;

        self.record_history(host, &mut metrics, settings).await;
        Ok(metrics)
    }

    /// Tier 2 on-demand probe: Query Docker containers and live stats for a specific host.
    pub async fn poll_host_containers(
        &self,
        host: &HostConfig,
        timeout: Duration,
    ) -> anyhow::Result<Vec<DockerContainerDetail>> {
        let (list_res, stats_res) = tokio::join!(
            self.session_mgr
                .exec(host, DockerManager::list_containers_cmd(), timeout),
            self.session_mgr
                .exec(host, DockerManager::stats_cmd(), timeout),
        );
        let list = list_res?;
        let stats = stats_res?;
        anyhow::ensure!(
            list.exit_code == 0,
            "Docker ps failed ({}): {}",
            list.exit_code,
            list.stderr
        );
        anyhow::ensure!(
            stats.exit_code == 0,
            "Docker stats failed ({}): {}",
            stats.exit_code,
            stats.stderr
        );
        Ok(DockerManager::parse_containers(&list.stdout, &stats.stdout))
    }

    /// Tier 2 on-demand probe: Query process table for a specific host.
    pub async fn poll_host_processes(
        &self,
        host: &HostConfig,
        sort_by: &str,
        limit: usize,
        timeout: Duration,
    ) -> anyhow::Result<Vec<ProcessItem>> {
        let cmd = ProcessManager::list_cmd(sort_by, limit);
        let res = self.session_mgr.exec(host, &cmd, timeout).await?;
        anyhow::ensure!(
            res.exit_code == 0,
            "Process query failed ({}): {}",
            res.exit_code,
            res.stderr
        );
        Ok(ProcessManager::parse_processes(&res.stdout))
    }

    /// Tier 2 on-demand probe: Query listening ports for a specific host.
    pub async fn poll_host_ports(
        &self,
        host: &HostConfig,
        timeout: Duration,
    ) -> anyhow::Result<Vec<ListeningPort>> {
        let cmd = NetworkDiagnostics::listening_ports_cmd();
        let res = self.session_mgr.exec(host, cmd, timeout).await?;
        anyhow::ensure!(
            res.exit_code == 0,
            "Listening ports query failed ({}): {}",
            res.exit_code,
            res.stderr
        );
        Ok(NetworkDiagnostics::parse_listening_ports(&res.stdout))
    }

    /// Default polling method for general callers (defaults to lightweight Tier 1 fleet polling).
    pub async fn poll_host(&self, host: &HostConfig) -> anyhow::Result<NodeMetrics> {
        self.poll_host_fleet(host, &AppSettings::default()).await
    }

    /// Tiered full poll (executes basic metrics + ports + processes + containers + stats + ping in parallel).
    pub async fn poll_host_full(
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
        self.record_history(host, &mut metrics, settings).await;
        Ok(metrics)
    }

    pub async fn poll_host_with_settings(
        &self,
        host: &HostConfig,
        settings: &AppSettings,
    ) -> anyhow::Result<NodeMetrics> {
        self.poll_host_full(host, settings).await
    }

    async fn record_history(
        &self,
        host: &HostConfig,
        metrics: &mut NodeMetrics,
        settings: &AppSettings,
    ) {
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

    #[tokio::test]
    async fn test_record_history_calculates_network_rates() {
        let scheduler = ProbeScheduler::new(Arc::new(SessionManager::new()));
        let mut host = HostConfig::new("Test Rates", "127.0.0.1", "root");
        host.id = HostId("test-rates".into());
        let settings = AppSettings::default();

        let mut m1 = NodeMetrics {
            host_id: host.id.clone(),
            uptime_secs: 100,
            net_available: true,
            net: NetMetrics {
                total_rx_bytes: 1000,
                total_tx_bytes: 500,
                ..Default::default()
            },
            ..Default::default()
        };
        scheduler.record_history(&host, &mut m1, &settings).await;

        let history = scheduler.get_history(&host.id).await.unwrap();
        assert_eq!(history.len(), 1);
        assert!(!history[0].net_rates_available);

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut m2 = NodeMetrics {
            host_id: host.id.clone(),
            uptime_secs: 101,
            net_available: true,
            net: NetMetrics {
                total_rx_bytes: 2000,
                total_tx_bytes: 1000,
                ..Default::default()
            },
            ..Default::default()
        };
        scheduler.record_history(&host, &mut m2, &settings).await;

        let latest = scheduler.get_latest(&host.id).await.unwrap();
        assert!(latest.net_rates_available);
        assert!(latest.net.rx_bytes_per_sec > 0);
        assert!(latest.net.tx_bytes_per_sec > 0);
    }

    #[test]
    fn test_tiered_flags_contract() {
        let m = NodeMetrics {
            containers_available: false,
            processes_available: false,
            listening_ports_available: false,
            ..Default::default()
        };

        assert!(!m.containers_available);
        assert!(!m.processes_available);
        assert!(!m.listening_ports_available);
    }
}
