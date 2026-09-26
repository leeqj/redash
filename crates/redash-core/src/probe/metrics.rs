use crate::config::HostId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuMetrics {
    pub usage_percent: f32,
    pub cores: u32,
    pub load_1: f32,
    pub load_5: f32,
    pub load_15: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiskMetrics {
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetMetrics {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockerContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeMetrics {
    #[serde(default)]
    pub net_available: bool,
    #[serde(default)]
    pub net_rates_available: bool,
    #[serde(default)]
    pub listening_ports_available: bool,
    #[serde(default)]
    pub containers_available: bool,
    #[serde(default)]
    pub processes_available: bool,
    #[serde(default)]
    pub collection_errors: Vec<String>,
    pub host_id: HostId,
    pub timestamp: u64,
    pub uptime_secs: u64,
    pub cpu: CpuMetrics,
    pub mem: MemMetrics,
    pub disks: Vec<DiskMetrics>,
    pub net: NetMetrics,
    pub top_processes: Vec<ProcessInfo>,
    pub containers: Vec<DockerContainerInfo>,
    #[serde(default)]
    pub rtt_ms: Option<u32>,
    #[serde(default)]
    pub listening_ports: Vec<crate::probe::network::ListeningPort>,
    #[serde(default)]
    pub containers_detail: Vec<crate::probe::docker::DockerContainerDetail>,
    #[serde(default)]
    pub processes_detail: Vec<crate::probe::process::ProcessItem>,
}
