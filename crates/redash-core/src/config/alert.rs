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

    pub async fn send_webhook(webhook_url: &str, event: &AlertEvent) -> Result<()> {
        let payload = serde_json::to_string(event)?;

        let output = tokio::process::Command::new("curl")
            .arg("--fail")
            .arg("--proto")
            .arg("=http,https")
            .arg("-s")
            .arg("-S")
            .arg("--output")
            .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
            .arg("--write-out")
            .arg("%{http_code}")
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

        let status: u16 = String::from_utf8_lossy(&output.stdout).trim().parse()?;
        anyhow::ensure!(
            (200..300).contains(&status),
            "Webhook returned HTTP {status}"
        );
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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
}
