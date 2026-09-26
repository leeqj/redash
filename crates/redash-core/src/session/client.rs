use anyhow::{Context, Result};
use russh::{client, keys::PublicKeyOrCertificate};
use std::path::PathBuf;

#[derive(Clone)]
pub struct ClientHandler {
    hostname: String,
    port: u16,
    known_hosts: PathBuf,
}

impl ClientHandler {
    pub fn new(hostname: &str, port: u16) -> Result<Self> {
        let path = dirs::home_dir()
            .context("Cannot locate SSH known_hosts")?
            .join(".ssh/known_hosts");
        Ok(Self::with_known_hosts(hostname, port, path))
    }
    pub fn with_known_hosts(hostname: &str, port: u16, known_hosts: PathBuf) -> Self {
        Self {
            hostname: hostname.trim_matches(['[', ']']).to_string(),
            port,
            known_hosts,
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, server_key: &PublicKeyOrCertificate) -> Result<bool> {
        anyhow::ensure!(
            server_key.certificate().is_none(),
            "SSH host certificates are not supported; configure a pinned host public key"
        );
        let key = server_key.public_key();
        let fingerprint = key.fingerprint(russh::keys::HashAlg::Sha256);
        // russh's known_hosts helper handles plain/hashed names but ignores markers.
        // A revoked key must never become trusted through another matching entry.
        match std::fs::read_to_string(&self.known_hosts) {
            Ok(contents) => {
                for line in contents.lines() {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    if fields.first() == Some(&"@revoked") && fields.len() >= 4 {
                        let revoked = russh::keys::parse_public_key_base64(fields[3])
                            .context("Invalid revoked key in known_hosts")?;
                        anyhow::ensure!(
                            revoked != key,
                            "SSH host key {fingerprint} is revoked in known_hosts"
                        );
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Cannot read SSH known_hosts"),
        }

        let trusted =
            russh::keys::check_known_hosts_path(&self.hostname, self.port, &key, &self.known_hosts)
                .with_context(|| {
                    format!(
                        "SSH host key verification failed for {}:{} ({fingerprint})",
                        self.hostname, self.port
                    )
                })?;
        anyhow::ensure!(
            trusted,
            "Unknown SSH host key for {}:{} ({fingerprint}). Verify this fingerprint with the administrator, then trust it using ssh and {} before retrying",
            self.hostname,
            self.port,
            self.known_hosts.display()
        );
        Ok(true)
    }
}
