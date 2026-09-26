
pub use redash_types::metrics::DockerContainerDetail;

#[derive(Debug, Clone, Default)]
struct ContainerStats {
    cpu_percent: f32,
    mem_usage_bytes: u64,
    mem_limit_bytes: u64,
}

pub struct DockerManager;

impl DockerManager {
    pub fn list_containers_cmd() -> &'static str {
        "docker ps -a --no-trunc --format '{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.State}}\t{{.Ports}}\t{{.CreatedAt}}' 2>/dev/null"
    }

    pub fn stats_cmd() -> &'static str {
        "docker stats --no-stream --format '{{.ID}}\t{{.CPUPerc}}\t{{.MemUsage}}' 2>/dev/null"
    }

    pub fn start_cmd(id: &str) -> String {
        format!("docker start {}", id)
    }

    pub fn stop_cmd(id: &str) -> String {
        format!("docker stop {}", id)
    }

    pub fn restart_cmd(id: &str) -> String {
        format!("docker restart {}", id)
    }

    pub fn rm_cmd(id: &str) -> String {
        format!("docker rm {}", id)
    }

    pub fn logs_cmd(id: &str, tail: usize) -> String {
        format!("docker logs --tail {} {} 2>&1", tail, id)
    }

    pub fn parse_containers(output: &str, stats_output: &str) -> Vec<DockerContainerDetail> {
        let stats_map = Self::parse_stats(stats_output);
        let mut containers = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 5 {
                continue;
            }

            let id = parts[0].trim().to_string();
            let name = parts[1].trim().to_string();
            let image = parts[2].trim().to_string();
            let status = parts[3].trim().to_string();
            let state = parts[4].trim().to_string();

            let ports = if parts.len() > 5 {
                parts[5]
                    .split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            } else {
                Vec::new()
            };

            let created = if parts.len() > 6 {
                parts[6].trim().to_string()
            } else {
                String::new()
            };

            // Match stats by exact ID or prefix
            let stat = stats_map
                .iter()
                .find(|(stat_id, _)| {
                    stat_id == &id
                        || id.starts_with(stat_id.as_str())
                        || stat_id.starts_with(id.as_str())
                })
                .map(|(_, s)| s.clone())
                .unwrap_or_default();

            containers.push(DockerContainerDetail {
                id,
                name,
                image,
                status,
                state,
                created,
                ports,
                cpu_percent: stat.cpu_percent,
                mem_usage_bytes: stat.mem_usage_bytes,
                mem_limit_bytes: stat.mem_limit_bytes,
            });
        }

        containers
    }

    fn parse_stats(stats_output: &str) -> Vec<(String, ContainerStats)> {
        let mut list = Vec::new();
        for line in stats_output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 3 {
                continue;
            }

            let id = parts[0].trim().to_string();
            let cpu_str = parts[1].trim().trim_end_matches('%').trim();
            let cpu_percent = cpu_str.parse::<f32>().unwrap_or(0.0);

            let (mem_usage_bytes, mem_limit_bytes) = Self::parse_mem_usage(parts[2]);

            list.push((
                id,
                ContainerStats {
                    cpu_percent,
                    mem_usage_bytes,
                    mem_limit_bytes,
                },
            ));
        }
        list
    }

    fn parse_mem_usage(mem_field: &str) -> (u64, u64) {
        if let Some((usage_str, limit_str)) = mem_field.split_once('/') {
            (
                Self::parse_byte_size(usage_str).unwrap_or(0),
                Self::parse_byte_size(limit_str).unwrap_or(0),
            )
        } else {
            (Self::parse_byte_size(mem_field).unwrap_or(0), 0)
        }
    }

    pub fn parse_byte_size(s: &str) -> Option<u64> {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') {
            return None;
        }

        // Find boundary between number and unit
        let split_idx = trimmed
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
            .unwrap_or(trimmed.len());

        let num_str = trimmed[..split_idx].replace(',', "");
        let num: f64 = num_str.trim().parse().ok()?;
        let unit = trimmed[split_idx..].trim().to_ascii_lowercase();

        let multiplier: f64 = match unit.as_str() {
            "b" | "bytes" | "" => 1.0,
            "kib" => 1024.0,
            "kb" | "k" => 1000.0,
            "mib" => 1024.0 * 1024.0,
            "mb" | "m" => 1000.0 * 1000.0,
            "gib" => 1024.0 * 1024.0 * 1024.0,
            "gb" | "g" => 1000.0 * 1000.0 * 1000.0,
            "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            "tb" | "t" => 1000.0 * 1000.0 * 1000.0 * 1000.0,
            _ => 1.0,
        };

        Some((num * multiplier).round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_byte_size() {
        assert_eq!(DockerManager::parse_byte_size("100B"), Some(100));
        assert_eq!(DockerManager::parse_byte_size("100 B"), Some(100));
        assert_eq!(DockerManager::parse_byte_size("1KiB"), Some(1024));
        assert_eq!(DockerManager::parse_byte_size("1KB"), Some(1000));
        assert_eq!(
            DockerManager::parse_byte_size("12.5MiB"),
            Some((12.5 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(
            DockerManager::parse_byte_size("2GiB"),
            Some(2 * 1024 * 1024 * 1024)
        );
        assert_eq!(DockerManager::parse_byte_size("--"), None);
        assert_eq!(DockerManager::parse_byte_size(""), None);
    }

    #[test]
    fn test_parse_containers_with_stats() {
        let ps_output = "\
c419992f03f3957fba30caeb193f7cbb1156e5429188a8d052d9a65fb016be32\tmy-redis\tredis:7-alpine\tUp 3 hours\trunning\t0.0.0.0:6379->6379/tcp, :::6379->6379/tcp\t2026-09-24 10:00:00 +0000 UTC
a1b2c3d4e5f678901234567890abcdef1234567890abcdef1234567890abcdef\tweb-app\tnginx:latest\tExited (0) 10 minutes ago\texited\t\t2026-09-24 08:00:00 +0000 UTC
";
        let stats_output = "\
c419992f03f3\t0.45%\t12.5MiB / 1.952GiB
";

        let containers = DockerManager::parse_containers(ps_output, stats_output);
        assert_eq!(containers.len(), 2);

        let redis = &containers[0];
        assert_eq!(
            redis.id,
            "c419992f03f3957fba30caeb193f7cbb1156e5429188a8d052d9a65fb016be32"
        );
        assert_eq!(redis.name, "my-redis");
        assert_eq!(redis.image, "redis:7-alpine");
        assert_eq!(redis.status, "Up 3 hours");
        assert_eq!(redis.state, "running");
        assert_eq!(redis.created, "2026-09-24 10:00:00 +0000 UTC");
        assert_eq!(
            redis.ports,
            vec!["0.0.0.0:6379->6379/tcp", ":::6379->6379/tcp"]
        );
        assert!((redis.cpu_percent - 0.45).abs() < 1e-4);
        assert_eq!(redis.mem_usage_bytes, (12.5f64 * 1024.0 * 1024.0) as u64);
        assert_eq!(
            redis.mem_limit_bytes,
            (1.952f64 * 1024.0 * 1024.0 * 1024.0).round() as u64
        );

        let web = &containers[1];
        assert_eq!(web.name, "web-app");
        assert_eq!(web.state, "exited");
        assert!(web.ports.is_empty());
        assert_eq!(web.cpu_percent, 0.0);
        assert_eq!(web.mem_usage_bytes, 0);
    }

    #[test]
    fn test_docker_commands() {
        assert_eq!(DockerManager::start_cmd("c123"), "docker start c123");
        assert_eq!(DockerManager::stop_cmd("c123"), "docker stop c123");
        assert_eq!(DockerManager::restart_cmd("c123"), "docker restart c123");
        assert_eq!(DockerManager::rm_cmd("c123"), "docker rm c123");
        assert_eq!(
            DockerManager::logs_cmd("c123", 50),
            "docker logs --tail 50 c123 2>&1"
        );
    }
}
