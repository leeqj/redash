use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(not(test))]
const SERVICE_NAME: &str = "redash-ssh";
// Read-only compatibility with the old format. Never write new file credentials.
const VAULT_SALT: &[u8] = b"REDASH_NATIVE_VAULT_SALT_v1";
#[cfg(not(test))]
static MIGRATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultBackendType {
    NativeKeyring,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub backend: VaultBackendType,
    pub native_keyring_available: bool,
    pub encrypted_file_path: Option<String>,
    pub memory_cached_count: usize,
}

#[derive(Debug, thiserror::Error)]
#[error("Saved credential is missing; edit this host to save it again")]
pub struct MissingCredential;

fn legacy_paths() -> Vec<PathBuf> {
    let Some(base) = dirs::config_dir().or_else(dirs::home_dir) else {
        return Vec::new();
    };
    ["redash", "serverbox"]
        .iter()
        .map(|name| base.join(name).join("credentials.enc"))
        .collect()
}

fn derive_machine_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(VAULT_SALT);
    if let Ok(hostname) = std::env::var("HOSTNAME").or_else(|_| std::env::var("HOST")) {
        hasher.update(hostname.as_bytes());
    }
    if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        hasher.update(user.as_bytes());
    }
    if let Some(home) = dirs::home_dir() {
        hasher.update(home.to_string_lossy().as_bytes());
    }
    hasher.finalize().into()
}

#[cfg(test)]
fn encrypt_payload(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let mut output = Vec::with_capacity(data.len() + 32);
    // Generate deterministic counter keystream using SHA256
    let mut block_idx: u32 = 0;
    let mut chunk_idx = 0;
    while chunk_idx < data.len() {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(block_idx.to_be_bytes());
        let mask = hasher.finalize();
        let chunk_size = (data.len() - chunk_idx).min(32);
        for i in 0..chunk_size {
            output.push(data[chunk_idx + i] ^ mask[i]);
        }
        chunk_idx += chunk_size;
        block_idx = block_idx.wrapping_add(1);
    }
    // Append HMAC / checksum tag
    let mut tag_hasher = Sha256::new();
    tag_hasher.update(key);
    tag_hasher.update(&output);
    let tag = tag_hasher.finalize();
    output.extend_from_slice(&tag);
    output
}

fn decrypt_payload(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    if data.len() < 32 {
        anyhow::bail!("Encrypted vault payload too short");
    }
    let (ciphertext, tag) = data.split_at(data.len() - 32);
    let mut tag_hasher = Sha256::new();
    tag_hasher.update(key);
    tag_hasher.update(ciphertext);
    let expected_tag = tag_hasher.finalize();
    if tag != expected_tag.as_slice() {
        anyhow::bail!("Vault checksum verification failed (corrupted or wrong machine)");
    }

    let mut output = Vec::with_capacity(ciphertext.len());
    let mut block_idx: u32 = 0;
    let mut chunk_idx = 0;
    while chunk_idx < ciphertext.len() {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(block_idx.to_be_bytes());
        let mask = hasher.finalize();
        let chunk_size = (ciphertext.len() - chunk_idx).min(32);
        for i in 0..chunk_size {
            output.push(ciphertext[chunk_idx + i] ^ mask[i]);
        }
        chunk_idx += chunk_size;
        block_idx = block_idx.wrapping_add(1);
    }
    Ok(output)
}

#[cfg(not(test))]
fn native_get(id: &str) -> keyring::Result<String> {
    Entry::new(SERVICE_NAME, id)?.get_password()
}
#[cfg(not(test))]
fn native_save(id: &str, value: &str) -> keyring::Result<()> {
    Entry::new(SERVICE_NAME, id)?.set_password(value)
}
#[cfg(not(test))]
fn native_delete(id: &str) -> keyring::Result<()> {
    Entry::new(SERVICE_NAME, id)?.delete_credential()
}

// Unit tests must never read or mutate the user's system keychain.
#[cfg(test)]
fn test_vault() -> &'static Mutex<HashMap<String, String>> {
    static VAULT: std::sync::OnceLock<Mutex<HashMap<String, String>>> = std::sync::OnceLock::new();
    VAULT.get_or_init(Mutex::default)
}
#[cfg(test)]
thread_local! { static FAIL_WRITES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
fn native_get(id: &str) -> keyring::Result<String> {
    test_vault()
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or(keyring::Error::NoEntry)
}
#[cfg(test)]
fn native_save(id: &str, value: &str) -> keyring::Result<()> {
    if FAIL_WRITES.get() {
        return Err(keyring::Error::NoDefaultStore);
    }
    test_vault().lock().unwrap().insert(id.into(), value.into());
    Ok(())
}
#[cfg(test)]
fn native_delete(id: &str) -> keyring::Result<()> {
    test_vault().lock().unwrap().remove(id);
    Ok(())
}

fn migrate_file(path: &Path) -> Result<()> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).context("Cannot read legacy credential file; original retained"),
    };
    let data = decrypt_payload(&bytes, &derive_machine_key())
        .context("Cannot decrypt legacy credentials; original retained. Restore the original machine environment or recover credentials manually")?;
    let values: HashMap<String, String> = serde_json::from_slice(&data)
        .context("Invalid legacy credential file; original retained")?;
    for (id, value) in values {
        match native_get(&id) {
            Ok(existing) => {
                anyhow::ensure!(
                    existing == value,
                    "Legacy and native credentials conflict for {id}; both retained. Resolve the conflict before migration"
                );
            }
            Err(keyring::Error::NoEntry) => {
                native_save(&id, &value)
                    .context("Credential migration failed; original retained")?;
                anyhow::ensure!(
                    native_get(&id)? == value,
                    "Credential migration verification failed; original retained"
                );
            }
            Err(e) => {
                return Err(e).context("Cannot access native credential store; original retained");
            }
        }
    }
    std::fs::remove_file(path)
        .context("Credentials migrated but insecure legacy file could not be removed")?;
    Ok(())
}

pub struct CredentialVault;
impl CredentialVault {
    pub fn is_native_keyring_available() -> bool {
        Entry::store_status().is_ok()
    }
    pub fn backend_type() -> VaultBackendType {
        if Self::is_native_keyring_available() {
            VaultBackendType::NativeKeyring
        } else {
            VaultBackendType::Unavailable
        }
    }
    pub fn status() -> VaultStatus {
        VaultStatus {
            backend: Self::backend_type(),
            native_keyring_available: Self::is_native_keyring_available(),
            encrypted_file_path: legacy_paths()
                .into_iter()
                .find(|p| p.exists())
                .map(|p| p.to_string_lossy().into_owned()),
            memory_cached_count: 0,
        }
    }
    pub fn migrate_legacy() -> Result<()> {
        #[cfg(not(test))]
        {
            let _guard = MIGRATION_LOCK
                .lock()
                .map_err(|_| anyhow::anyhow!("Credential migration lock poisoned"))?;
            for path in legacy_paths() {
                migrate_file(&path)?;
            }
        }
        Ok(())
    }
    pub fn save_secret(id: &str, secret: &str) -> Result<()> {
        Self::migrate_legacy()?;
        native_save(id, secret)
            .context("Failed to persist credential in the system credential store")
    }
    pub fn get_secret(id: &str) -> Result<String> {
        match native_get(id) {
            Ok(value) => Ok(value),
            Err(keyring::Error::NoEntry) => {
                Self::migrate_legacy()?;
                match native_get(id) {
                    Ok(value) => Ok(value),
                    Err(keyring::Error::NoEntry) => Err(MissingCredential.into()),
                    Err(error) => Err(error).context("Failed to read migrated credential"),
                }
            }
            Err(error) => {
                Err(error).context("Failed to read credential from the system credential store")
            }
        }
    }
    pub fn delete_secret(id: &str) -> Result<()> {
        Self::migrate_legacy()?;
        match native_delete(id) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("Failed to delete credential"),
        }
    }
    pub fn has_secret(id: &str) -> bool {
        Self::get_secret(id).is_ok()
    }
    pub fn duplicate_secret(source: &str, target: &str) -> Result<bool> {
        let secret = Self::get_secret(source)?;
        Self::save_secret(target, &secret)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_native_write_is_not_reported_as_saved() {
        let id = uuid::Uuid::new_v4().to_string();
        FAIL_WRITES.set(true);
        let result = CredentialVault::save_secret(&id, "not persisted");
        FAIL_WRITES.set(false);
        assert!(result.is_err());
        assert!(CredentialVault::get_secret(&id).is_err());
    }

    #[test]
    fn legacy_migration_preserves_corruption_and_native_values() {
        let dir = std::env::temp_dir().join(format!("redash-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("credentials.enc");
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(migrate_file(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
        let id = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        native_save(&id, "new").unwrap();
        let data = serde_json::to_vec(&HashMap::from([
            (id.clone(), "old"),
            (second.clone(), "legacy"),
        ]))
        .unwrap();
        std::fs::write(&path, encrypt_payload(&data, &derive_machine_key())).unwrap();
        assert!(migrate_file(&path).is_err());
        assert!(path.exists());
        assert_eq!(native_get(&id).unwrap(), "new");
        let data = serde_json::to_vec(&HashMap::from([
            (id.clone(), "new"),
            (second.clone(), "legacy"),
        ]))
        .unwrap();
        std::fs::write(&path, encrypt_payload(&data, &derive_machine_key())).unwrap();
        migrate_file(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(native_get(&id).unwrap(), "new");
        assert_eq!(native_get(&second).unwrap(), "legacy");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_credentials_are_errors() {
        let error = CredentialVault::get_secret(&uuid::Uuid::new_v4().to_string())
            .unwrap_err()
            .context("Password authentication could not start");
        assert!(error.downcast_ref::<MissingCredential>().is_some());
        assert!(
            CredentialVault::duplicate_secret(&uuid::Uuid::new_v4().to_string(), "unused").is_err()
        );
    }
}
