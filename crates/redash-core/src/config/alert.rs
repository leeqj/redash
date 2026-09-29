use crate::probe::NodeMetrics;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AlertRule {
    pub cpu_threshold_percent: Option<f32>,
    pub mem_threshold_percent: Option<f32>,
    pub disk_threshold_percent: Option<f32>,
    #[serde(default)]
    pub notify_offline: bool,
    #[serde(default)]
    pub macos_notification: bool,
    #[serde(default)]
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AlertEvent {
    pub host_id: String,
    pub host_name: String,
    pub alert_type: String,
    pub message: String,
    pub timestamp: u64,
}

pub struct AlertDispatcher;

impl AlertDispatcher {
    pub fn format_macos_notification_script(title: &str, message: &str) -> String {
        let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_msg = message.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            "display notification \"{}\" with title \"{}\"",
            escaped_msg, escaped_title
        )
    }

    pub async fn send_macos_notification(title: &str, message: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let script = Self::format_macos_notification_script(title, message);
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                tokio::process::Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .kill_on_drop(true)
                    .output(),
            )
            .await??;

            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("osascript failed: {}", err);
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (title, message);
            Ok(())
        }
    }

    pub fn is_feishu_webhook(webhook_url: &str) -> bool {
        webhook_url.contains("open.feishu.cn") || webhook_url.contains("open.larksuite.com")
    }

    pub fn format_timestamp_utc(timestamp: u64) -> String {
        let days = timestamp / 86400;
        let time_of_day = timestamp % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;

        let mut year = 1970;
        let mut d = days;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            let days_in_year = if leap { 366 } else { 365 };
            if d < days_in_year {
                break;
            }
            d -= days_in_year;
            year += 1;
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_months = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut month = 1;
        for &dim in &days_in_months {
            if d < dim {
                break;
            }
            d -= dim;
            month += 1;
        }
        let day = d + 1;
        format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02} UTC")
    }

    pub fn build_feishu_payload(event: &AlertEvent) -> serde_json::Value {
        let (title, template_color) = match event.alert_type.as_str() {
            "offline" => ("🚨 主机离线警报", "carmine"),
            "cpu" => ("⚠️ CPU 负载告警", "red"),
            "mem" => ("⚠️ 内存过载告警", "orange"),
            "disk" => ("⚠️ 磁盘空间紧迫告警", "red"),
            _ => ("⚠️ 服务器指标告警", "orange"),
        };

        let type_cn = match event.alert_type.as_str() {
            "offline" => "节点离线 / 采集超时",
            "cpu" => "CPU 负载超标",
            "mem" => "内存占用超标",
            "disk" => "磁盘空间超标",
            other => other,
        };

        let time_str = Self::format_timestamp_utc(event.timestamp);

        serde_json::json!({
            "msg_type": "interactive",
            "card": {
                "header": {
                    "title": {
                        "tag": "plain_text",
                        "content": format!("ReDash - {}", title)
                    },
                    "template": template_color
                },
                "elements": [
                    {
                        "tag": "div",
                        "fields": [
                            {
                                "is_short": true,
                                "text": {
                                    "tag": "lark_md",
                                    "content": format!("**🖥️ 目标主机**\n{}", event.host_name)
                                }
                            },
                            {
                                "is_short": true,
                                "text": {
                                    "tag": "lark_md",
                                    "content": format!("**🏷️ 告警类型**\n{}", type_cn)
                                }
                            }
                        ]
                    },
                    {
                        "tag": "div",
                        "text": {
                            "tag": "lark_md",
                            "content": format!("**📝 详细信息**\n{}", event.message)
                        }
                    },
                    {
                        "tag": "hr"
                    },
                    {
                        "tag": "note",
                        "elements": [
                            {
                                "tag": "plain_text",
                                "content": format!("时间: {} | Host ID: {} | ReDash 监控中心", time_str, event.host_id)
                            }
                        ]
                    }
                ]
            }
        })
    }

    pub async fn send_webhook(webhook_url: &str, event: &AlertEvent) -> Result<()> {
        let is_feishu = Self::is_feishu_webhook(webhook_url);
        let payload = if is_feishu {
            serde_json::to_string(&Self::build_feishu_payload(event))?
        } else {
            serde_json::to_string(event)?
        };

        let output = tokio::process::Command::new("curl")
            .arg("--proto")
            .arg("=http,https")
            .arg("-s")
            .arg("-S")
            .arg("-w")
            .arg("\n%{http_code}")
            .arg("--max-time")
            .arg("10")
            .arg("-X")
            .arg("POST")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-d")
            .arg(&payload)
            .kill_on_drop(true)
            .arg("--")
            .arg(webhook_url)
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Webhook failed: {}", err);
        }

        let full_output = String::from_utf8_lossy(&output.stdout);
        let (body, status_code_str) = match full_output.rfind('\n') {
            Some(idx) => (&full_output[..idx], full_output[idx + 1..].trim()),
            None => ("", full_output.trim()),
        };

        let status: u16 = status_code_str.parse().map_err(|e| {
            anyhow::anyhow!("Failed to parse HTTP status code '{status_code_str}': {e}")
        })?;

        anyhow::ensure!(
            (200..300).contains(&status),
            "Webhook returned HTTP {status}: {body}"
        );

        if is_feishu && let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(code) = json.get("code").and_then(|c| c.as_i64())
                && code != 0
            {
                let msg = json
                    .get("msg")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                anyhow::bail!("飞书机器人推送被拒绝 (code {code}): {msg}");
            } else if let Some(code) = json.get("StatusCode").and_then(|c| c.as_i64())
                && code != 0
            {
                let msg = json
                    .get("StatusMessage")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                anyhow::bail!("飞书机器人推送被拒绝 (StatusCode {code}): {msg}");
            }
        }

        Ok(())
    }

    pub async fn dispatch(rule: &AlertRule, event: &AlertEvent) -> Result<()> {
        let desktop = async {
            if rule.macos_notification {
                Self::send_macos_notification(
                    &format!("ReDash Alert: {}", event.host_name),
                    &event.message,
                )
                .await
            } else {
                Ok(())
            }
        };
        let webhook = async {
            if let Some(url) = rule
                .webhook_url
                .as_ref()
                .filter(|url| !url.trim().is_empty())
            {
                Self::send_webhook(url, event).await
            } else {
                Ok(())
            }
        };
        let (desktop, webhook) = tokio::join!(desktop, webhook);
        desktop.and(webhook)
    }

    pub fn evaluate_agent_telemetry(
        rule: &AlertRule,
        telemetry: &redash_types::AgentTelemetry,
    ) -> Vec<AlertEvent> {
        let mut rule = rule.clone();
        if !telemetry.validity.cpu
            || !telemetry.cpu_usage_pct.is_finite()
            || !(0.0..=100.0).contains(&telemetry.cpu_usage_pct)
        {
            rule.cpu_threshold_percent = None;
        }
        if !telemetry.validity.memory
            || telemetry.mem_total_bytes == 0
            || telemetry.mem_used_bytes > telemetry.mem_total_bytes
        {
            rule.mem_threshold_percent = None;
        }
        if !telemetry.validity.disk
            || telemetry.disk_total_bytes == 0
            || telemetry.disk_used_bytes > telemetry.disk_total_bytes
        {
            rule.disk_threshold_percent = None;
        }
        let metrics = NodeMetrics {
            timestamp: telemetry.timestamp,
            cpu: crate::probe::CpuMetrics {
                usage_percent: telemetry.cpu_usage_pct,
                ..Default::default()
            },
            mem: crate::probe::MemMetrics {
                usage_percent: telemetry.memory_usage_pct(),
                ..Default::default()
            },
            disks: vec![crate::probe::DiskMetrics {
                mount_point: "/".into(),
                usage_percent: telemetry.disk_usage_pct(),
                ..Default::default()
            }],
            ..Default::default()
        };
        Self::evaluate_metrics(
            &rule,
            &format!("agent:{}", telemetry.node_id),
            &telemetry.hostname,
            &metrics,
        )
    }

    pub fn evaluate_metrics(
        rule: &AlertRule,
        host_id: &str,
        host_name: &str,
        metrics: &NodeMetrics,
    ) -> Vec<AlertEvent> {
        let mut events = Vec::new();
        let timestamp = metrics.timestamp;

        // CPU threshold
        if let Some(threshold) = rule.cpu_threshold_percent
            && metrics.cpu.usage_percent >= threshold
        {
            events.push(AlertEvent {
                host_id: host_id.to_string(),
                host_name: host_name.to_string(),
                alert_type: "cpu".to_string(),
                message: format!(
                    "Host {} CPU usage is {:.1}%, exceeding threshold of {:.1}%",
                    host_name, metrics.cpu.usage_percent, threshold
                ),
                timestamp,
            });
        }

        // Memory threshold
        if let Some(threshold) = rule.mem_threshold_percent
            && metrics.mem.usage_percent >= threshold
        {
            events.push(AlertEvent {
                host_id: host_id.to_string(),
                host_name: host_name.to_string(),
                alert_type: "mem".to_string(),
                message: format!(
                    "Host {} Memory usage is {:.1}%, exceeding threshold of {:.1}%",
                    host_name, metrics.mem.usage_percent, threshold
                ),
                timestamp,
            });
        }

        // Disk threshold
        if let Some(threshold) = rule.disk_threshold_percent {
            for disk in &metrics.disks {
                if disk.usage_percent >= threshold {
                    events.push(AlertEvent {
                        host_id: host_id.to_string(),
                        host_name: host_name.to_string(),
                        alert_type: "disk".to_string(),
                        message: format!(
                            "Host {} Disk ({}) usage is {:.1}%, exceeding threshold of {:.1}%",
                            host_name, disk.mount_point, disk.usage_percent, threshold
                        ),
                        timestamp,
                    });
                }
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::{CpuMetrics, DiskMetrics, MemMetrics};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_alert_rule_serde() {
        let rule = AlertRule {
            cpu_threshold_percent: Some(85.0),
            mem_threshold_percent: Some(90.0),
            disk_threshold_percent: Some(95.0),
            notify_offline: true,
            macos_notification: true,
            webhook_url: Some("https://hooks.slack.com/services/xxx".into()),
        };

        let json = serde_json::to_string(&rule).expect("serialize");
        let deserialized: AlertRule = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rule, deserialized);
    }

    #[test]
    fn test_format_macos_notification_script() {
        let script = AlertDispatcher::format_macos_notification_script(
            "Alert \"Warning\"",
            "CPU usage at 95% \\ high load",
        );
        assert_eq!(
            script,
            "display notification \"CPU usage at 95% \\\\ high load\" with title \"Alert \\\"Warning\\\"\""
        );
    }

    #[test]
    fn test_evaluate_metrics() {
        let rule = AlertRule {
            cpu_threshold_percent: Some(80.0),
            mem_threshold_percent: Some(80.0),
            disk_threshold_percent: Some(90.0),
            notify_offline: false,
            macos_notification: false,
            webhook_url: None,
        };

        let mut metrics = NodeMetrics {
            timestamp: 1700000000,
            cpu: CpuMetrics {
                usage_percent: 85.0,
                ..Default::default()
            },
            mem: MemMetrics {
                usage_percent: 75.0,
                ..Default::default()
            },
            disks: vec![
                DiskMetrics {
                    mount_point: "/".into(),
                    usage_percent: 92.5,
                    ..Default::default()
                },
                DiskMetrics {
                    mount_point: "/data".into(),
                    usage_percent: 40.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let events = AlertDispatcher::evaluate_metrics(&rule, "h1", "prod-web", &metrics);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].alert_type, "cpu");
        assert!(events[0].message.contains("85.0%"));
        assert_eq!(events[1].alert_type, "disk");
        assert!(events[1].message.contains("/"));
        assert!(events[1].message.contains("92.5%"));

        // When metrics drop below threshold
        metrics.cpu.usage_percent = 50.0;
        metrics.disks[0].usage_percent = 70.0;
        let events_cleared = AlertDispatcher::evaluate_metrics(&rule, "h1", "prod-web", &metrics);
        assert!(events_cleared.is_empty());
    }

    #[tokio::test]
    async fn test_send_webhook_via_mock_server() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => panic!("failed to bind mock server: {}", e),
        };
        let port = listener.local_addr().unwrap().port();
        let webhook_url = format!("http://127.0.0.1:{}", port);

        let event = AlertEvent {
            host_id: "node-1".into(),
            host_name: "test-node".into(),
            alert_type: "cpu".into(),
            message: "CPU 99%".into(),
            timestamp: 1234567890,
        };

        let server_task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 2048];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);

            assert!(request.starts_with("POST / HTTP/"));
            assert!(request.contains("\"alert_type\":\"cpu\""));
            assert!(request.contains("\"host_name\":\"test-node\""));

            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        AlertDispatcher::send_webhook(&webhook_url, &event)
            .await
            .expect("send_webhook should succeed");

        server_task.await.unwrap();
    }
    #[tokio::test]
    async fn webhook_http_failure_is_not_success() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => panic!("failed to bind mock server: {}", e),
        };
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        assert!(
            AlertDispatcher::send_webhook(&url, &AlertEvent::default())
                .await
                .is_err()
        );
        server.await.unwrap();
    }

    #[test]
    fn test_feishu_detection_and_payload() {
        let feishu_url = "https://open.feishu.cn/open-apis/bot/v2/hook/abc-123";
        let lark_url = "https://open.larksuite.com/open-apis/bot/v2/hook/xyz-789";
        let generic_url = "https://example.com/webhook";

        assert!(AlertDispatcher::is_feishu_webhook(feishu_url));
        assert!(AlertDispatcher::is_feishu_webhook(lark_url));
        assert!(!AlertDispatcher::is_feishu_webhook(generic_url));

        let event = AlertEvent {
            host_id: "host-42".into(),
            host_name: "prod-db-master".into(),
            alert_type: "cpu".into(),
            message: "CPU 98.5% overload".into(),
            timestamp: 1700000000,
        };

        let payload = AlertDispatcher::build_feishu_payload(&event);
        assert_eq!(payload["msg_type"], "interactive");
        assert_eq!(payload["card"]["header"]["template"], "red");
        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(json_str.contains("prod-db-master"));
        assert!(json_str.contains("CPU 98.5% overload"));
        assert!(json_str.contains("CPU 负载超标"));
    }
}
