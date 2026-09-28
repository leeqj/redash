use ed25519_dalek::{Signer, SigningKey};
use redash_types::{RemediationAction, SignedAction};

pub struct ClientSigner;

impl ClientSigner {
    /// Generates an ED25519 keypair.
    pub fn generate_keypair() -> (String, String) {
        let mut seed = [0u8; 32];
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let now_bytes = now.to_le_bytes();
        let pid_bytes = pid.to_le_bytes();
        for i in 0..16 {
            seed[i] = now_bytes[i % now_bytes.len()] ^ 0x5a ^ (i as u8);
        }
        for i in 16..32 {
            seed[i] = pid_bytes[(i - 16) % pid_bytes.len()] ^ 0xa5 ^ (i as u8);
        }
        Self::keypair_from_seed(&seed)
    }

    /// Generates an ED25519 keypair from a 32-byte seed.
    /// Returns `(public_key_hex, private_key_hex)`.
    pub fn keypair_from_seed(seed: &[u8; 32]) -> (String, String) {
        let signing_key = SigningKey::from_bytes(seed);
        let verifying_key = signing_key.verifying_key();
        let pub_hex = hex::encode(verifying_key.as_bytes());
        let priv_hex = hex::encode(seed);
        (pub_hex, priv_hex)
    }

    /// Digitally signs a remediation action using the client's private key.
    /// Produces a cryptographically secure `SignedAction` for zero-trust delivery.
    pub fn sign_action(
        private_key_hex: &str,
        node_id: &str,
        action: RemediationAction,
        timestamp: u64,
        nonce: &str,
    ) -> Result<SignedAction, String> {
        let priv_bytes = hex::decode(private_key_hex)
            .map_err(|e| format!("Invalid private key hex: {}", e))?;

        let seed: [u8; 32] = priv_bytes
            .try_into()
            .map_err(|_| "Private key must be exactly 32 bytes".to_string())?;

        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let pub_hex = hex::encode(verifying_key.as_bytes());

        let action_id = format!("act-{}", &nonce[..8.min(nonce.len())]);

        let signable_bytes = SignedAction::canonical_signable_bytes(
            &action_id,
            node_id,
            &action,
            timestamp,
            nonce,
        )
        .map_err(|e| format!("Serialization error: {}", e))?;

        let sig = signing_key.sign(&signable_bytes);
        let sig_hex = hex::encode(sig.to_bytes());

        Ok(SignedAction {
            action_id,
            node_id: node_id.to_string(),
            action,
            timestamp,
            nonce: nonce.to_string(),
            public_key_hex: pub_hex,
            signature_hex: sig_hex,
        })
    }

    /// Generates the one-line bash command for enrolling a new node into this control plane.
    pub fn format_onboarding_command(
        hub_url: &str,
        node_id: Option<&str>,
        auth_token: &str,
        public_key_hex: Option<&str>,
    ) -> String {
        let mut cmd = format!(
            "curl -fsSL https://raw.githubusercontent.com/reways/redash/main/scripts/install_agent.sh | sh -s -- --hub {}",
            hub_url
        );

        if let Some(id) = node_id {
            cmd.push_str(&format!(" --node-id {}", id));
        }

        if !auth_token.is_empty() && auth_token != "default-token" {
            cmd.push_str(&format!(" --token {}", auth_token));
        }

        if let Some(key) = public_key_hex {
            cmd.push_str(&format!(" --key {}", key));
        }

        cmd
    }

    /// Generates the one-line docker run command for containerized agent deployment.
    pub fn format_docker_command(
        hub_url: &str,
        node_id: Option<&str>,
        auth_token: &str,
        public_key_hex: Option<&str>,
    ) -> String {
        let mut cmd = format!(
            "docker run -d --name redash-agent --restart always --net host --pid host -v /var/run/docker.sock:/var/run/docker.sock:ro -e REDASH_HUB_URL={}",
            hub_url
        );

        if let Some(id) = node_id {
            cmd.push_str(&format!(" -e REDASH_NODE_ID={}", id));
        }

        if !auth_token.is_empty() && auth_token != "default-token" {
            cmd.push_str(&format!(" -e REDASH_AUTH_TOKEN={}", auth_token));
        }

        if let Some(key) = public_key_hex {
            cmd.push_str(&format!(" -e REDASH_TRUSTED_KEY={}", key));
        }

        cmd.push_str(" ghcr.io/reways/redash-agent:latest");
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_signer_roundtrip() {
        let seed = [42u8; 32];
        let (pub_hex, priv_hex) = ClientSigner::keypair_from_seed(&seed);
        assert_eq!(pub_hex.len(), 64);
        assert_eq!(priv_hex.len(), 64);

        let action = RemediationAction::RestartContainer {
            container_id: "nginx-frontend".to_string(),
        };

        let signed = ClientSigner::sign_action(
            &priv_hex,
            "node-paris-1",
            action,
            1710000000,
            "nonce-random-12345",
        )
        .expect("Signing must succeed");

        assert_eq!(signed.public_key_hex, pub_hex);
        assert!(!signed.signature_hex.is_empty());
        assert_eq!(signed.node_id, "node-paris-1");
    }

    #[test]
    fn test_format_onboarding_command() {
        let cmd = ClientSigner::format_onboarding_command(
            "wss://hub.reways.dev/v1/agent/ws",
            Some("oracle-arm-1"),
            "token-secret",
            Some("pk-abcdef123456"),
        );

        assert!(cmd.contains("wss://hub.reways.dev/v1/agent/ws"));
        assert!(cmd.contains("--node-id oracle-arm-1"));
        assert!(cmd.contains("--key pk-abcdef123456"));
    }
}
