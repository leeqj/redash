pub mod docker;
pub mod sys;

use docker::DockerClient;
use redash_types::AgentTelemetry;
use sys::SystemCollector;

pub struct TelemetryCollector {
    node_id: String,
    hostname: String,
    sys_collector: SystemCollector,
    docker_client: DockerClient,
}

impl TelemetryCollector {
    pub fn new(node_id: String) -> Self {
        let hostname = gethostname::gethostname().to_string_lossy().into_owned();

        Self {
            node_id,
            hostname,
            sys_collector: SystemCollector::new(),
            docker_client: DockerClient::new(),
        }
    }

    pub async fn collect(&mut self) -> AgentTelemetry {
        let sys_snapshot = self.sys_collector.sample();
        let containers = self.docker_client.list_containers().await;

        AgentTelemetry {
            node_id: self.node_id.clone(),
            hostname: self.hostname.clone(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            timestamp: sys_snapshot.timestamp,
            uptime_secs: sys_snapshot.uptime_secs,
            cpu_usage_pct: sys_snapshot.cpu_usage_pct,
            cpu_cores: sys_snapshot.cpu_cores,
            mem_used_bytes: sys_snapshot.mem_used_bytes,
            mem_total_bytes: sys_snapshot.mem_total_bytes,
            disk_used_bytes: sys_snapshot.disk_used_bytes,
            disk_total_bytes: sys_snapshot.disk_total_bytes,
            net_rx_rate: sys_snapshot.net_rx_rate,
            net_tx_rate: sys_snapshot.net_tx_rate,
            containers,
        }
    }

    pub fn docker(&self) -> &DockerClient {
        &self.docker_client
    }
}
