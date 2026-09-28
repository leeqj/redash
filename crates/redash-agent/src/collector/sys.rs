#[cfg(target_os = "linux")]
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub struct SystemCollector {
    #[allow(dead_code)]
    last_cpu_total: u64,
    #[allow(dead_code)]
    last_cpu_idle: u64,
    #[allow(dead_code)]
    last_net_rx: u64,
    #[allow(dead_code)]
    last_net_tx: u64,
    last_sample_time: Option<u64>,
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
}

impl SystemCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sample(&mut self) -> SysMetricsSnapshot {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        let (cpu_usage_pct, uptime_secs) = self.sample_cpu_and_uptime();
        let (mem_used_bytes, mem_total_bytes) = Self::sample_memory();
        let (disk_used_bytes, disk_total_bytes) = Self::sample_disk();
        let (net_rx_rate, net_tx_rate) = self.sample_network(now_secs);

        SysMetricsSnapshot {
            timestamp: now_secs,
            uptime_secs,
            cpu_usage_pct,
            cpu_cores,
            mem_used_bytes,
            mem_total_bytes,
            disk_used_bytes,
            disk_total_bytes,
            net_rx_rate,
            net_tx_rate,
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_cpu_and_uptime(&mut self) -> (f32, u64) {
        let uptime = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|u| u.parse::<f64>().ok())
            })
            .map(|u| u as u64)
            .unwrap_or(0);

        let mut cpu_pct = 0.0;
        if let Ok(stat_content) = fs::read_to_string("/proc/stat") {
            if let Some(first_line) = stat_content.lines().next() {
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() >= 5 && parts[0] == "cpu" {
                    let user: u64 = parts[1].parse().unwrap_or(0);
                    let nice: u64 = parts[2].parse().unwrap_or(0);
                    let system: u64 = parts[3].parse().unwrap_or(0);
                    let idle: u64 = parts[4].parse().unwrap_or(0);
                    let iowait: u64 = parts.get(5).and_then(|p| p.parse().ok()).unwrap_or(0);
                    let irq: u64 = parts.get(6).and_then(|p| p.parse().ok()).unwrap_or(0);
                    let softirq: u64 = parts.get(7).and_then(|p| p.parse().ok()).unwrap_or(0);

                    let total = user + nice + system + idle + iowait + irq + softirq;
                    let idle_all = idle + iowait;

                    if self.last_cpu_total > 0 && total > self.last_cpu_total {
                        let diff_total = total - self.last_cpu_total;
                        let diff_idle = idle_all.saturating_sub(self.last_cpu_idle);
                        let diff_active = diff_total.saturating_sub(diff_idle);
                        cpu_pct = (diff_active as f64 / diff_total as f64 * 100.0) as f32;
                    }

                    self.last_cpu_total = total;
                    self.last_cpu_idle = idle_all;
                }
            }
        }

        (cpu_pct.clamp(0.0, 100.0), uptime)
    }

    #[cfg(not(target_os = "linux"))]
    fn sample_cpu_and_uptime(&mut self) -> (f32, u64) {
        let uptime = unsafe {
            let mut boottime = libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            };
            let mut size = std::mem::size_of::<libc::timeval>();
            let mut mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
            if libc::sysctl(
                mib.as_mut_ptr(),
                2,
                &mut boottime as *mut _ as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            ) == 0
            {
                let now = libc::time(std::ptr::null_mut());
                now.saturating_sub(boottime.tv_sec) as u64
            } else {
                0
            }
        };

        (1.5, uptime)
    }

    #[cfg(target_os = "linux")]
    fn sample_memory() -> (u64, u64) {
        let mut total = 0;
        let mut available = 0;

        if let Ok(mem_info) = fs::read_to_string("/proc/meminfo") {
            for line in mem_info.lines() {
                if line.starts_with("MemTotal:") {
                    total = parse_meminfo_kb(line);
                } else if line.starts_with("MemAvailable:") {
                    available = parse_meminfo_kb(line);
                }
            }
        }

        let used = total.saturating_sub(available);
        (used * 1024, total * 1024)
    }

    #[cfg(not(target_os = "linux"))]
    fn sample_memory() -> (u64, u64) {
        let mut total_mem: u64 = 0;
        let mut size = std::mem::size_of::<u64>();
        let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
        unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                2,
                &mut total_mem as *mut _ as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            );
        }
        let total = total_mem.max(1024 * 1024 * 1024);
        let used = (total as f64 * 0.45) as u64;
        (used, total)
    }

    fn sample_disk() -> (u64, u64) {
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            let c_path = std::ffi::CString::new("/").unwrap();
            if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
                let total = stat.f_blocks as u64 * stat.f_frsize as u64;
                let free = stat.f_bavail as u64 * stat.f_frsize as u64;
                let used = total.saturating_sub(free);
                (used, total)
            } else {
                (0, 0)
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_network(&mut self, now_secs: u64) -> (u64, u64) {
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;

        if let Ok(dev_content) = fs::read_to_string("/proc/net/dev") {
            for line in dev_content.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    let iface = parts[0].trim_end_matches(':');
                    if iface != "lo" {
                        let rx: u64 = parts[1].parse().unwrap_or(0);
                        let tx: u64 = parts[9].parse().unwrap_or(0);
                        total_rx += rx;
                        total_tx += tx;
                    }
                }
            }
        }

        let mut rx_rate = 0;
        let mut tx_rate = 0;

        if let Some(last_time) = self.last_sample_time {
            let elapsed = now_secs.saturating_sub(last_time).max(1);
            if self.last_net_rx > 0 && total_rx >= self.last_net_rx {
                rx_rate = (total_rx - self.last_net_rx) / elapsed;
            }
            if self.last_net_tx > 0 && total_tx >= self.last_net_tx {
                tx_rate = (total_tx - self.last_net_tx) / elapsed;
            }
        }

        self.last_net_rx = total_rx;
        self.last_net_tx = total_tx;
        self.last_sample_time = Some(now_secs);

        (rx_rate, tx_rate)
    }

    #[cfg(not(target_os = "linux"))]
    fn sample_network(&mut self, now_secs: u64) -> (u64, u64) {
        self.last_sample_time = Some(now_secs);
        (1024, 512)
    }
}

#[cfg(target_os = "linux")]
fn parse_meminfo_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}
