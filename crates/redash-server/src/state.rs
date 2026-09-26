use redash_core::config::{AppSettings, HostConfig, HostId, HostStore};
use redash_core::probe::NodeMetrics;
use redash_core::session::SessionManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub session_mgr: Arc<SessionManager>,
    pub host_store: Arc<RwLock<HostStore>>,
    pub app_settings: Arc<RwLock<AppSettings>>,
    pub metrics_cache: Arc<RwLock<HashMap<HostId, NodeMetrics>>>,
}

impl AppState {
    pub fn new() -> Self {
        let store = HostStore::load_from_file(&HostStore::default_path())
            .unwrap_or_else(|_| HostStore::new());
        let settings = AppSettings::load_from_file(&AppSettings::default_path())
            .unwrap_or_default();

        Self {
            session_mgr: Arc::new(SessionManager::new()),
            host_store: Arc::new(RwLock::new(store)),
            app_settings: Arc::new(RwLock::new(settings)),
            metrics_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_host(&self, id: &str) -> Option<HostConfig> {
        let store = self.host_store.read().await;
        store.hosts.iter().find(|h| h.id.0 == id).cloned()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
