use crate::terminal::TerminalGrid;
use redash_types::agent::DetectedAgent;
use redash_types::host::HostConfig;
use redash_types::metrics::NodeMetrics;
use redash_types::settings::AppSettings;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Fleet,
    Terminal,
    Sftp,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchTab {
    Terminal,
    Docker,
    Processes,
    Network,
    Tunnels,
    Snippets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSortField {
    CpuDesc,
    MemDesc,
    PidAsc,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum UserAction {
    SwitchView(ActiveView),
    SelectHost(Option<String>),
    FilterChanged(String),
    OpenAddModal,
    CloseAddModal,
    ModalInput { field: usize, text: String },
    ModalNextField,
    ModalSubmit,
    DeleteHost(String),
    AppendTerminal(String),
    UpdateMetrics { host_id: String, metrics: NodeMetrics },
    SetLocale(String),
    SetTheme(String),
    SwitchWorkbenchTab(WorkbenchTab),
    SetProcessSort(ProcessSortField),
    SetSnippetCategory(String),
    OpenDockerLogs { id: String, name: String },
    CloseDockerLogs,
    SetSnippetOutput(Option<(String, String)>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum UiEffect {
    None,
    FetchHosts,
    SaveHost(HostConfig),
    DeleteHost(String),
    SendTerminalInput(String),
    SaveSettings,
}

pub struct AppStateMachine {
    pub active_view: ActiveView,
    pub hosts: Vec<HostConfig>,
    pub selected_host_id: Option<String>,
    pub filter_query: String,
    pub metrics: HashMap<String, NodeMetrics>,
    pub metrics_history: HashMap<String, Vec<f32>>,
    pub terminal_lines: Vec<String>,
    pub terminal_grid: TerminalGrid,
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
    pub active_workbench_tab: WorkbenchTab,
    pub process_sort_by: ProcessSortField,
    pub selected_snippet_category: String,
    pub docker_log_modal: Option<(String, String)>,
    pub snippet_output: Option<(String, String)>,
}

impl Default for AppStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl AppStateMachine {
    pub fn new() -> Self {
        let mut grid = TerminalGrid::new(120, 40);
        grid.write_stream("ReDash Web Terminal [Version 0.1.0-beta]\r\nConnected to ReDash Web Gateway over high-performance WebSocket PTY.\r\n\r\n");

        Self {
            active_view: ActiveView::Fleet,
            hosts: Vec::new(),
            selected_host_id: None,
            filter_query: String::new(),
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
            active_workbench_tab: WorkbenchTab::Terminal,
            process_sort_by: ProcessSortField::CpuDesc,
            selected_snippet_category: "All".to_string(),
            docker_log_modal: None,
            snippet_output: None,
        }
    }

    pub fn handle_action(&mut self, action: UserAction) -> Vec<UiEffect> {
        let mut effects = Vec::new();

        match action {
            UserAction::SwitchView(view) => {
                self.active_view = view;
            }
            UserAction::SelectHost(id) => {
                self.selected_host_id = id;
            }
            UserAction::FilterChanged(q) => {
                self.filter_query = q;
            }
            UserAction::OpenAddModal => {
                self.show_add_modal = true;
                self.modal_name.clear();
                self.modal_hostname.clear();
                self.modal_port = "22".to_string();
                self.modal_user = "root".to_string();
                self.modal_field_idx = 0;
            }
            UserAction::CloseAddModal => {
                self.show_add_modal = false;
            }
            UserAction::ModalInput { field, text } => match field {
                0 => self.modal_name = text,
                1 => self.modal_hostname = text,
                2 => self.modal_port = text,
                3 => self.modal_user = text,
                _ => {}
            },
            UserAction::ModalNextField => {
                self.modal_field_idx = (self.modal_field_idx + 1) % 4;
            }
            UserAction::ModalSubmit => {
                if !self.modal_name.is_empty() && !self.modal_hostname.is_empty() {
                    let port = self.modal_port.parse::<u16>().unwrap_or(22);
                    let mut host = HostConfig::new(
                        &self.modal_name,
                        &self.modal_hostname,
                        &self.modal_user,
                    );
                    host.port = port;
                    effects.push(UiEffect::SaveHost(host));
                    self.show_add_modal = false;
                }
            }
            UserAction::DeleteHost(id) => {
                self.hosts.retain(|h| h.id.0 != id);
                effects.push(UiEffect::DeleteHost(id));
            }
            UserAction::AppendTerminal(text) => {
                self.terminal_grid.write_stream(&text);
                for line in text.split('\n') {
                    let clean = line.trim_end_matches('\r').to_string();
                    self.terminal_lines.push(clean);
                    if self.terminal_lines.len() > 500 {
                        self.terminal_lines.remove(0);
                    }
                }
            }
            UserAction::UpdateMetrics { host_id, metrics } => {
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
            UserAction::SetLocale(locale_code) => {
                self.settings.language = locale_code;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetTheme(theme_name) => {
                self.settings.theme_name = theme_name;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SwitchWorkbenchTab(tab) => {
                self.active_workbench_tab = tab;
            }
            UserAction::SetProcessSort(sort) => {
                self.process_sort_by = sort;
            }
            UserAction::SetSnippetCategory(cat) => {
                self.selected_snippet_category = cat;
            }
            UserAction::OpenDockerLogs { id, name } => {
                self.docker_log_modal = Some((id, name));
            }
            UserAction::CloseDockerLogs => {
                self.docker_log_modal = None;
            }
            UserAction::SetSnippetOutput(output) => {
                self.snippet_output = output;
            }
        }

        effects
    }

    pub fn switch_view(&mut self, view: ActiveView) {
        self.handle_action(UserAction::SwitchView(view));
    }

    pub fn switch_workbench_tab(&mut self, tab: WorkbenchTab) {
        self.handle_action(UserAction::SwitchWorkbenchTab(tab));
    }

    pub fn set_process_sort(&mut self, sort: ProcessSortField) {
        self.handle_action(UserAction::SetProcessSort(sort));
    }

    pub fn set_snippet_category(&mut self, cat: &str) {
        self.handle_action(UserAction::SetSnippetCategory(cat.to_string()));
    }

    pub fn open_docker_logs(&mut self, id: String, name: String) {
        self.handle_action(UserAction::OpenDockerLogs { id, name });
    }

    pub fn close_docker_logs(&mut self) {
        self.handle_action(UserAction::CloseDockerLogs);
    }

    pub fn set_snippet_output(&mut self, output: Option<(String, String)>) {
        self.handle_action(UserAction::SetSnippetOutput(output));
    }

    pub fn update_metrics(&mut self, host_id: String, metrics: NodeMetrics) {
        self.handle_action(UserAction::UpdateMetrics { host_id, metrics });
    }

    pub fn append_terminal_output(&mut self, text: &str) {
        self.handle_action(UserAction::AppendTerminal(text.to_string()));
    }

    pub fn open_add_modal(&mut self) {
        self.handle_action(UserAction::OpenAddModal);
    }

    pub fn close_add_modal(&mut self) {
        self.handle_action(UserAction::CloseAddModal);
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

        let mut host = HostConfig::new(name, hostname, user);
        host.port = port;
        host.tags = vec!["web".to_string()];
        Some(host)
    }

    pub fn t<'a>(&self, key: &'a str) -> &'a str {
        let loc = crate::i18n::Locale::from_code(&self.settings.language).unwrap_or(crate::i18n::Locale::ZhCn);
        crate::i18n::lookup_in_locale(loc, key).unwrap_or(key)
    }

    pub fn current_palette(&self) -> &'static crate::theme::ThemePalette {
        crate::theme::get_palette(&self.settings.theme_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_transitions() {
        let mut sm = AppStateMachine::new();
        assert_eq!(sm.active_view, ActiveView::Fleet);

        let effects = sm.handle_action(UserAction::SwitchView(ActiveView::Terminal));
        assert_eq!(sm.active_view, ActiveView::Terminal);
        assert!(effects.is_empty());

        let _ = sm.handle_action(UserAction::OpenAddModal);
        assert!(sm.show_add_modal);

        let _ = sm.handle_action(UserAction::ModalInput {
            field: 0,
            text: "Prod Server".to_string(),
        });
        let _ = sm.handle_action(UserAction::ModalInput {
            field: 1,
            text: "192.168.1.50".to_string(),
        });
        let effects = sm.handle_action(UserAction::ModalSubmit);

        assert!(!sm.show_add_modal);
        assert_eq!(effects.len(), 1);
        if let UiEffect::SaveHost(host) = &effects[0] {
            assert_eq!(host.name, "Prod Server");
            assert_eq!(host.hostname, "192.168.1.50");
        } else {
            panic!("Expected SaveHost effect");
        }
    }

    #[test]
    fn test_workbench_state_and_actions() {
        let mut sm = AppStateMachine::new();
        // Check defaults
        assert_eq!(sm.active_workbench_tab, WorkbenchTab::Terminal);
        assert_eq!(sm.process_sort_by, ProcessSortField::CpuDesc);
        assert_eq!(sm.selected_snippet_category, "All");
        assert_eq!(sm.docker_log_modal, None);
        assert_eq!(sm.snippet_output, None);

        // Switch workbench tabs
        let effects = sm.handle_action(UserAction::SwitchWorkbenchTab(WorkbenchTab::Docker));
        assert!(effects.is_empty());
        assert_eq!(sm.active_workbench_tab, WorkbenchTab::Docker);

        sm.switch_workbench_tab(WorkbenchTab::Processes);
        assert_eq!(sm.active_workbench_tab, WorkbenchTab::Processes);

        sm.switch_workbench_tab(WorkbenchTab::Network);
        assert_eq!(sm.active_workbench_tab, WorkbenchTab::Network);

        sm.switch_workbench_tab(WorkbenchTab::Tunnels);
        assert_eq!(sm.active_workbench_tab, WorkbenchTab::Tunnels);

        sm.switch_workbench_tab(WorkbenchTab::Snippets);
        assert_eq!(sm.active_workbench_tab, WorkbenchTab::Snippets);

        // Process sort fields
        sm.handle_action(UserAction::SetProcessSort(ProcessSortField::MemDesc));
        assert_eq!(sm.process_sort_by, ProcessSortField::MemDesc);

        sm.set_process_sort(ProcessSortField::PidAsc);
        assert_eq!(sm.process_sort_by, ProcessSortField::PidAsc);

        // Snippet category
        sm.handle_action(UserAction::SetSnippetCategory("Docker".to_string()));
        assert_eq!(sm.selected_snippet_category, "Docker");

        sm.set_snippet_category("Network");
        assert_eq!(sm.selected_snippet_category, "Network");

        // Docker log modal
        sm.handle_action(UserAction::OpenDockerLogs {
            id: "c-1234567890".to_string(),
            name: "nginx-proxy".to_string(),
        });
        assert_eq!(
            sm.docker_log_modal,
            Some(("c-1234567890".to_string(), "nginx-proxy".to_string()))
        );

        sm.close_docker_logs();
        assert_eq!(sm.docker_log_modal, None);

        // Snippet output
        sm.set_snippet_output(Some(("Test Title".to_string(), "Success output".to_string())));
        assert_eq!(
            sm.snippet_output,
            Some(("Test Title".to_string(), "Success output".to_string()))
        );

        sm.set_snippet_output(None);
        assert_eq!(sm.snippet_output, None);
    }
}
