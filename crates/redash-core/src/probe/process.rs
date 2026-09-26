
pub use redash_types::metrics::ProcessItem;

pub struct ProcessManager;

impl ProcessManager {
    pub fn list_cmd(sort_by: &str, limit: usize) -> String {
        let (linux_sort, darwin_sort) = if sort_by.eq_ignore_ascii_case("mem") {
            ("--sort=-%mem", "-m")
        } else {
            ("--sort=-%cpu", "-r")
        };

        if limit > 0 {
            format!(
                "(ps -eo pid,user,%cpu,%mem,stat,rss,comm {} 2>/dev/null || ps -eo pid,user,%cpu,%mem,state,rss,comm {} 2>/dev/null) | head -n {}",
                linux_sort,
                darwin_sort,
                limit + 1
            )
        } else {
            format!(
                "ps -eo pid,user,%cpu,%mem,stat,rss,comm {} 2>/dev/null || ps -eo pid,user,%cpu,%mem,state,rss,comm {} 2>/dev/null",
                linux_sort, darwin_sort
            )
        }
    }

    pub fn kill_cmd(pid: u32, force: bool) -> String {
        if force {
            format!("kill -9 {}", pid)
        } else {
            format!("kill -15 {}", pid)
        }
    }

    pub fn parse_processes(output: &str) -> Vec<ProcessItem> {
        let mut processes = Vec::new();

        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }

            // Skip header if line starts with PID
            let pid = match parts[0].parse::<u32>() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let user = parts[1].to_string();
            let cpu_percent = parts[2].parse::<f32>().unwrap_or(0.0);
            let mem_percent = parts[3].parse::<f32>().unwrap_or(0.0);
            let status = parts[4].to_string();
            let rss_kb = parts[5].parse::<u64>().unwrap_or(0);
            let rss_bytes = rss_kb.saturating_mul(1024);
            let command = if parts.len() > 6 {
                parts[6..].join(" ")
            } else {
                String::new()
            };

            processes.push(ProcessItem {
                pid,
                user,
                cpu_percent,
                mem_percent,
                status,
                rss_bytes,
                command,
            });
        }

        processes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_processes() {
        let sample = "\
  PID USER     %CPU %MEM STAT   RSS COMMAND
    1 root      0.1  0.2 Ss    4096 /sbin/init splash
  101 www-data 12.5  3.4 S    32768 nginx: worker process
  205 leeqj     2.0  1.1 R+    8192 ps -eo pid,user,%cpu,%mem,stat,rss,comm
";

        let procs = ProcessManager::parse_processes(sample);
        assert_eq!(procs.len(), 3);

        assert_eq!(procs[0].pid, 1);
        assert_eq!(procs[0].user, "root");
        assert!((procs[0].cpu_percent - 0.1).abs() < 1e-4);
        assert!((procs[0].mem_percent - 0.2).abs() < 1e-4);
        assert_eq!(procs[0].status, "Ss");
        assert_eq!(procs[0].rss_bytes, 4096 * 1024);
        assert_eq!(procs[0].command, "/sbin/init splash");

        assert_eq!(procs[1].pid, 101);
        assert_eq!(procs[1].user, "www-data");
        assert_eq!(procs[1].rss_bytes, 32768 * 1024);
        assert_eq!(procs[1].command, "nginx: worker process");

        assert_eq!(procs[2].pid, 205);
        assert_eq!(procs[2].status, "R+");
    }

    #[test]
    fn test_list_and_kill_commands() {
        let cmd_cpu = ProcessManager::list_cmd("cpu", 10);
        assert!(cmd_cpu.contains("--sort=-%cpu"));
        assert!(cmd_cpu.contains("-r"));
        assert!(cmd_cpu.contains("head -n 11"));

        let cmd_mem = ProcessManager::list_cmd("mem", 0);
        assert!(cmd_mem.contains("--sort=-%mem"));
        assert!(cmd_mem.contains("-m"));
        assert!(!cmd_mem.contains("head"));

        assert_eq!(ProcessManager::kill_cmd(1234, false), "kill -15 1234");
        assert_eq!(ProcessManager::kill_cmd(1234, true), "kill -9 1234");
    }
}
