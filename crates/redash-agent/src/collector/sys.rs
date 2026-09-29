use redash_types::TelemetryValidity;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, Networks, System};

pub struct SystemCollector {
    system: System,
    networks: Networks,
    disks: Disks,
    last_sample: Option<Instant>,
}

pub struct SysMetricsSnapshot {
    pub timestamp: u64,
    pub uptime_secs: u64,
    pub cpu_usage_pct: f32,
    pub cpu_cores: usize,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub net_rx_rate: u64,
    pub net_tx_rate: u64,
    pub validity: TelemetryValidity,
}

impl Default for SystemCollector {
    fn default() -> Self {
        Self::new()
    }
}
impl SystemCollector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            last_sample: None,
        }
    }
    pub fn sample(&mut self) -> SysMetricsSnapshot {
        let now = Instant::now();
        let elapsed = self
            .last_sample
            .map(|t| now.duration_since(t).as_secs_f64());
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh();
        let disk = self
            .disks
            .list_mut()
            .iter_mut()
            .find(|d| d.mount_point() == std::path::Path::new("/"));
        let (disk_used, disk_total, disk_valid) = match disk {
            Some(disk) => {
                if disk.refresh() && disk.total_space() > 0 {
                    (
                        disk.total_space().saturating_sub(disk.available_space()),
                        disk.total_space(),
                        true,
                    )
                } else {
                    (0, 0, false)
                }
            }
            _ => (0, 0, false),
        };
        let (rx, tx) = self
            .networks
            .iter()
            .filter(|(name, _)| !matches!(name.as_str(), "lo" | "lo0"))
            .fold((0u64, 0u64), |(rx, tx), (_, n)| {
                (
                    rx.saturating_add(n.received()),
                    tx.saturating_add(n.transmitted()),
                )
            });
        let rate = |bytes: u64| {
            elapsed
                .filter(|e| *e > 0.0)
                .map(|e| (bytes as f64 / e) as u64)
                .unwrap_or(0)
        };
        let validity = TelemetryValidity {
            cpu: elapsed.is_some_and(|e| e >= sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_secs_f64())
                && !self.system.cpus().is_empty(),
            memory: self.system.total_memory() > 0,
            disk: disk_valid,
            network: elapsed.is_some() && !self.networks.is_empty(),
        };
        self.last_sample = Some(now);
        SysMetricsSnapshot {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            uptime_secs: System::uptime(),
            cpu_usage_pct: self.system.global_cpu_usage(),
            cpu_cores: self.system.cpus().len(),
            mem_used_bytes: self.system.used_memory(),
            mem_total_bytes: self.system.total_memory(),
            disk_used_bytes: disk_used,
            disk_total_bytes: disk_total,
            net_rx_rate: rate(rx),
            net_tx_rate: rate(tx),
            validity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_metrics_have_real_totals_and_warmup_validity() {
        let mut collector = SystemCollector::new();
        let first = collector.sample();
        assert!(!first.validity.cpu && !first.validity.network);
        assert!(first.validity.memory && first.mem_total_bytes > 0);
        assert!(first.mem_used_bytes <= first.mem_total_bytes);
        std::thread::sleep(
            sysinfo::MINIMUM_CPU_UPDATE_INTERVAL + std::time::Duration::from_millis(20),
        );
        let second = collector.sample();
        assert!(second.validity.cpu);
        assert!((0.0..=100.0).contains(&second.cpu_usage_pct));
    }
}
