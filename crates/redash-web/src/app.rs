use crate::models::{AppSettings, DetectedAgent, HostConfig, HostId, NodeMetrics};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Fleet,
    Terminal,
    Sftp,
    Settings,
}

pub struct AppState {
    pub active_view: ActiveView,
    pub hosts: Vec<HostConfig>,
    pub selected_host_id: Option<String>,
    pub metrics: HashMap<String, NodeMetrics>,
    pub metrics_history: HashMap<String, Vec<f32>>,
    pub terminal_lines: Vec<String>,
    pub terminal_grid: redash_ui_core::terminal::TerminalGrid,
    pub agent: Option<DetectedAgent>,
    pub settings: AppSettings,
    pub show_add_modal: bool,
    pub modal_name: String,
    pub modal_hostname: String,
    pub modal_port: String,
    pub modal_user: String,
    pub modal_field_idx: usize,
    pub is_connected: bool,
    pub scroll_y: f64,
}

impl AppState {
    pub fn new() -> Self {
        let mut grid = redash_ui_core::terminal::TerminalGrid::new(120, 40);
        grid.write_stream("ReDash Web Terminal [Version 0.1.0-beta]\r\nConnected to ReDash Web Gateway over high-performance WebSocket PTY.\r\n\r\n");

        Self {
            active_view: ActiveView::Fleet,
            hosts: Vec::new(),
            selected_host_id: None,
            metrics: HashMap::new(),
            metrics_history: HashMap::new(),
            terminal_lines: vec![
                "ReDash Web Terminal [Version 0.1.0-beta]".to_string(),
                "Connected to ReDash Web Gateway over high-performance WebSocket PTY.".to_string(),
                "".to_string(),
            ],
            terminal_grid: grid,
            agent: None,
            settings: AppSettings::default(),
            show_add_modal: false,
            modal_name: String::new(),
            modal_hostname: String::new(),
            modal_port: "22".to_string(),
            modal_user: "root".to_string(),
            modal_field_idx: 0,
            is_connected: true,
            scroll_y: 0.0,
        }
    }

    pub fn switch_view(&mut self, view: ActiveView) {
        self.active_view = view;
    }

    pub fn update_metrics(&mut self, host_id: String, metrics: NodeMetrics) {
        let history = self
            .metrics_history
            .entry(host_id.clone())
            .or_default();
        history.push(metrics.cpu_percent());
        if history.len() > 30 {
            history.remove(0);
        }
        self.metrics.insert(host_id, metrics);
    }

    pub fn append_terminal_output(&mut self, text: &str) {
        self.terminal_grid.write_stream(text);
        for line in text.split('\n') {
            let clean = line.trim_end_matches('\r').to_string();
            self.terminal_lines.push(clean);
            if self.terminal_lines.len() > 500 {
                self.terminal_lines.remove(0);
            }
        }
    }

    pub fn open_add_modal(&mut self) {
        self.show_add_modal = true;
        self.modal_name.clear();
        self.modal_hostname.clear();
        self.modal_port = "22".to_string();
        self.modal_user = "root".to_string();
        self.modal_field_idx = 0;
    }

    pub fn close_add_modal(&mut self) {
        self.show_add_modal = false;
    }

    pub fn build_new_host(&self) -> Option<HostConfig> {
        let name = self.modal_name.trim();
        let hostname = self.modal_hostname.trim();
        if name.is_empty() || hostname.is_empty() {
            return None;
        }

        let port: u16 = self.modal_port.trim().parse().unwrap_or(22);
        let user = if self.modal_user.trim().is_empty() {
            "root".to_string()
        } else {
            self.modal_user.trim().to_string()
        };

        #[cfg(target_arch = "wasm32")]
        let id_str = format!("host_{:x}", (js_sys::Date::now() as u64));
        #[cfg(not(target_arch = "wasm32"))]
        let id_str = format!("host_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());

        let mut host = HostConfig::new(name, hostname, user);
        host.id = HostId(id_str);
        host.port = port;
        host.tags = vec!["web".to_string()];
        Some(host)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_initial() {
        let state = AppState::new();
        assert_eq!(state.active_view, ActiveView::Fleet);
        assert!(state.hosts.is_empty());
        assert!(state.selected_host_id.is_none());
        assert!(!state.terminal_lines.is_empty());
        assert!(!state.show_add_modal);
    }

    #[test]
    fn test_switch_view() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Terminal);
        assert_eq!(state.active_view, ActiveView::Terminal);
        state.switch_view(ActiveView::Settings);
        assert_eq!(state.active_view, ActiveView::Settings);
    }

    #[test]
    fn test_metrics_history_buffer() {
        let mut state = AppState::new();
        let host_id = "srv-1".to_string();

        for i in 0..40 {
            let mut metrics = NodeMetrics::default();
            metrics.cpu.usage_percent = i as f32;
            state.update_metrics(host_id.clone(), metrics);
        }

        let history = state.metrics_history.get(&host_id).unwrap();
        // Buffer max length is capped at 30
        assert_eq!(history.len(), 30);
        assert_eq!(*history.last().unwrap(), 39.0);
    }

    #[test]
    fn test_terminal_buffer_limits() {
        let mut state = AppState::new();
        for i in 0..600 {
            state.append_terminal_output(&format!("Log line {}", i));
        }
        assert!(state.terminal_lines.len() <= 500);
    }

    #[test]
    fn test_build_new_host_validation() {
        let mut state = AppState::new();
        state.open_add_modal();
        assert!(state.show_add_modal);
        assert!(state.build_new_host().is_none());

        state.modal_name = "Edge Server".to_string();
        state.modal_hostname = "10.0.0.5".to_string();
        state.modal_port = "2222".to_string();
        state.modal_user = "deploy".to_string();

        let host = state.build_new_host().expect("should build valid host");
        assert_eq!(host.name, "Edge Server");
        assert_eq!(host.hostname, "10.0.0.5");
        assert_eq!(host.port, 2222);
        assert_eq!(host.user, "deploy");
    }
}
