pub use redash_types::metrics::ListeningPort;

pub struct NetworkDiagnostics;

impl NetworkDiagnostics {
    pub fn listening_ports_cmd() -> &'static str {
        "ss -tulpn 2>/dev/null || netstat -tulpn 2>/dev/null"
    }

    pub fn measure_rtt_cmd(target: &str) -> String {
        format!(
            "ping -c 1 -W 2 {} 2>/dev/null || ping -c 1 -t 2 {} 2>/dev/null",
            target, target
        )
    }

    pub fn parse_listening_ports(output: &str) -> Vec<ListeningPort> {
        let mut results = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let proto_candidate = parts[0].to_ascii_lowercase();
            if !proto_candidate.starts_with("tcp") && !proto_candidate.starts_with("udp") {
                continue;
            }

            // Detect if this is `ss` or `netstat`
            // `ss` format typically has "LISTEN" or "UNCONN" as column 1
            if parts.len() >= 5 && (parts[1] == "LISTEN" || parts[1] == "UNCONN") {
                // `ss` line: Local Address is parts[4]
                if let Some((bind_ip, port)) = Self::parse_ip_port(parts[4]) {
                    let (pid, process_name) = Self::extract_ss_process(line);
                    results.push(ListeningPort {
                        proto: proto_candidate,
                        bind_ip,
                        port,
                        pid,
                        process_name,
                    });
                }
            } else if parts.len() >= 4 {
                // `netstat` format: Local Address is parts[3]
                if let Some((bind_ip, port)) = Self::parse_ip_port(parts[3]) {
                    let last_token = parts.last().unwrap_or(&"");
                    let (pid, process_name) = Self::extract_netstat_process(last_token);
                    results.push(ListeningPort {
                        proto: proto_candidate,
                        bind_ip,
                        port,
                        pid,
                        process_name,
                    });
                }
            }
        }

        results
    }

    pub fn parse_ip_port(addr: &str) -> Option<(String, u16)> {
        let trimmed = addr.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Bracketed IPv6, e.g. [::]:22 or [fe80::1%lo]:8080
        if trimmed.starts_with('[')
            && let Some(close_bracket) = trimmed.find(']')
        {
            let mut ip = trimmed[1..close_bracket].to_string();
            if let Some(pct) = ip.find('%') {
                ip.truncate(pct);
            }
            let rest = &trimmed[close_bracket + 1..];
            let port_str = rest.strip_prefix(':')?;
            let port = port_str.parse::<u16>().ok()?;
            return Some((ip, port));
        }

        // Colon separated: last colon is port
        if let Some((ip_part, port_str)) = trimmed.rsplit_once(':')
            && let Ok(port) = port_str.parse::<u16>()
        {
            let mut ip = ip_part.to_string();
            if let Some(pct) = ip.find('%') {
                ip.truncate(pct);
            }
            if ip.is_empty() || ip == ":" {
                ip = "::".to_string();
            }
            return Some((ip, port));
        }

        // Dot separated (Darwin netstat), e.g. 127.0.0.1.8080 or *.22
        if let Some((ip_part, port_str)) = trimmed.rsplit_once('.')
            && let Ok(port) = port_str.parse::<u16>()
        {
            let ip = ip_part.to_string();
            return Some((ip, port));
        }

        None
    }

    fn extract_ss_process(line: &str) -> (Option<u32>, Option<String>) {
        if let Some(users_idx) = line.find("users:(") {
            let users_part = &line[users_idx..];
            let pid = if let Some(pid_idx) = users_part.find("pid=") {
                let sub = &users_part[pid_idx + 4..];
                let digits: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<u32>().ok()
            } else {
                None
            };

            let process_name = users_part.find('"').and_then(|first_quote| {
                let sub = &users_part[first_quote + 1..];
                sub.find('"')
                    .map(|second_quote| sub[..second_quote].to_string())
            });

            (pid, process_name)
        } else {
            (None, None)
        }
    }

    fn extract_netstat_process(token: &str) -> (Option<u32>, Option<String>) {
        if token == "-" || token.is_empty() {
            return (None, None);
        }
        if let Some((pid_str, name)) = token.split_once('/') {
            let pid = pid_str.parse::<u32>().ok();
            let name = if name.is_empty() || name == "-" {
                None
            } else {
                Some(name.to_string())
            };
            (pid, name)
        } else {
            (None, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ss_output() {
        let sample = "\
Netid State  Recv-Q Send-Q Local Address:Port  Peer Address:PortProcess
tcp   LISTEN 0      128          0.0.0.0:22         0.0.0.0:*    users:((\"sshd\",pid=1234,fd=3))
tcp   LISTEN 0      128             [::]:22            [::]:*    users:((\"sshd\",pid=1234,fd=4))
tcp   LISTEN 0      512        127.0.0.1:8080       0.0.0.0:*    users:((\"redash\",pid=999,fd=7))
udp   UNCONN 0      0            0.0.0.0:68         0.0.0.0:*    users:((\"dhclient\",pid=567,fd=6))
";

        let ports = NetworkDiagnostics::parse_listening_ports(sample);
        assert_eq!(ports.len(), 4);

        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].bind_ip, "0.0.0.0");
        assert_eq!(ports[0].port, 22);
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[0].process_name.as_deref(), Some("sshd"));

        assert_eq!(ports[1].proto, "tcp");
        assert_eq!(ports[1].bind_ip, "::");
        assert_eq!(ports[1].port, 22);

        assert_eq!(ports[2].bind_ip, "127.0.0.1");
        assert_eq!(ports[2].port, 8080);
        assert_eq!(ports[2].pid, Some(999));
        assert_eq!(ports[2].process_name.as_deref(), Some("redash"));

        assert_eq!(ports[3].proto, "udp");
        assert_eq!(ports[3].port, 68);
        assert_eq!(ports[3].pid, Some(567));
        assert_eq!(ports[3].process_name.as_deref(), Some("dhclient"));
    }

    #[test]
    fn test_parse_netstat_output() {
        let sample = "\
Active Internet connections (only servers)
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
tcp        0      0 0.0.0.0:22              0.0.0.0:*               LISTEN      1234/sshd
tcp6       0      0 :::22                   :::*                    LISTEN      1234/sshd
tcp        0      0 127.0.0.1:8080          0.0.0.0:*               LISTEN      -
udp        0      0 0.0.0.0:68              0.0.0.0:*                           567/dhclient
";

        let ports = NetworkDiagnostics::parse_listening_ports(sample);
        assert_eq!(ports.len(), 4);

        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].bind_ip, "0.0.0.0");
        assert_eq!(ports[0].port, 22);
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[0].process_name.as_deref(), Some("sshd"));

        assert_eq!(ports[1].proto, "tcp6");
        assert_eq!(ports[1].bind_ip, "::");
        assert_eq!(ports[1].port, 22);

        assert_eq!(ports[2].bind_ip, "127.0.0.1");
        assert_eq!(ports[2].port, 8080);
        assert_eq!(ports[2].pid, None);
        assert_eq!(ports[2].process_name, None);

        assert_eq!(ports[3].proto, "udp");
        assert_eq!(ports[3].port, 68);
        assert_eq!(ports[3].pid, Some(567));
        assert_eq!(ports[3].process_name.as_deref(), Some("dhclient"));
    }

    #[test]
    fn test_commands() {
        assert_eq!(
            NetworkDiagnostics::listening_ports_cmd(),
            "ss -tulpn 2>/dev/null || netstat -tulpn 2>/dev/null"
        );
        let rtt_cmd = NetworkDiagnostics::measure_rtt_cmd("1.1.1.1");
        assert!(rtt_cmd.contains("ping -c 1 -W 2 1.1.1.1"));
        assert!(rtt_cmd.contains("ping -c 1 -t 2 1.1.1.1"));
    }
}
