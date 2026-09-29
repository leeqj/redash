pub mod client;
pub mod collector;
pub mod discovery;
pub mod remediation;
pub mod tty;

pub use client::{calculate_adaptive_cadence, AgentClient, AgentConfig};
pub use collector::TelemetryCollector;
pub use discovery::LanDiscoveryAgent;
pub use remediation::RemediationEngine;

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use redash_types::{RemediationAction, SignedAction};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn test_telemetry_collection_smoke() {
        let mut collector = TelemetryCollector::new("test-node-1".to_string());
        let snapshot = collector.collect().await;

        assert_eq!(snapshot.node_id, "test-node-1");
        assert!(snapshot.cpu_cores >= 1);
        assert!(snapshot.mem_total_bytes > 0);
    }

    #[test]
    fn test_zero_trust_signature_verification() {
        let secret_bytes = [7u8; 32];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        let pub_hex = hex::encode(verifying_key.as_bytes());

        let engine = RemediationEngine::new(Some(&pub_hex)).unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let action = RemediationAction::RestartContainer {
            container_id: "c-12345".to_string(),
        };

        let signable_bytes =
            SignedAction::canonical_signable_bytes("act-1", "node-1", &action, now, "nonce-random")
                .unwrap();

        let sig = signing_key.sign(&signable_bytes);
        let sig_hex = hex::encode(sig.to_bytes());

        let valid_action = SignedAction {
            action_id: "act-1".to_string(),
            node_id: "node-1".to_string(),
            action: action.clone(),
            timestamp: now,
            nonce: "nonce-random".to_string(),
            public_key_hex: pub_hex.clone(),
            signature_hex: sig_hex,
        };

        assert!(engine.verify_signature(&valid_action).is_ok());

        // Test forged action rejection (tampered container_id)
        let mut forged_action = valid_action.clone();
        forged_action.action = RemediationAction::PruneContainers;
        assert!(engine.verify_signature(&forged_action).is_err());
    }

    #[test]
    fn test_replay_attack_prevention() {
        let secret_bytes = [9u8; 32];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        let pub_hex = hex::encode(verifying_key.as_bytes());

        let engine = RemediationEngine::new(Some(&pub_hex)).unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let action = RemediationAction::VacuumLogs { max_size_mb: 50 };

        let signable_bytes =
            SignedAction::canonical_signable_bytes("act-9", "node-9", &action, now, "unique-nonce-1")
                .unwrap();

        let sig = signing_key.sign(&signable_bytes);
        let sig_hex = hex::encode(sig.to_bytes());

        let valid_action = SignedAction {
            action_id: "act-9".to_string(),
            node_id: "node-9".to_string(),
            action: action.clone(),
            timestamp: now,
            nonce: "unique-nonce-1".to_string(),
            public_key_hex: pub_hex.clone(),
            signature_hex: sig_hex,
        };

        // First verification succeeds
        assert!(engine.verify_signature(&valid_action).is_ok());

        // Replaying the exact same action with identical nonce must be rejected
        let replay_result = engine.verify_signature(&valid_action);
        assert!(replay_result.is_err());
        assert!(replay_result.unwrap_err().contains("duplicate nonce"));

        // Expired timestamp (>60s old) must be rejected
        let mut expired_action = valid_action.clone();
        expired_action.timestamp = now - 100;
        expired_action.nonce = "unique-nonce-2".to_string();
        let expired_result = engine.verify_signature(&expired_action);
        assert!(expired_result.is_err());
        assert!(expired_result.unwrap_err().contains("expired"));
    }

    #[test]
    fn test_adaptive_cadence_transitions() {
        use redash_types::{AgentTelemetry, ContainerSummary};
        use std::time::Duration;

        let base_telemetry = AgentTelemetry {
            node_id: "node-1".to_string(),
            hostname: "homelab".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            timestamp: 1710000000,
            uptime_secs: 3600,
            cpu_usage_pct: 12.0,
            cpu_cores: 8,
            mem_used_bytes: 4 * 1024 * 1024 * 1024,
            mem_total_bytes: 16 * 1024 * 1024 * 1024, // 25.0%
            disk_used_bytes: 50 * 1024 * 1024 * 1024,
            disk_total_bytes: 500 * 1024 * 1024 * 1024,
            net_rx_rate: 1024,
            net_tx_rate: 2048,
            containers: vec![
                ContainerSummary {
                    id: "c1".to_string(),
                    name: "redis".to_string(),
                    image: "redis:alpine".to_string(),
                    state: "running".to_string(),
                    status: "Up 2 hours".to_string(),
                    created: 1709990000,
                    ports: vec![],
                }
            ],
        };

        let container_snapshot = vec![("c1".to_string(), "running".to_string())];

        // 1. Steady state with no spikes -> stays at base interval (5s)
        let interval = calculate_adaptive_cadence(
            5,
            &base_telemetry,
            Some(12.5),
            Some(25.1),
            Some(&container_snapshot),
        );
        assert_eq!(interval, Duration::from_secs(5));

        // 2. High CPU stress (>= 80%) -> drops to 1s
        let mut stressed_cpu = base_telemetry.clone();
        stressed_cpu.cpu_usage_pct = 82.5;
        let interval = calculate_adaptive_cadence(
            5,
            &stressed_cpu,
            Some(80.0),
            Some(25.0),
            Some(&container_snapshot),
        );
        assert_eq!(interval, Duration::from_secs(1));

        // 3. Volatile RAM shift (>= 2% delta) -> drops to 1s
        let mut jumped_mem = base_telemetry.clone();
        jumped_mem.mem_used_bytes = (4.5 * 1024.0 * 1024.0 * 1024.0) as u64; // ~28.1%, delta 3.1%
        let interval = calculate_adaptive_cadence(
            5,
            &jumped_mem,
            Some(12.0),
            Some(25.0),
            Some(&container_snapshot),
        );
        assert_eq!(interval, Duration::from_secs(1));

        // 4. Volatile CPU jump (>= 5% delta) -> drops to 1s
        let mut jumped_cpu = base_telemetry.clone();
        jumped_cpu.cpu_usage_pct = 19.5; // delta 7.5% from 12.0%
        let interval = calculate_adaptive_cadence(
            5,
            &jumped_cpu,
            Some(12.0),
            Some(25.0),
            Some(&container_snapshot),
        );
        assert_eq!(interval, Duration::from_secs(1));

        // 5. Container exited / restarting -> drops to 1s
        let mut dead_container = base_telemetry.clone();
        dead_container.containers[0].state = "exited".to_string();
        let interval = calculate_adaptive_cadence(
            5,
            &dead_container,
            Some(12.0),
            Some(25.0),
            Some(&container_snapshot),
        );
        assert_eq!(interval, Duration::from_secs(1));
    }
}
