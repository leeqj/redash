pub mod agent;
pub mod batch;
pub mod buffer;
pub mod control_plane;
pub mod formatters;
pub mod host;
pub mod math;
pub mod metrics;
pub mod protocol;
pub mod settings;
pub mod sftp;

pub use agent::*;
pub use batch::*;
pub use buffer::*;
pub use control_plane::*;
pub use formatters::*;
pub use host::*;
pub use math::*;
pub use metrics::*;
pub use protocol::*;
pub use settings::*;
pub use sftp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_plane_canonical_bytes() {
        let action = RemediationAction::RestartContainer {
            container_id: "c-12345".to_string(),
        };
        let bytes = SignedAction::canonical_signable_bytes(
            "act-1",
            "node-1",
            &action,
            1700000000,
            "nonce-xyz",
        )
        .unwrap();

        assert!(!bytes.is_empty());
        let str_rep = String::from_utf8(bytes).unwrap();
        assert!(str_rep.contains("c-12345"));
        assert!(str_rep.contains("restart_container"));
    }

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
        assert_eq!(coords[0], (10.0, 70.0));
        assert_eq!(coords[1], (60.0, 45.0));
        assert_eq!(coords[2], (110.0, 20.0));
    }

    #[test]
    fn test_protocol_serde() {
        let msg = ClientTerminalMessage::Resize {
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cols\":120"));

        let de: ClientTerminalMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(de, msg);
    }

    #[test]
    fn test_e2ee_envelope_and_handshake_serde() {
        let env = EncryptedEnvelope::new("sess-1", 1, "001122", "aabbcc", "ddeeff");
        let json = serde_json::to_string(&env).unwrap();
        let de: EncryptedEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(de, env);

        let init = E2eeHandshakeInit {
            version: 2,
            node_id: "test-node".into(),
            session_id: "sess-1".to_string(),
            client_ephemeral_pubkey_hex: "010203".to_string(),
            timestamp: 1700000000,
            nonce: "nonce-1".to_string(),
            signature_hex: "sig-1".to_string(),
        };
        let init_json = serde_json::to_string(&init).unwrap();
        let de_init: E2eeHandshakeInit = serde_json::from_str(&init_json).unwrap();
        assert_eq!(de_init, init);

        let ack = E2eeHandshakeAck {
            signature_hex: "sig".into(),
            session_id: "sess-1".to_string(),
            agent_ephemeral_pubkey_hex: "040506".to_string(),
            success: true,
            error_msg: None,
        };
        let ack_json = serde_json::to_string(&ack).unwrap();
        let de_ack: E2eeHandshakeAck = serde_json::from_str(&ack_json).unwrap();
        assert_eq!(de_ack, ack);

        // Test LanBeacon
        let beacon = LanBeacon {
            node_id: "node-lan-1".to_string(),
            hostname: "mac-studio.local".to_string(),
            direct_port: 43210,
            version: "0.2.1-beta".to_string(),
            timestamp: 1700000000,
        };
        let beacon_json = serde_json::to_string(&beacon).unwrap();
        let de_beacon: LanBeacon = serde_json::from_str(&beacon_json).unwrap();
        assert_eq!(de_beacon, beacon);

        // Test DevicePairingPayload URI roundtrip
        let payload = DevicePairingPayload {
            hub_url: "ws://192.168.1.10:8080".to_string(),
            client_public_key: "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
                .to_string(),
            device_name: "MacBook Pro M3".to_string(),
            auth_token: Some("secret-token-xyz".to_string()),
            node_id: Some("node-prod-01".to_string()),
            created_at: 1700000000,
        };
        let uri = payload.to_uri();
        assert!(uri.starts_with("redash://pair?data="));
        let decoded = DevicePairingPayload::from_uri(&uri).unwrap();
        assert_eq!(decoded, payload);

        // Test to_websocket_endpoint
        assert_eq!(
            to_websocket_endpoint("http://127.0.0.1:8080/"),
            "ws://127.0.0.1:8080"
        );
        assert_eq!(
            to_websocket_endpoint("https://hub.example.com"),
            "wss://hub.example.com"
        );
        assert_eq!(
            to_websocket_endpoint("ws://custom:9000"),
            "ws://custom:9000"
        );

        // Test shell helpers
        assert!(!default_system_shell().is_empty());
        assert!(format_pty_resize_command(24, 80).contains("COLUMNS=80 LINES=24"));
    }
}
