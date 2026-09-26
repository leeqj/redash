use crate::config::HostId;
use crate::probe::metrics::*;
use anyhow::Result;

pub struct LinuxProbe;

impl LinuxProbe {
    pub fn command() -> &'static str {
        r#"
export LC_ALL=C
echo "===CPU_BEFORE==="
head -n 1 /proc/stat
sleep 0.2
echo "===CPU_AFTER==="
head -n 1 /proc/stat
echo "===UPTIME==="
cat /proc/uptime 2>/dev/null
echo "===NPROC==="
nproc 2>/dev/null || grep -c ^processor /proc/cpuinfo 2>/dev/null || echo 1
echo "===LOAD==="
cat /proc/loadavg 2>/dev/null
echo "===MEM==="
cat /proc/meminfo 2>/dev/null | head -n 10
echo "===DF==="
df -k -P / 2>/dev/null | tail -n +2
echo "===NET==="
cat /proc/net/dev 2>/dev/null | tail -n +3
echo "===TOP==="
ps -eo pid,user,%cpu,%mem,comm --sort=-%cpu 2>/dev/null | head -n 11 | tail -n +2
echo "===DOCKER==="
if command -v docker >/dev/null 2>&1; then
    docker ps --format "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.State}}" 2>/dev/null | head -n 10
fi
"#
    }

    pub fn parse(host_id: &HostId, output: &str) -> Result<NodeMetrics> {
        let mut metrics = NodeMetrics {
            host_id: host_id.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ..Default::default()
        };

        let mut cpu_before = None;
        let mut cpu_after = None;
        let sections: Vec<&str> = output.split("===").collect();
        let mut i = 1;
        while i < sections.len() {
            let section_name = sections[i].trim();
            let section_body = if i + 1 < sections.len() {
                sections[i + 1]
            } else {
                ""
            };
            i += 2;

            match section_name {
                "CPU_BEFORE" => cpu_before = cpu_counters(section_body),
                "CPU_AFTER" => cpu_after = cpu_counters(section_body),
                "UPTIME" => {
                    if let Some(first_word) = section_body.split_whitespace().next()
                        && let Ok(secs) = first_word.parse::<f64>()
                    {
                        metrics.uptime_secs = secs as u64;
                    }
                }
                "LOAD" => {
                    let parts: Vec<&str> = section_body.split_whitespace().collect();
                    if parts.len() >= 3 {
                        metrics.cpu.load_1 = parts[0].parse().unwrap_or(0.0);
                        metrics.cpu.load_5 = parts[1].parse().unwrap_or(0.0);
                        metrics.cpu.load_15 = parts[2].parse().unwrap_or(0.0);
                    }
                }
                "NPROC" => {
                    if let Some(word) = section_body.split_whitespace().next()
                        && let Ok(cores) = word.parse::<u32>()
                        && cores > 0
                    {
                        metrics.cpu.cores = cores;
                    }
                }
                "MEM" => {
                    let mut total_kb = 0u64;
                    let mut avail_kb = 0u64;
                    for line in section_body.lines() {
                        if line.starts_with("MemTotal:") {
                            total_kb = line
                                .split_whitespace()
                                .nth(1)
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                        } else if line.starts_with("MemAvailable:") {
                            avail_kb = line
                                .split_whitespace()
                                .nth(1)
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                        }
                    }
                    if total_kb > 0 {
                        let used_kb = total_kb.saturating_sub(avail_kb);
                        metrics.mem.total_bytes = total_kb * 1024;
                        metrics.mem.used_bytes = used_kb * 1024;
                        metrics.mem.free_bytes = avail_kb * 1024;
                        metrics.mem.usage_percent = (used_kb as f32 / total_kb as f32) * 100.0;
                    }
                }
                "DF" => {
                    for line in section_body.lines() {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 6 {
                            let total_kb: u64 = parts[1].parse().unwrap_or(0);
                            let used_kb: u64 = parts[2].parse().unwrap_or(0);
                            let avail_kb: u64 = parts[3].parse().unwrap_or(0);
                            let mount = parts[5].to_string();
                            let usage_pct = if total_kb > 0 {
                                (used_kb as f32 / total_kb as f32) * 100.0
                            } else {
                                0.0
                            };
                            metrics.disks.push(DiskMetrics {
                                mount_point: mount,
                                total_bytes: total_kb * 1024,
                                used_bytes: used_kb * 1024,
                                free_bytes: avail_kb * 1024,
                                usage_percent: usage_pct,
                            });
                        }
                    }
                }
                "NET" => {
                    for line in section_body.lines() {
                        if let Some((iface, stats)) = line.split_once(':') {
                            if iface.trim() == "lo" {
                                continue;
                            }
                            let parts: Vec<&str> = stats.split_whitespace().collect();
                            if parts.len() >= 9
                                && let (Ok(rx), Ok(tx)) =
                                    (parts[0].parse::<u64>(), parts[8].parse::<u64>())
                            {
                                metrics.net_available = true;
                                metrics.net.total_rx_bytes += rx;
                                metrics.net.total_tx_bytes += tx;
                            }
                        }
                    }
                }
                "TOP" => {
                    for line in section_body.lines() {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 5 {
                            let pid: u32 = parts[0].parse().unwrap_or(0);
                            let user = parts[1].to_string();
                            let cpu_percent: f32 = parts[2].parse().unwrap_or(0.0);
                            let mem_percent: f32 = parts[3].parse().unwrap_or(0.0);
                            let command = parts[4..].join(" ");
                            metrics.top_processes.push(ProcessInfo {
                                pid,
                                user,
                                cpu_percent,
                                mem_percent,
                                command,
                            });
                        }
                    }
                }
                "DOCKER" => {
                    for line in section_body.lines() {
                        let parts: Vec<&str> = line.split('\t').collect();
                        if parts.len() >= 5 {
                            metrics.containers.push(DockerContainerInfo {
                                id: parts[0].to_string(),
                                name: parts[1].to_string(),
                                image: parts[2].to_string(),
                                status: parts[3].to_string(),
                                state: parts[4].to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        metrics.cpu.usage_percent = cpu_usage(cpu_before, cpu_after)
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid CPU counter samples"))?;
        anyhow::ensure!(
            metrics.mem.total_bytes > 0 && metrics.cpu.cores > 0,
            "Missing or invalid system metrics"
        );
        Ok(metrics)
    }
}

fn cpu_counters(sample: &str) -> Option<(u64, u64)> {
    let values = sample
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() < 4 {
        return None;
    }
    Some((
        values.iter().sum(),
        values[3] + values.get(4).copied().unwrap_or(0),
    ))
}
fn cpu_usage(before: Option<(u64, u64)>, after: Option<(u64, u64)>) -> Option<f32> {
    let (total, idle) = before?;
    let (next_total, next_idle) = after?;
    let delta = next_total.checked_sub(total)?;
    let idle_delta = next_idle.checked_sub(idle)?;
    (delta > 0 && idle_delta <= delta).then(|| 100.0 * (delta - idle_delta) as f32 / delta as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_reset_samples_are_not_zero_cpu() {
        assert!(LinuxProbe::parse(&HostId::new(), "").is_err());
        assert_eq!(cpu_usage(Some((100, 80)), Some((90, 70))), None);
        assert_eq!(cpu_usage(Some((100, 80)), Some((100, 80))), None);
    }
    #[test]
    fn test_linux_probe_parsing() {
        let sample_output = r#"
===CPU_BEFORE===
cpu 10 0 10 80 0 0 0 0 0 0
===CPU_AFTER===
cpu 20 0 20 160 0 0 0 0 0 0
===UPTIME===
123456.78 987654.32
===LOAD===
0.75 0.50 0.25 1/450 12345
===NPROC===
4
===MEM===
MemTotal:        16384000 kB
MemFree:          4096000 kB
MemAvailable:     8192000 kB
Buffers:           512000 kB
Cached:           3584000 kB
SwapTotal:        2048000 kB
SwapFree:         1024000 kB
===DF===
/dev/sda1        104857600 41943040  62914560  40% /
===NET===
  eth0: 104857600     1000    0    0    0     0          0         0 52428800      800    0    0    0     0       0          0
===TOP===
  101 root       15.5  2.0 nginx: master process
  102 www-data   10.2  4.5 php-fpm: pool www
===DOCKER===
abc12345	my-redis	redis:7-alpine	Up 2 days	running
"#;

        let host_id = HostId("test-node-1".into());
        let metrics = LinuxProbe::parse(&host_id, sample_output).expect("parse should succeed");

        assert_eq!(metrics.uptime_secs, 123456);
        assert!((metrics.cpu.load_1 - 0.75).abs() < 1e-5);
        assert!((metrics.cpu.load_5 - 0.50).abs() < 1e-5);
        assert!((metrics.cpu.load_15 - 0.25).abs() < 1e-5);
        assert_eq!(metrics.cpu.cores, 4);
        assert!((metrics.cpu.usage_percent - 20.0).abs() < 1e-3);
        assert_eq!(metrics.mem.total_bytes, 16384000 * 1024);
        assert_eq!(metrics.net.total_rx_bytes, 104857600);
        assert_eq!(metrics.net.total_tx_bytes, 52428800);
        assert_eq!(metrics.disks.len(), 1);
        assert_eq!(metrics.disks[0].mount_point, "/");
        assert_eq!(metrics.disks[0].usage_percent, 40.0);
        assert_eq!(metrics.top_processes.len(), 2);
        assert_eq!(metrics.top_processes[0].pid, 101);
        assert_eq!(metrics.top_processes[0].command, "nginx: master process");
        assert_eq!(metrics.containers.len(), 1);
        assert_eq!(metrics.containers[0].name, "my-redis");
        assert_eq!(metrics.containers[0].state, "running");
    }
}
