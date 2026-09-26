use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct HostId(pub String);

impl HostId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for HostId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for HostId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AuthMethod {
    Password {
        /// Key in OS keyring or reference
        credential_id: String,
    },
    PrivateKey {
        key_path: PathBuf,
        passphrase_id: Option<String>,
    },
    #[default]
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TargetOs {
    #[default]
    Linux,
    Darwin,
    Windows,
    Unknown,
}

fn default_group() -> String {
    "Default".to_string()
}

fn default_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostConfig {
    pub id: HostId,
    pub name: String,
    pub hostname: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub auth: AuthMethod,
    #[serde(default = "default_group")]
    pub group: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub target_os: TargetOs,
    /// Jump host / Bastion host id if chained
    #[serde(default)]
    pub jump_host: Option<HostId>,
    #[serde(default)]
    pub proxy_jump_id: Option<HostId>,
    #[serde(default)]
    pub bandwidth_limit_gb: Option<u64>,
    #[serde(default)]
    pub bandwidth_reset_day: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum HostValidationError {
    #[error("Host name cannot be empty")]
    EmptyName,
    #[error("Host name exceeds maximum length of 64 characters")]
    NameTooLong,
    #[error("Hostname cannot be empty")]
    EmptyHostname,
    #[error("Hostname contains whitespace")]
    HostnameContainsWhitespace,
    #[error("Invalid hostname or IP address")]
    InvalidHostname,
    #[error("Port cannot be 0")]
    ZeroPort,
    #[error("User cannot be empty")]
    EmptyUser,
    #[error("User contains invalid characters (whitespace or colon)")]
    InvalidUser,
    #[error("Password credential ID cannot be empty")]
    EmptyCredentialId,
    #[error("Private key path cannot be empty")]
    EmptyKeyPath,
    #[error("Passphrase credential ID cannot be empty")]
    EmptyPassphraseId,
    #[error("Bandwidth reset day must be between 1 and 31")]
    InvalidResetDay,
}

fn is_valid_hostname_or_ip(raw_host: &str) -> bool {
    let host = raw_host.trim();
    if host.is_empty() || host.len() > 253 {
        return false;
    }

    let unbracketed = if host.starts_with('[') && host.ends_with(']') && host.len() > 2 {
        &host[1..host.len() - 1]
    } else {
        host
    };

    if unbracketed.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }

    if host.contains(':') || host.contains('/') || host.contains('@') {
        return false;
    }

    let labels: Vec<&str> = host.split('.').collect();
    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return false;
        }
    }

    true
}

impl HostConfig {
    pub fn proxy_id(&self) -> Option<&HostId> {
        self.proxy_jump_id.as_ref().or(self.jump_host.as_ref())
    }

    pub fn credential_id(&self) -> Option<&str> {
        match &self.auth {
            AuthMethod::Password { credential_id } => Some(credential_id),
            AuthMethod::PrivateKey { passphrase_id, .. } => passphrase_id.as_deref(),
            AuthMethod::Agent => None,
        }
    }

    pub fn same_connection(&self, other: &Self) -> bool {
        self.id == other.id
            && self.hostname == other.hostname
            && self.port == other.port
            && self.user == other.user
            && self.auth == other.auth
            && self.proxy_id() == other.proxy_id()
    }

    pub fn new(
        name: impl Into<String>,
        hostname: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        Self {
            id: HostId::new(),
            name: name.into(),
            hostname: hostname.into(),
            port: 22,
            user: user.into(),
            auth: AuthMethod::Agent,
            group: "Default".to_string(),
            tags: Vec::new(),
            target_os: TargetOs::Linux,
            jump_host: None,
            proxy_jump_id: None,
            bandwidth_limit_gb: None,
            bandwidth_reset_day: None,
        }
    }

    pub fn validate(&self) -> Result<(), HostValidationError> {
        let trimmed_name = self.name.trim();
        if trimmed_name.is_empty() {
            return Err(HostValidationError::EmptyName);
        }
        if trimmed_name.chars().count() > 64 {
            return Err(HostValidationError::NameTooLong);
        }

        if self.hostname.is_empty() {
            return Err(HostValidationError::EmptyHostname);
        }
        if self.hostname.contains(char::is_whitespace) {
            return Err(HostValidationError::HostnameContainsWhitespace);
        }
        if !is_valid_hostname_or_ip(&self.hostname) {
            return Err(HostValidationError::InvalidHostname);
        }

        if self.port == 0 {
            return Err(HostValidationError::ZeroPort);
        }

        let trimmed_user = self.user.trim();
        if trimmed_user.is_empty() {
            return Err(HostValidationError::EmptyUser);
        }
        if self.user.contains(char::is_whitespace) || self.user.contains(':') {
            return Err(HostValidationError::InvalidUser);
        }
        if !self
            .user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(HostValidationError::InvalidUser);
        }

        match &self.auth {
            AuthMethod::Password { credential_id } => {
                if credential_id.trim().is_empty() {
                    return Err(HostValidationError::EmptyCredentialId);
                }
            }
            AuthMethod::PrivateKey {
                key_path,
                passphrase_id,
            } => {
                if key_path.to_string_lossy().trim().is_empty() {
                    return Err(HostValidationError::EmptyKeyPath);
                }
                if let Some(pass_id) = passphrase_id
                    && pass_id.trim().is_empty()
                {
                    return Err(HostValidationError::EmptyPassphraseId);
                }
            }
            AuthMethod::Agent => {}
        }

        if let Some(day) = self.bandwidth_reset_day
            && !(1..=31).contains(&day)
        {
            return Err(HostValidationError::InvalidResetDay);
        }

        Ok(())
    }
}
