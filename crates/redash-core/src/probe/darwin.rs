use crate::config::HostId;
use crate::probe::metrics::*;
use anyhow::Result;

pub struct DarwinProbe;

impl DarwinProbe {
    /// Lightweight Tier 1 probe command for macOS: excludes high-overhead process listing (`ps`).
    pub fn fleet_command() -> &'static str {
        r#"
export LC_ALL=C
echo "===CPU==="
top -l 2 -s 1 -n 0 | grep 'CPU usage' | tail -n 1
echo "===NET==="
netstat -ibn
echo "===BOOT==="
sysctl -n kern.boottime 2>/dev/null
echo "===NPROC==="
sysctl -n hw.ncpu 2>/dev/null || echo 4
echo "===UPTIME==="
uptime 2>/dev/null
echo "===MEM==="
sysctl -n hw.memsize 2>/dev/null
echo "===VMSTAT==="
vm_stat 2>/dev/null
echo "===DF==="
df -k -P / 2>/dev/null | tail -n +2
"#
    }

    pub fn command() -> &'static str {
        r#"
export LC_ALL=C
echo "===CPU==="
top -l 2 -s 1 -n 0 | grep 'CPU usage' | tail -n 1
echo "===NET==="
netstat -ibn
echo "===BOOT==="
sysctl -n kern.boottime 2>/dev/null
echo "===NPROC==="
sysctl -n hw.ncpu 2>/dev/null || echo 4
echo "===UPTIME==="
uptime 2>/dev/null
echo "===MEM==="
sysctl -n hw.memsize 2>/dev/null
echo "===VMSTAT==="
vm_stat 2>/dev/null
echo "===DF==="
df -k -P / 2>/dev/null | tail -n +2
echo "===TOP==="
ps -eo pid,user,%cpu,%mem,comm -r 2>/dev/null | head -n 11 | tail -n +2
"#
    }

    pub fn parse(host_id: &HostId, output: &str) -> Result<NodeMetrics> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut metrics = NodeMetrics {
            host_id: host_id.clone(),
            timestamp: now,
            ..Default::default()
        };

        let mut cpu_valid = false;
        let mut memory_valid = false;
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
                "CPU" => {
                    if let Some(idle) = section_body
                        .split(',')
                        .find(|part| part.contains("idle"))
                        .and_then(|part| part.split_whitespace().next())
                        .and_then(|value| value.trim_end_matches('%').parse::<f32>().ok())
                        && (0.0..=100.0).contains(&idle)
                    {
                        metrics.cpu.usage_percent = 100.0 - idle;
                        cpu_valid = true;
                    }
                }
                "NET" => {
                    let mut interfaces = std::collections::HashSet::new();
                    let mut lines = section_body.lines().filter(|l| !l.trim().is_empty());
                    if let Some(header) = lines.next() {
                        let columns: Vec<_> = header.split_whitespace().collect();
                        if let (Some(rx), Some(tx)) = (
                            columns.iter().position(|c| *c == "Ibytes"),
                            columns.iter().position(|c| *c == "Obytes"),
                        ) {
                            for line in lines {
                                let values: Vec<_> = line.split_whitespace().collect();
                                if values.len() > tx
                                    && !values[0].starts_with("lo")
                                    && values[2].starts_with("<Link#")
                                    && interfaces.insert(values[0])
                                    && let (Ok(rx), Ok(tx)) =
                                        (values[rx].parse::<u64>(), values[tx].parse::<u64>())
                                {
                                    metrics.net.total_rx_bytes += rx;
                                    metrics.net.total_tx_bytes += tx;
                                    metrics.net_available = true;
                                }
                            }
                        }
                    }
                }
                "BOOT" => {
                    // Output format: { sec = 1711234567, usec = 890123 } ...
                    if let Some(sec_idx) = section_body.find("sec = ") {
                        let sub = &section_body[sec_idx + 6..];
                        if let Some(comma_or_space) =
                            sub.find(|c: char| c == ',' || c.is_whitespace())
                            && let Ok(boot_epoch) = sub[..comma_or_space].trim().parse::<u64>()
                            && now >= boot_epoch
                        {
                            metrics.uptime_secs = now - boot_epoch;
                        }
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
                "UPTIME" => {
                    // uptime on macOS: "15:10  up 4 days,  3:12, 3 users, load averages: 2.14 2.50 2.45"
                    if let Some(load_idx) = section_body.find("load averages:") {
                        let load_part = &section_body[load_idx + 14..];
                        let parts: Vec<&str> = load_part.split_whitespace().collect();
                        if parts.len() >= 3 {
                            metrics.cpu.load_1 = parts[0].parse().unwrap_or(0.0);
                            metrics.cpu.load_5 = parts[1].parse().unwrap_or(0.0);
                            metrics.cpu.load_15 = parts[2].parse().unwrap_or(0.0);
                        }
                    }
                }
                "MEM" => {
                    if let Ok(bytes) = section_body.trim().parse::<u64>() {
                        metrics.mem.total_bytes = bytes;
                    }
                }
                "VMSTAT" => {
                    let mut page_size = 4096u64;
                    let mut active_pages = 0u64;
                    let mut wired_pages = 0u64;
                    let mut compressed_pages = 0u64;

                    for line in section_body.lines() {
                        let trimmed = line.trim();
                        if let Some(idx) = trimmed.find("page size of ") {
                            let sub = &trimmed[idx + 13..];
                            if let Some(space_idx) = sub.find(' ')
                                && let Ok(ps) = sub[..space_idx].parse::<u64>()
                            {
                                page_size = ps;
                            }
                        } else if trimmed.starts_with("Pages occupied by compressor:") {
                            compressed_pages = trimmed
                                .split_whitespace()
                                .last()
                                .and_then(|v| v.trim_end_matches('.').parse().ok())
                                .unwrap_or(0);
                        } else if trimmed.starts_with("Pages active:") {
                            memory_valid = true;
                            active_pages = trimmed
                                .split_whitespace()
                                .nth(2)
                                .and_then(|v| v.trim_end_matches('.').parse().ok())
                                .unwrap_or(0);
                        } else if trimmed.starts_with("Pages wired down:") {
                            wired_pages = trimmed
                                .split_whitespace()
                                .nth(3)
                                .and_then(|v| v.trim_end_matches('.').parse().ok())
                                .unwrap_or(0);
                        }
                    }

                    let used_bytes = (active_pages + wired_pages + compressed_pages) * page_size;
                    if metrics.mem.total_bytes > 0 && used_bytes > 0 {
                        metrics.mem.used_bytes = used_bytes;
                        metrics.mem.free_bytes = metrics.mem.total_bytes.saturating_sub(used_bytes);
                        metrics.mem.usage_percent =
                            ((used_bytes as f32 / metrics.mem.total_bytes as f32) * 100.0)
                                .clamp(0.0, 100.0);
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
                _ => {}
            }
        }

        anyhow::ensure!(
            cpu_valid && memory_valid && metrics.mem.total_bytes > 0 && metrics.cpu.cores > 0,
            "Missing or invalid macOS system metrics"
        );
        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_uses_cpu_samples_and_requires_real_memory_data() {
        let output = "===CPU===\nCPU usage: 10.00% user, 5.00% sys, 85.00% idle\n===NPROC===\n8\n===MEM===\n1048576\n===VMSTAT===\nMach Virtual Memory Statistics: (page size of 4096 bytes)\nPages active: 10.\nPages wired down: 20.\nPages occupied by compressor: 5.\n";
        let metrics = DarwinProbe::parse(&HostId::new(), output).unwrap();
        assert_eq!(metrics.cpu.usage_percent, 15.0);
        assert_eq!(metrics.mem.used_bytes, 35 * 4096);
        assert!(!metrics.net_available);
        assert!(DarwinProbe::parse(&HostId::new(), "===MEM===\n1048576").is_err());
    }

    #[test]
    fn test_darwin_probe_fleet_command_parsing() {
        let fleet_cmd = DarwinProbe::fleet_command();
        assert!(fleet_cmd.contains("vm_stat"));
        assert!(fleet_cmd.contains("sysctl -n hw.memsize"));
        assert!(fleet_cmd.contains("top -l 2 -s 1 -n 0"));
        assert!(!fleet_cmd.contains("ps -eo"));

        let fleet_output = r#"
===CPU===
CPU usage: 12.50% user, 7.50% sys, 80.00% idle
===NET===
Name  Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll
en0   1500  <Link#4>      00:11:22:33:44:55  10000     0   10485760     8000     0    5242880     0
===BOOT===
{ sec = 1700000000, usec = 0 }
===NPROC===
10
===UPTIME===
12:00  up 10 days,  2:30, 3 users, load averages: 1.50 1.20 0.90
===MEM===
17179869184
===VMSTAT===
Mach Virtual Memory Statistics: (page size of 4096 bytes)
Pages free:                              100000.
Pages active:                            500000.
Pages inactive:                          300000.
Pages speculative:                        50000.
Pages throttled:                              0.
Pages wired down:                        200000.
Pages purgeable:                          10000.
"Pages purgeable and non-volatile":           0.
File-backed pages:                       150000.
"Anonymous pages":                       650000.
Pages used for internal operations:           0.
Pages occupied by compressor:            100000.
===DF===
/dev/disk1s1s1 488245288 20000000 200000000 10% /
"#;
        let host_id = HostId("fleet-mac-node".into());
        let metrics =
            DarwinProbe::parse(&host_id, fleet_output).expect("darwin fleet parse should succeed");

        assert!((metrics.cpu.usage_percent - 20.0).abs() < 1e-3);
        assert_eq!(metrics.cpu.cores, 10);
        assert_eq!(metrics.mem.total_bytes, 17179869184);
        assert!(metrics.mem.used_bytes > 0);
        assert!(metrics.net_available);
        assert_eq!(metrics.net.total_rx_bytes, 10485760);
        assert_eq!(metrics.net.total_tx_bytes, 5242880);
        assert!(metrics.top_processes.is_empty());
    }
}
