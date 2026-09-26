pub mod agent;
pub mod formatters;
pub mod host;
pub mod math;
pub mod metrics;
pub mod protocol;
pub mod settings;

pub use agent::*;
pub use formatters::*;
pub use host::*;
pub use math::*;
pub use metrics::*;
pub use protocol::*;
pub use settings::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_id_and_validation() {
        let mut host = HostConfig::new("Production Server", "192.168.1.10", "deploy");
        assert!(host.validate().is_ok());

        host.name = "".to_string();
        assert_eq!(host.validate(), Err(HostValidationError::EmptyName));

        host.name = "Valid Name".to_string();
        host.port = 0;
        assert_eq!(host.validate(), Err(HostValidationError::ZeroPort));
    }

    #[test]
    fn test_formatters() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_percent(45.678), "45.7%");
        assert_eq!(format_duration(3665), "1h 1m 5s");
        assert_eq!(format_bytes_rate(1024), "1.0 KB/s");
    }

    #[test]
    fn test_math_meter_level_and_sparkline() {
        assert_eq!(evaluate_meter_level(40.0), MeterLevel::Normal);
        assert_eq!(evaluate_meter_level(78.0), MeterLevel::Warning);
        assert_eq!(evaluate_meter_level(95.0), MeterLevel::Critical);

        let samples = vec![0.0, 50.0, 100.0];
        let coords = normalize_sparkline(&samples, 10.0, 20.0, 100.0, 50.0);
        assert_eq!(coords.len(), 3);
        assert_eq!(coords[0], (10.0, 70.0)); // 0% load -> bottom (y = 20 + 50)
        assert_eq!(coords[1], (60.0, 45.0)); // 50% load -> middle (y = 20 + 25)
        assert_eq!(coords[2], (110.0, 20.0)); // 100% load -> top (y = 20 + 0)
    }

    #[test]
    fn test_protocol_serde() {
        let msg = ClientTerminalMessage::Resize { cols: 120, rows: 40 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cols\":120"));

        let de: ClientTerminalMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(de, msg);
    }

    #[test]
    fn test_agent_metrics_extraction() {
        let text = "Claude Code\nTokens: 14.5k | Cost: $0.082\nGenerating solution...";
        let (cost, tokens) = extract_metrics_from_buffer(text);
        assert_eq!(cost, Some(0.082));
        assert_eq!(tokens, Some(14500));
    }
}
