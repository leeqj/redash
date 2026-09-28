pub mod alert;
pub mod credentials;
pub mod host;
pub mod settings;
pub mod snippet;

pub use alert::*;
pub use credentials::*;
pub use host::*;
pub use settings::*;
pub use snippet::*;

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostStoreError {
    #[error("Host not found: {0:?}")]
    HostNotFound(HostId),
    #[error("Validation failed: {0}")]
    Validation(#[from] HostValidationError),
    #[error("Credential error: {0}")]
    Credential(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct HostStore {
    pub hosts: Vec<HostConfig>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
    #[serde(skip)]
    pub load_error: Option<String>,
}

impl HostStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            hosts: Vec::new(),
            path: Some(path.into()),
            load_error: None,
        }
    }

    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(Self::default_path)
    }

    pub fn default_path() -> PathBuf {
        if let Ok(dir) = std::env::var("REDASH_CONFIG_DIR") {
            return PathBuf::from(dir).join("hosts.json");
        }
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("./.redash"));
        let redash_path = base.join("redash").join("hosts.json");
        if redash_path.exists() {
            return redash_path;
        }
        let legacy_path = base.join("serverbox").join("hosts.json");
        if legacy_path.exists() {
            return legacy_path;
        }
        redash_path
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let data = match fs::read_to_string(path) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::with_path(path));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read host config at {path:?}"));
            }
        };
        let mut store: Self = serde_json::from_str(&data)
            .with_context(|| format!("Failed to parse host config at {:?}", path))?;
        store.set_path(path);
        Ok(store)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(error) = &self.load_error {
            anyhow::bail!("Host configuration is read-only after load failure: {error}");
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }
        let data = serde_json::to_string_pretty(self).context("Failed to serialize host config")?;

        let tmp_file_name = format!(
            ".{}.tmp-{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("hosts"),
            uuid::Uuid::new_v4()
        );
        let tmp_path = path.with_file_name(tmp_file_name);
        if let Err(e) = fs::write(&tmp_path, &data) {
            let _ = fs::remove_file(&tmp_path);
            return Err(anyhow::anyhow!(
                "Failed to write temporary host config to {:?}: {}",
                tmp_path,
                e
            ));
        }
        if let Err(e) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(anyhow::anyhow!(
                "Failed to atomically rename {:?} to {:?}: {}",
                tmp_path,
                path,
                e
            ));
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), HostStoreError> {
        if let Some(error) = &self.load_error {
            return Err(HostStoreError::Io(error.clone()));
        }
        let path = self.path();
        self.save_to_file(&path)
            .map_err(|e| HostStoreError::Io(e.to_string()))
    }

    pub fn add_or_update(&mut self, host: HostConfig) {
        if let Some(idx) = self.hosts.iter().position(|h| h.id == host.id) {
            self.hosts[idx] = host;
        } else {
            self.hosts.push(host);
        }
    }

    fn commit(&mut self, hosts: Vec<HostConfig>) -> Result<(), HostStoreError> {
        let mut candidate = self.clone();
        candidate.hosts = hosts;
        candidate.save()?;
        let old = std::mem::replace(self, candidate);
        for host in &old.hosts {
            if let Some(id) = host.credential_id()
                && !self.hosts.iter().any(|h| h.credential_id() == Some(id))
                && let Err(error) = CredentialVault::delete_secret(id)
            {
                // Configuration is already committed. A cleanup failure must not resurrect hosts.
                log::error!("Host configuration saved, but credential cleanup failed: {error:#}");
            }
        }
        Ok(())
    }

    pub fn replace_hosts(&mut self, hosts: Vec<HostConfig>) -> Result<(), HostStoreError> {
        let mut ids = HashSet::new();
        for host in &hosts {
            host.validate()?;
            if !ids.insert(&host.id) {
                return Err(HostStoreError::Serialization("Duplicate host ID".into()));
            }
        }
        self.commit(hosts)
    }

    pub fn save_host(&mut self, host: HostConfig) -> Result<(), HostStoreError> {
        host.validate()?;
        let mut hosts = self.hosts.clone();
        if let Some(old) = hosts.iter_mut().find(|h| h.id == host.id) {
            *old = host;
        } else {
            hosts.push(host);
        }
        self.commit(hosts)
    }
    pub fn remove(&mut self, id: &HostId) -> Option<HostConfig> {
        self.hosts
            .iter()
            .position(|h| &h.id == id)
            .map(|i| self.hosts.remove(i))
    }
    pub fn find(&self, id: &HostId) -> Option<&HostConfig> {
        self.hosts.iter().find(|h| &h.id == id)
    }
    pub fn find_mut(&mut self, id: &HostId) -> Option<&mut HostConfig> {
        self.hosts.iter_mut().find(|h| &h.id == id)
    }
    pub fn delete_host(&mut self, id: &HostId) -> Result<Option<HostConfig>, HostStoreError> {
        Ok(self
            .remove_batch(std::slice::from_ref(id))?
            .into_iter()
            .next())
    }
    pub fn move_host(&mut self, id: &HostId, up: bool) -> Result<bool, HostStoreError> {
        let pos = self
            .hosts
            .iter()
            .position(|h| &h.id == id)
            .ok_or_else(|| HostStoreError::HostNotFound(id.clone()))?;
        let target = if up {
            pos.saturating_sub(1)
        } else {
            (pos + 1).min(self.hosts.len() - 1)
        };
        if pos == target {
            return Ok(false);
        }
        let mut hosts = self.hosts.clone();
        hosts.swap(pos, target);
        self.commit(hosts)?;
        Ok(true)
    }
    pub fn move_host_to_top(&mut self, id: &HostId) -> Result<bool, HostStoreError> {
        let pos = self
            .hosts
            .iter()
            .position(|h| &h.id == id)
            .ok_or_else(|| HostStoreError::HostNotFound(id.clone()))?;
        if pos == 0 {
            return Ok(false);
        }
        let mut hosts = self.hosts.clone();
        let host = hosts.remove(pos);
        hosts.insert(0, host);
        self.commit(hosts)?;
        Ok(true)
    }
    pub fn reorder_hosts(&mut self, order: &[HostId]) -> Result<(), HostStoreError> {
        let mut hosts = self.hosts.clone();
        hosts.sort_by_key(|host| {
            order
                .iter()
                .position(|id| id == &host.id)
                .unwrap_or(usize::MAX)
        });
        self.commit(hosts)
    }
    pub fn clone_host(&mut self, id: &HostId) -> Result<HostConfig, HostStoreError> {
        let pos = self
            .hosts
            .iter()
            .position(|h| &h.id == id)
            .ok_or_else(|| HostStoreError::HostNotFound(id.clone()))?;
        let mut host = self.hosts[pos].clone();
        host.id = HostId::new();
        host.name = format!(
            "{}-Copy",
            host.name.chars().take(59).collect::<String>().trim_end()
        );
        if let Some(source) = host.credential_id() {
            let target = format!("redash_{}", uuid::Uuid::new_v4());
            CredentialVault::duplicate_secret(source, &target)
                .map_err(|e| HostStoreError::Credential(format!("{e:#}")))?;
            match &mut host.auth {
                AuthMethod::Password { credential_id } => *credential_id = target,
                AuthMethod::PrivateKey { passphrase_id, .. } => *passphrase_id = Some(target),
                AuthMethod::Agent => {}
            }
        }
        host.validate()?;
        let mut hosts = self.hosts.clone();
        hosts.insert(pos + 1, host.clone());
        if let Err(error) = self.commit(hosts) {
            if let Some(id) = host.credential_id() {
                let _ = CredentialVault::delete_secret(id);
            }
            return Err(error);
        }
        Ok(host)
    }
    pub fn remove_batch(&mut self, ids: &[HostId]) -> Result<Vec<HostConfig>, HostStoreError> {
        let ids: HashSet<_> = ids.iter().collect();
        let (removed, retained): (Vec<_>, Vec<_>) = self
            .hosts
            .iter()
            .cloned()
            .partition(|h| ids.contains(&h.id));
        if !removed.is_empty() {
            self.commit(retained)?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_persistence_keeps_memory_and_credentials() {
        let dir = std::env::temp_dir().join(format!("redash-transaction-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let blocker = dir.join("not-a-directory");
        fs::write(&blocker, b"blocker").unwrap();
        let mut store = HostStore::with_path(blocker.join("hosts.json"));
        let mut host = HostConfig::new("retained", "localhost", "test");
        let credential = uuid::Uuid::new_v4().to_string();
        CredentialVault::save_secret(&credential, "secret").unwrap();
        host.auth = AuthMethod::Password {
            credential_id: credential.clone(),
        };
        store.add_or_update(host.clone());
        let original = store.clone();
        host.name = "changed".into();
        assert!(store.save_host(host.clone()).is_err());
        assert_eq!(store, original);
        assert!(store.delete_host(&host.id).is_err());
        assert_eq!(store, original);
        assert_eq!(CredentialVault::get_secret(&credential).unwrap(), "secret");
        assert!(store.remove_batch(&[host.id]).is_err());
        assert_eq!(store, original);
        let corrupt = dir.join("corrupt.json");
        fs::write(&corrupt, b"invalid").unwrap();
        assert!(HostStore::load_from_file(&corrupt).is_err());
        assert_eq!(fs::read(&corrupt).unwrap(), b"invalid");
        CredentialVault::delete_secret(&credential).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_host_store_crud_and_serde() {
        let mut store = HostStore::default();
        let mut h1 = HostConfig::new("web-prod-01", "192.168.1.10", "root");
        h1.group = "Production".into();
        let h1_id = h1.id.clone();

        store.add_or_update(h1);
        assert_eq!(store.hosts.len(), 1);
        assert_eq!(store.find(&h1_id).unwrap().name, "web-prod-01");

        // Update existing host
        let mut updated = store.find(&h1_id).unwrap().clone();
        updated.name = "web-prod-01-renamed".into();
        store.add_or_update(updated);
        assert_eq!(store.hosts.len(), 1);
        assert_eq!(store.find(&h1_id).unwrap().name, "web-prod-01-renamed");

        // Remove
        let removed = store.remove(&h1_id);
        assert!(removed.is_some());
        assert_eq!(store.hosts.len(), 0);
    }

    #[test]
    fn test_host_validation_rules() {
        // Valid configs
        let mut valid = HostConfig::new("web-prod-01", "192.168.1.10", "root");
        assert_eq!(valid.validate(), Ok(()));

        valid.hostname = "2001:db8::1".into();
        assert_eq!(valid.validate(), Ok(()));

        valid.hostname = "[::1]".into();
        assert_eq!(valid.validate(), Ok(()));

        valid.hostname = "server.internal.lan".into();
        assert_eq!(valid.validate(), Ok(()));

        // Name validation
        let mut invalid_name = valid.clone();
        invalid_name.name = "".into();
        assert_eq!(invalid_name.validate(), Err(HostValidationError::EmptyName));

        invalid_name.name = "   ".into();
        assert_eq!(invalid_name.validate(), Err(HostValidationError::EmptyName));

        invalid_name.name = "a".repeat(65);
        assert_eq!(
            invalid_name.validate(),
            Err(HostValidationError::NameTooLong)
        );

        let mut boundary_name = valid.clone();
        boundary_name.name = "a".repeat(64);
        assert_eq!(boundary_name.validate(), Ok(()));

        // Hostname validation
        let mut invalid_host = valid.clone();
        invalid_host.hostname = "".into();
        assert_eq!(
            invalid_host.validate(),
            Err(HostValidationError::EmptyHostname)
        );

        invalid_host.hostname = "host name".into();
        assert_eq!(
            invalid_host.validate(),
            Err(HostValidationError::HostnameContainsWhitespace)
        );

        invalid_host.hostname = "-invalid-domain".into();
        assert_eq!(
            invalid_host.validate(),
            Err(HostValidationError::InvalidHostname)
        );

        invalid_host.hostname = "host:22".into();
        assert_eq!(
            invalid_host.validate(),
            Err(HostValidationError::InvalidHostname)
        );

        invalid_host.hostname = "user@domain.com".into();
        assert_eq!(
            invalid_host.validate(),
            Err(HostValidationError::InvalidHostname)
        );

        // Port validation (prevent port 0)
        let mut invalid_port = valid.clone();
        invalid_port.port = 0;
        assert_eq!(invalid_port.validate(), Err(HostValidationError::ZeroPort));

        invalid_port.port = 65535;
        assert_eq!(invalid_port.validate(), Ok(()));

        // User validation
        let mut invalid_user = valid.clone();
        invalid_user.user = "".into();
        assert_eq!(invalid_user.validate(), Err(HostValidationError::EmptyUser));

        invalid_user.user = "   ".into();
        assert_eq!(invalid_user.validate(), Err(HostValidationError::EmptyUser));

        invalid_user.user = "user:name".into();
        assert_eq!(
            invalid_user.validate(),
            Err(HostValidationError::InvalidUser)
        );

        invalid_user.user = "user name".into();
        assert_eq!(
            invalid_user.validate(),
            Err(HostValidationError::InvalidUser)
        );

        // Auth validation
        let mut auth_test = valid.clone();
        auth_test.auth = AuthMethod::Password {
            credential_id: "".into(),
        };
        assert_eq!(
            auth_test.validate(),
            Err(HostValidationError::EmptyCredentialId)
        );

        auth_test.auth = AuthMethod::Password {
            credential_id: "valid_cred_id".into(),
        };
        assert_eq!(auth_test.validate(), Ok(()));

        auth_test.auth = AuthMethod::PrivateKey {
            key_path: PathBuf::from(""),
            passphrase_id: None,
        };
        assert_eq!(auth_test.validate(), Err(HostValidationError::EmptyKeyPath));

        auth_test.auth = AuthMethod::PrivateKey {
            key_path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            passphrase_id: Some("".into()),
        };
        assert_eq!(
            auth_test.validate(),
            Err(HostValidationError::EmptyPassphraseId)
        );

        auth_test.auth = AuthMethod::PrivateKey {
            key_path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            passphrase_id: Some("pass_cred_id".into()),
        };
        assert_eq!(auth_test.validate(), Ok(()));

        // Bandwidth validation
        let mut bw_test = valid.clone();
        bw_test.bandwidth_limit_gb = Some(1000);
        bw_test.bandwidth_reset_day = Some(1);
        assert_eq!(bw_test.validate(), Ok(()));

        bw_test.bandwidth_reset_day = Some(31);
        assert_eq!(bw_test.validate(), Ok(()));

        bw_test.bandwidth_reset_day = Some(0);
        assert_eq!(
            bw_test.validate(),
            Err(HostValidationError::InvalidResetDay)
        );

        bw_test.bandwidth_reset_day = Some(32);
        assert_eq!(
            bw_test.validate(),
            Err(HostValidationError::InvalidResetDay)
        );
    }

    #[test]
    fn test_host_clone_operation() {
        let tmp_dir = std::env::temp_dir().join(format!("sb_test_clone_{}", uuid::Uuid::new_v4()));
        let tmp_file = tmp_dir.join("hosts.json");
        let mut store = HostStore::with_path(&tmp_file);

        let mut original = HostConfig::new("web-prod-01", "10.0.0.1", "deploy");
        original.port = 2222;
        original.group = "Production".into();
        original.tags = vec!["web".into(), "nginx".into()];
        let orig_id = original.id.clone();

        let cred_id = format!("cred_test_{}", orig_id.0);
        CredentialVault::save_secret(&cred_id, "secret_password_123").unwrap();
        original.auth = AuthMethod::Password {
            credential_id: cred_id.clone(),
        };

        store.add_or_update(original.clone());
        store.save().unwrap();

        // Perform clone
        let cloned = store
            .clone_host(&orig_id)
            .expect("clone_host should succeed");

        // Verify clone properties
        assert_ne!(cloned.id, orig_id);
        assert_eq!(cloned.name, "web-prod-01-Copy");
        assert_eq!(cloned.hostname, "10.0.0.1");
        assert_eq!(cloned.port, 2222);
        assert_eq!(cloned.user, "deploy");
        assert_eq!(cloned.group, "Production");
        assert_eq!(cloned.tags, vec!["web", "nginx"]);

        // Verify secret duplication
        if let AuthMethod::Password {
            credential_id: cloned_cred,
        } = &cloned.auth
        {
            assert_ne!(cloned_cred, &cred_id);
            let secret =
                CredentialVault::get_secret(cloned_cred).expect("Duplicated secret must exist");
            assert_eq!(secret, "secret_password_123");
        } else {
            panic!("Cloned auth method should be Password");
        }

        // Verify store has 2 hosts
        assert_eq!(store.hosts.len(), 2);
        assert!(store.find(&orig_id).is_some());
        assert!(store.find(&cloned.id).is_some());

        // Verify file persistence
        let reloaded = HostStore::load_from_file(&tmp_file).expect("File should reload");
        assert_eq!(reloaded.hosts.len(), 2);
        assert_eq!(reloaded.find(&cloned.id).unwrap().name, "web-prod-01-Copy");

        // Verify cloning non-existent host returns HostNotFound
        let fake_id = HostId::new();
        assert!(matches!(
            store.clone_host(&fake_id),
            Err(HostStoreError::HostNotFound(_))
        ));

        // Cleanup
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_host_batch_removal() {
        let tmp_dir = std::env::temp_dir().join(format!("sb_test_batch_{}", uuid::Uuid::new_v4()));
        let tmp_file = tmp_dir.join("hosts.json");
        let mut store = HostStore::with_path(&tmp_file);

        let mut h1 = HostConfig::new("node-01", "10.0.0.1", "root");
        let cred_1 = format!("cred_del_{}", h1.id.0);
        CredentialVault::save_secret(&cred_1, "pass1").unwrap();
        h1.auth = AuthMethod::Password {
            credential_id: cred_1.clone(),
        };

        let mut h2 = HostConfig::new("node-02", "10.0.0.2", "root");
        let cred_2 = format!("cred_del_{}", h2.id.0);
        CredentialVault::save_secret(&cred_2, "pass2").unwrap();
        h2.auth = AuthMethod::Password {
            credential_id: cred_2.clone(),
        };

        let h3 = HostConfig::new("node-03", "10.0.0.3", "root");
        let h4 = HostConfig::new("node-04", "10.0.0.4", "root");

        let id1 = h1.id.clone();
        let id2 = h2.id.clone();
        let id3 = h3.id.clone();
        let id4 = h4.id.clone();

        store.add_or_update(h1);
        store.add_or_update(h2);
        store.add_or_update(h3);
        store.add_or_update(h4);
        store.save().unwrap();

        assert_eq!(store.hosts.len(), 4);

        // Batch remove h1 and h2
        let removed = store
            .remove_batch(&[id1.clone(), id2.clone()])
            .expect("Batch remove must succeed");
        assert_eq!(removed.len(), 2);
        assert_eq!(store.hosts.len(), 2);
        assert!(store.find(&id1).is_none());
        assert!(store.find(&id2).is_none());
        assert!(store.find(&id3).is_some());
        assert!(store.find(&id4).is_some());

        // Verify credentials cleaned up
        assert!(CredentialVault::get_secret(&cred_1).is_err());
        assert!(CredentialVault::get_secret(&cred_2).is_err());

        // Verify persistence
        let reloaded = HostStore::load_from_file(&tmp_file).expect("File must reload");
        assert_eq!(reloaded.hosts.len(), 2);
        assert!(reloaded.find(&id3).is_some());
        assert!(reloaded.find(&id4).is_some());

        // Test empty batch removal
        let empty_res = store
            .remove_batch(&[])
            .expect("Empty batch remove should succeed");
        assert!(empty_res.is_empty());

        // Cleanup
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_host_reordering_and_deletion() {
        let tmp_dir =
            std::env::temp_dir().join(format!("redash_test_reorder_{}", uuid::Uuid::new_v4()));
        let tmp_file = tmp_dir.join("hosts.json");
        let mut store = HostStore::load_from_file(&tmp_file).unwrap();

        let h1 = HostConfig::new("h1", "10.0.0.1", "root");
        let h2 = HostConfig::new("h2", "10.0.0.2", "root");
        let h3 = HostConfig::new("h3", "10.0.0.3", "root");

        let id1 = h1.id.clone();
        let id2 = h2.id.clone();
        let id3 = h3.id.clone();

        store.add_or_update(h1);
        store.add_or_update(h2);
        store.add_or_update(h3);
        store.save().unwrap();

        assert_eq!(store.hosts[0].id, id1);
        assert_eq!(store.hosts[1].id, id2);
        assert_eq!(store.hosts[2].id, id3);

        // Move h2 up
        let moved = store.move_host(&id2, true).unwrap();
        assert!(moved);
        assert_eq!(store.hosts[0].id, id2);
        assert_eq!(store.hosts[1].id, id1);
        assert_eq!(store.hosts[2].id, id3);

        // Move h2 to top (already at top, should return false)
        assert!(!store.move_host_to_top(&id2).unwrap());

        // Move h3 to top
        assert!(store.move_host_to_top(&id3).unwrap());
        assert_eq!(store.hosts[0].id, id3);
        assert_eq!(store.hosts[1].id, id2);
        assert_eq!(store.hosts[2].id, id1);

        // Reorder explicitly
        store
            .reorder_hosts(&[id1.clone(), id3.clone(), id2.clone()])
            .unwrap();
        assert_eq!(store.hosts[0].id, id1);
        assert_eq!(store.hosts[1].id, id3);
        assert_eq!(store.hosts[2].id, id2);

        // Delete single host h3
        let deleted = store.delete_host(&id3).unwrap();
        assert!(deleted.is_some());
        assert_eq!(store.hosts.len(), 2);
        assert_eq!(store.hosts[0].id, id1);
        assert_eq!(store.hosts[1].id, id2);

        let reloaded = HostStore::load_from_file(&tmp_file).unwrap();
        assert_eq!(reloaded.hosts.len(), 2);
        assert!(reloaded.find(&id3).is_none());

        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
