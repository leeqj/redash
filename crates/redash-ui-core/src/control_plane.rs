use ed25519_dalek::{Signer, SigningKey};
use redash_types::{RemediationAction, SignedAction};

pub struct ClientSigner;

impl ClientSigner {
    /// Loads an ED25519 keypair from a JSON file, or generates a new one and persists it.
    /// Ensures consistent key identity across application launches.
    pub fn load_or_generate_keypair(
        key_path: &std::path::Path,
    ) -> Result<(String, String), String> {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Store {
            #[serde(default)]
            version: u8,
            public_key: String,
            private_key: String,
        }
        match std::fs::read_to_string(key_path) {
            Ok(content) => {
                let store: Store = serde_json::from_str(&content).map_err(|e| e.to_string())?;
                if store.version != 2 {
                    return Err(format!(
                        "Legacy control-plane key at {} must be rotated: back it up, remove it, restart and re-enroll Agents with the new public key",
                        key_path.display()
                    ));
                }
                let seed: [u8; 32] = hex::decode(&store.private_key)
                    .map_err(|e| e.to_string())?
                    .try_into()
                    .map_err(|_| "Invalid private key length")?;
                if Self::keypair_from_seed(&seed).0 != store.public_key {
                    return Err("Stored keypair does not match".into());
                }
                return Ok((store.public_key, store.private_key));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.to_string()),
        }
        let (public_key, private_key) = Self::generate_keypair();
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let store = Store {
            version: 2,
            public_key: public_key.clone(),
            private_key: private_key.clone(),
        };
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options.open(key_path).map_err(|e| e.to_string())?;
        file.write_all(
            serde_json::to_string_pretty(&store)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok((public_key, private_key))
    }

    pub fn sign_fresh_action(
        private_key: &str,
        node_id: &str,
        action: RemediationAction,
    ) -> Result<SignedAction, String> {
        Self::sign_action(
            private_key,
            node_id,
            action,
            crate::e2ee::now_secs(),
            &crate::e2ee::random_hex(),
        )
    }

    /// Generates an ED25519 keypair.
    pub fn generate_keypair() -> (String, String) {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).expect("OS cryptographic random source unavailable");
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
        let priv_bytes =
            hex::decode(private_key_hex).map_err(|e| format!("Invalid private key hex: {}", e))?;

        let seed: [u8; 32] = priv_bytes
            .try_into()
            .map_err(|_| "Private key must be exactly 32 bytes".to_string())?;

        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let pub_hex = hex::encode(verifying_key.as_bytes());

        let action_id = format!("act-{nonce}");

        let signable_bytes =
            SignedAction::canonical_signable_bytes(&action_id, node_id, &action, timestamp, nonce)
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
            shell_arg(hub_url)
        );

        if let Some(id) = node_id {
            cmd.push_str(&format!(" --node-id {}", shell_arg(id)));
        }

        if !auth_token.is_empty() && auth_token != "default-token" {
            cmd.push_str(&format!(" --token {}", shell_arg(auth_token)));
        }

        if let Some(key) = public_key_hex {
            cmd.push_str(&format!(" --key {}", shell_arg(key)));
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
            shell_arg(hub_url)
        );

        if let Some(id) = node_id {
            cmd.push_str(&format!(" -e REDASH_NODE_ID={}", shell_arg(id)));
        }

        if !auth_token.is_empty() && auth_token != "default-token" {
            cmd.push_str(&format!(" -e REDASH_AUTH_TOKEN={}", shell_arg(auth_token)));
        }

        if let Some(key) = public_key_hex {
            cmd.push_str(&format!(" -e REDASH_TRUSTED_KEY={}", shell_arg(key)));
        }

        cmd.push_str(" ghcr.io/reways/redash-agent:latest");
        cmd
    }
}

fn shell_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:/.".contains(&b))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
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

#[cfg(test)]
mod identity_storage_tests {
    use super::*;
    #[test]
    fn legacy_identity_is_preserved_and_new_identity_is_private_and_stable() {
        let path = std::env::temp_dir().join(format!(
            "redash-identity-{}.json",
            crate::e2ee::random_hex()
        ));
        let (public, private) = ClientSigner::generate_keypair();
        let legacy = serde_json::json!({"public_key": public, "private_key": private}).to_string();
        std::fs::write(&path, &legacy).unwrap();
        assert!(
            ClientSigner::load_or_generate_keypair(&path)
                .unwrap_err()
                .contains("must be rotated")
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), legacy);
        std::fs::remove_file(&path).unwrap();
        let fresh = ClientSigner::load_or_generate_keypair(&path).unwrap();
        assert_eq!(
            fresh,
            ClientSigner::load_or_generate_keypair(&path).unwrap()
        );
        assert_ne!(fresh.0, public);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn same_second_actions_have_independent_nonces_and_ids() {
        let (_, private) = ClientSigner::generate_keypair();
        let first = ClientSigner::sign_fresh_action(
            &private,
            "node",
            RemediationAction::DiagnosePort { port: 6379 },
        )
        .unwrap();
        let second = ClientSigner::sign_fresh_action(
            &private,
            "node",
            RemediationAction::DiagnosePort { port: 9090 },
        )
        .unwrap();
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.action_id, second.action_id);
    }
}
