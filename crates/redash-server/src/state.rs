use crate::control_plane::registry::ControlPlaneRegistry;
use redash_core::config::{AppSettings, AppSettingsExt, HostConfig, HostId, HostStore};
use redash_core::probe::NodeMetrics;
use redash_core::session::SessionManager;
use std::collections::HashMap;
use std::sync::{Arc, RwLock as HostLock};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub gateway_auth: crate::auth::GatewayAuth,
    pub session_mgr: Arc<SessionManager>,
    pub host_store: Arc<HostLock<HostStore>>,
    pub app_settings: Arc<RwLock<AppSettings>>,
    pub metrics_cache: Arc<RwLock<HashMap<HostId, NodeMetrics>>>,
    pub control_plane: ControlPlaneRegistry,
}

impl AppState {
    pub fn new() -> Self {
        Self::from_host_path(&HostStore::default_path())
    }

    pub fn from_host_path(path: &std::path::Path) -> Self {
        let store = HostStore::load_from_file(path).unwrap_or_else(|error| {
            let mut store = HostStore::with_path(path);
            store.load_error = Some(format!("Cannot load host configuration: {error:#}"));
            store
        });
        Self::with_host_store(store)
    }

    pub fn with_host_store(store: HostStore) -> Self {
        // The resolver and API use the same committed store, including after edits.
        let host_store = Arc::new(HostLock::new(store));
        let resolver_store = host_store.clone();
        let session_mgr = SessionManager::new().with_host_resolver(Arc::new(move |id| {
            resolver_store.read().unwrap().find(id).cloned()
        }));
        let settings =
            AppSettings::load_from_file(&AppSettings::default_path()).unwrap_or_default();

        Self {
            gateway_auth: crate::auth::GatewayAuth::from_env(),
            session_mgr: Arc::new(session_mgr),
            host_store,
            app_settings: Arc::new(RwLock::new(settings)),
            metrics_cache: Arc::new(RwLock::new(HashMap::new())),
            control_plane: ControlPlaneRegistry::new(),
        }
    }

    pub async fn get_host(&self, id: &str) -> Option<HostConfig> {
        let store = self.host_store.read().unwrap();
        store.hosts.iter().find(|h| h.id.0 == id).cloned()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
