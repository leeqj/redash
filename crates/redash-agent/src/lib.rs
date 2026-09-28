pub mod client;
pub mod collector;
pub mod remediation;
pub mod tty;

pub use client::{AgentClient, AgentConfig};
pub use collector::TelemetryCollector;
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
}
