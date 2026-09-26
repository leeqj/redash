use crate::host::HostId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CpuMetrics {
    pub usage_percent: f32,
    pub cores: u32,
    pub load_1: f32,
    pub load_5: f32,
    pub load_15: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MemMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DiskMetrics {
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct NetMetrics {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DockerContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ListeningPort {
    pub proto: String,
    pub bind_ip: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DockerContainerDetail {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: String,
    pub ports: Vec<String>,
    pub cpu_percent: f32,
    pub mem_usage_bytes: u64,
    pub mem_limit_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProcessItem {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub status: String,
    pub rss_bytes: u64,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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
    pub listening_ports: Vec<ListeningPort>,
    #[serde(default)]
    pub containers_detail: Vec<DockerContainerDetail>,
    #[serde(default)]
    pub processes_detail: Vec<ProcessItem>,
}

impl NodeMetrics {
    pub fn cpu_percent(&self) -> f32 {
        self.cpu.usage_percent
    }

    pub fn mem_percent(&self) -> f32 {
        self.mem.usage_percent
    }
}
