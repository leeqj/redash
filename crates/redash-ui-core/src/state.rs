use crate::terminal::TerminalGrid;
use redash_types::agent::DetectedAgent;
use redash_types::batch::BatchJobResult;
use redash_types::host::HostConfig;
use redash_types::metrics::NodeMetrics;
use redash_types::settings::AppSettings;
use redash_types::sftp::RemoteFileItem;
use std::collections::{HashMap, HashSet};

pub use redash_types::metrics::{CardMetricType, ChartTimeRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Fleet,
    Terminal,
    Batch,
    Sftp,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    Appearance,
    Terminal,
    Probe,
    Alerts,
    Backup,
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
    ModalInput {
        field: usize,
        text: String,
    },
    ModalNextField,
    ModalSubmit,
    DeleteHost(String),
    AppendTerminal(String),
    UpdateMetrics {
        host_id: String,
        metrics: NodeMetrics,
    },
    SetLocale(String),
    SetTheme(String),
    SwitchWorkbenchTab(WorkbenchTab),
    SetProcessSort(ProcessSortField),
    SetSnippetCategory(String),
    OpenDockerLogs {
        id: String,
        name: String,
    },
    CloseDockerLogs,
    SetSnippetOutput(Option<(String, String)>),
    SetSftpPath(String),
    SetSftpFiles(Vec<RemoteFileItem>),
    SetSftpLoading(bool),
    SetSftpError(Option<String>),
    OpenSftpEditor {
        path: String,
        content: String,
    },
    UpdateSftpEditorContent(String),
    CloseSftpEditor,
    RequestReadSftpFile(String),
    RequestSaveSftpFile,
    SwitchSettingsCategory(SettingsCategory),
    SetProbeInterval(u64),
    SetPingTarget(String),
    SetCpuThreshold(Option<f32>),
    SetMemThreshold(Option<f32>),
    SetDiskThreshold(Option<f32>),
    SetWebhookUrl(Option<String>),
    ToggleGlow,
    ResetSettings,
    SetTerminalFontSize(f32),
    SetTerminalCursorStyle(String),
    SetTerminalFontFamily(String),
    SetTerminalScrollback(usize),
    SetSettingsSaveStatus(Option<(String, bool)>),
    ToggleBatchHost(String),
    SelectAllBatchHosts,
    ClearBatchHosts,
    SetBatchCommand(String),
    SetBatchRunning(bool),
    SetBatchResults(BatchJobResult),
    SelectBatchLogHost(Option<String>),
    ToggleTerminalSearch,
    SetTerminalSearchQuery(String),
    CloseTerminalSearch,
    TriggerRunBatch,
    SetHoverPos(Option<(f64, f64)>),
    SetFilterFocused(bool),
    SetCardMetric {
        host_id: String,
        metric: CardMetricType,
    },
    SetHoveredChartPoint {
        host_id: String,
        point: Option<usize>,
    },
    ClearHoveredChartPoint(String),
    SetHostChartTimeRange {
        host_id: String,
        range: ChartTimeRange,
    },
    SetModalIsTesting(bool),
    SetModalTestStatus(Option<(String, bool)>),
    ResizeTerminal {
        cols: usize,
        rows: usize,
    },
    // Control Plane Actions
    UpdateControlPlaneNodes(Vec<redash_types::ManagedNodeDetail>),
    ReceiveAgentTelemetry(redash_types::AgentTelemetry),
    SetClientKeypair {
        public_key: String,
        private_key: String,
    },
    TriggerRemediation {
        node_id: String,
        action: redash_types::RemediationAction,
    },
    ActionExecutionCompleted(redash_types::ActionResult),
    ToggleAgentEnrollModal,
    SetEnrollNodeId(String),
    SetEnrollToken(String),
    SetEnrollTab(usize),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum UiEffect {
    None,
    FetchHosts,
    ResizeTerminal {
        cols: u16,
        rows: u16,
    },
    SaveHost(HostConfig),
    DeleteHost(String),
    SendTerminalInput(String),
    SaveSettings,
    TestHostConnection {
        host: HostConfig,
        password: Option<String>,
    },
    FetchSftpList {
        host_id: String,
        path: String,
    },
    ReadSftpFile {
        host_id: String,
        path: String,
    },
    SaveSftpFile {
        host_id: String,
        path: String,
        content: String,
    },
    RunBatch {
        host_ids: Vec<String>,
        command: String,
    },
    DispatchControlPlaneAction {
        signed_action: redash_types::SignedAction,
    },
    FetchControlPlaneNodes,
}

pub struct AppStateMachine {
    pub active_view: ActiveView,
    pub hosts: Vec<HostConfig>,
    pub selected_host_id: Option<String>,
    pub filter_query: String,
    pub metrics: HashMap<String, NodeMetrics>,
    pub metrics_history: HashMap<String, Vec<f32>>,
    pub cpu_histories: HashMap<String, Vec<f32>>,
    pub mem_histories: HashMap<String, Vec<f32>>,
    pub disk_histories: HashMap<String, Vec<f32>>,
    pub active_card_metrics: HashMap<String, CardMetricType>,
    pub active_time_ranges: HashMap<String, ChartTimeRange>,
    pub hovered_chart_points: HashMap<String, usize>,
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
    pub modal_is_testing: bool,
    pub modal_test_status: Option<(String, bool)>,
    pub active_workbench_tab: WorkbenchTab,
    pub process_sort_by: ProcessSortField,
    pub selected_snippet_category: String,
    pub docker_log_modal: Option<(String, String)>,
    pub snippet_output: Option<(String, String)>,
    pub sftp_current_path: String,
    pub sftp_files: Vec<RemoteFileItem>,
    pub sftp_loading: bool,
    pub sftp_error: Option<String>,
    pub sftp_editor: Option<(String, String)>,
    pub sftp_editor_modified: bool,
    pub active_settings_category: SettingsCategory,
    pub settings_save_status: Option<(String, bool)>,
    pub ping_target: String,
    pub batch_selected_host_ids: HashSet<String>,
    pub batch_command: String,
    pub batch_is_running: bool,
    pub batch_results: Option<BatchJobResult>,
    pub batch_selected_log_host: Option<String>,
    pub terminal_search_active: bool,
    pub terminal_search_query: String,
    pub terminal_search_match_count: usize,
    pub hover_pos: Option<(f64, f64)>,
    pub is_filter_focused: bool,
    pub control_plane_nodes: Vec<redash_types::ManagedNodeDetail>,
    pub control_plane_telemetries: HashMap<String, redash_types::AgentTelemetry>,
    pub client_keypair: Option<(String, String)>,
    pub pending_action: Option<(String, redash_types::RemediationAction)>,
    pub last_action_result: Option<redash_types::ActionResult>,
    pub show_agent_enroll_modal: bool,
    pub enroll_hub_url: String,
    pub enroll_node_id_input: String,
    pub enroll_token_input: String,
    pub enroll_tab: usize,
}

impl Default for AppStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl AppStateMachine {
    pub fn new() -> Self {
        let mut grid = TerminalGrid::new(120, 40);
        grid.write_stream("ReDash Web Terminal [Version 0.1.1-beta]\r\nConnected to ReDash Web Gateway over high-performance WebSocket PTY.\r\n\r\n");

        Self {
            active_view: ActiveView::Fleet,
            hosts: Vec::new(),
            selected_host_id: None,
            filter_query: String::new(),
            metrics: HashMap::new(),
            metrics_history: HashMap::new(),
            cpu_histories: HashMap::new(),
            mem_histories: HashMap::new(),
            disk_histories: HashMap::new(),
            active_card_metrics: HashMap::new(),
            active_time_ranges: HashMap::new(),
            hovered_chart_points: HashMap::new(),
            terminal_lines: vec![
                "ReDash Web Terminal [Version 0.1.1-beta]".to_string(),
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
            modal_is_testing: false,
            modal_test_status: None,
            active_workbench_tab: WorkbenchTab::Terminal,
            process_sort_by: ProcessSortField::CpuDesc,
            selected_snippet_category: "All".to_string(),
            docker_log_modal: None,
            snippet_output: None,
            sftp_current_path: "/".to_string(),
            sftp_files: Vec::new(),
            sftp_loading: false,
            sftp_error: None,
            sftp_editor: None,
            sftp_editor_modified: false,
            active_settings_category: SettingsCategory::Appearance,
            settings_save_status: None,
            ping_target: "1.1.1.1".to_string(),
            batch_selected_host_ids: HashSet::new(),
            batch_command: "uptime".to_string(),
            batch_is_running: false,
            batch_results: None,
            batch_selected_log_host: None,
            terminal_search_active: false,
            terminal_search_query: String::new(),
            terminal_search_match_count: 0,
            hover_pos: None,
            is_filter_focused: false,
            control_plane_nodes: Vec::new(),
            control_plane_telemetries: HashMap::new(),
            client_keypair: None,
            pending_action: None,
            last_action_result: None,
            show_agent_enroll_modal: false,
            enroll_hub_url: "ws://127.0.0.1:8080/v1/agent/ws".to_string(),
            enroll_node_id_input: String::new(),
            enroll_token_input: "default-token".to_string(),
            enroll_tab: 0,
        }
    }

    pub fn handle_action(&mut self, action: UserAction) -> Vec<UiEffect> {
        let mut effects = Vec::new();

        match action {
            UserAction::SwitchView(view) => {
                self.active_view = view;
                if (view == ActiveView::Terminal || view == ActiveView::Sftp)
                    && self.selected_host_id.is_none()
                {
                    self.selected_host_id = self.hosts.first().map(|h| h.id.0.clone());
                }
                if view == ActiveView::Sftp
                    && self.sftp_files.is_empty()
                    && !self.sftp_loading
                    && let Some(host_id) = &self.selected_host_id
                {
                    self.sftp_loading = true;
                    effects.push(UiEffect::FetchSftpList {
                        host_id: host_id.clone(),
                        path: self.sftp_current_path.clone(),
                    });
                }
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
                self.modal_is_testing = false;
                self.modal_test_status = None;
            }
            UserAction::CloseAddModal => {
                self.show_add_modal = false;
                self.modal_is_testing = false;
                self.modal_test_status = None;
            }
            UserAction::ModalInput { field, text } => {
                self.modal_test_status = None;
                match field {
                    0 => self.modal_name = text,
                    1 => self.modal_hostname = text,
                    2 => self.modal_port = text,
                    3 => self.modal_user = text,
                    _ => {}
                }
            }
            UserAction::ModalNextField => {
                self.modal_field_idx = (self.modal_field_idx + 1) % 4;
            }
            UserAction::ModalSubmit => {
                if !self.modal_name.is_empty() && !self.modal_hostname.is_empty() {
                    let port = self.modal_port.parse::<u16>().unwrap_or(22);
                    let mut host =
                        HostConfig::new(&self.modal_name, &self.modal_hostname, &self.modal_user);
                    host.port = port;
                    effects.push(UiEffect::SaveHost(host));
                    self.show_add_modal = false;
                    self.modal_is_testing = false;
                    self.modal_test_status = None;
                }
            }
            UserAction::DeleteHost(id) => {
                self.hosts.retain(|h| h.id.0 != id);
                self.metrics.remove(&id);
                self.metrics_history.remove(&id);
                self.cpu_histories.remove(&id);
                self.mem_histories.remove(&id);
                self.disk_histories.remove(&id);
                self.active_card_metrics.remove(&id);
                self.active_time_ranges.remove(&id);
                self.hovered_chart_points.remove(&id);
                self.batch_selected_host_ids.remove(&id);
                if self.selected_host_id.as_deref() == Some(&id) {
                    self.selected_host_id = self.hosts.first().map(|h| h.id.0.clone());
                }
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
                if self.terminal_search_active && !self.terminal_search_query.is_empty() {
                    let q = self.terminal_search_query.to_lowercase();
                    self.terminal_search_match_count = self
                        .terminal_lines
                        .iter()
                        .map(|line| line.to_lowercase().matches(&q).count())
                        .sum();
                }
            }
            UserAction::UpdateMetrics { host_id, metrics } => {
                let cpu_h = self.cpu_histories.entry(host_id.clone()).or_default();
                cpu_h.push(metrics.cpu_percent());
                if cpu_h.len() > 1800 {
                    cpu_h.remove(0);
                }
                let mem_h = self.mem_histories.entry(host_id.clone()).or_default();
                mem_h.push(metrics.mem_percent());
                if mem_h.len() > 1800 {
                    mem_h.remove(0);
                }
                let disk_h = self.disk_histories.entry(host_id.clone()).or_default();
                disk_h.push(metrics.disk_percent());
                if disk_h.len() > 1800 {
                    disk_h.remove(0);
                }
                let history = self.metrics_history.entry(host_id.clone()).or_default();
                history.push(metrics.cpu_percent());
                if history.len() > 1800 {
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
            UserAction::SetSftpPath(path) => {
                self.sftp_current_path = path.clone();
                self.sftp_loading = true;
                self.sftp_error = None;
                if let Some(host_id) = &self.selected_host_id {
                    effects.push(UiEffect::FetchSftpList {
                        host_id: host_id.clone(),
                        path,
                    });
                }
            }
            UserAction::SetSftpFiles(files) => {
                self.sftp_files = files;
                self.sftp_loading = false;
                self.sftp_error = None;
            }
            UserAction::SetSftpLoading(loading) => {
                self.sftp_loading = loading;
            }
            UserAction::SetSftpError(error) => {
                self.sftp_error = error;
                self.sftp_loading = false;
            }
            UserAction::OpenSftpEditor { path, content } => {
                self.sftp_editor = Some((path, content));
                self.sftp_editor_modified = false;
                self.sftp_loading = false;
            }
            UserAction::UpdateSftpEditorContent(content) => {
                if let Some((_, ref mut current_content)) = self.sftp_editor {
                    *current_content = content;
                    self.sftp_editor_modified = true;
                }
            }
            UserAction::CloseSftpEditor => {
                self.sftp_editor = None;
                self.sftp_editor_modified = false;
            }
            UserAction::RequestReadSftpFile(path) => {
                self.sftp_loading = true;
                if let Some(host_id) = &self.selected_host_id {
                    effects.push(UiEffect::ReadSftpFile {
                        host_id: host_id.clone(),
                        path,
                    });
                }
            }
            UserAction::RequestSaveSftpFile => {
                if let (Some(host_id), Some((path, content))) =
                    (&self.selected_host_id, &self.sftp_editor)
                {
                    self.sftp_loading = true;
                    effects.push(UiEffect::SaveSftpFile {
                        host_id: host_id.clone(),
                        path: path.clone(),
                        content: content.clone(),
                    });
                }
            }
            UserAction::SwitchSettingsCategory(cat) => {
                self.active_settings_category = cat;
            }
            UserAction::SetProbeInterval(interval) => {
                self.settings.probe_interval_secs = interval;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetPingTarget(target) => {
                self.ping_target = target;
            }
            UserAction::SetCpuThreshold(threshold) => {
                self.settings.alert_cpu_threshold = threshold.unwrap_or(0.0);
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetMemThreshold(threshold) => {
                self.settings.alert_mem_threshold = threshold.unwrap_or(0.0);
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetDiskThreshold(threshold) => {
                self.settings.alert_disk_threshold = threshold.unwrap_or(0.0);
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetWebhookUrl(url) => {
                self.settings.alert_webhook_url = url;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::ToggleGlow => {
                self.settings.glow_effects_enabled = !self.settings.glow_effects_enabled;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::ResetSettings => {
                self.settings = AppSettings::default();
                self.ping_target = "1.1.1.1".to_string();
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetTerminalFontSize(size) => {
                self.settings.terminal_font_size = size;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetTerminalCursorStyle(style) => {
                self.settings.terminal_cursor_style = style;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetTerminalFontFamily(font) => {
                self.settings.terminal_font_family = font;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetTerminalScrollback(lines) => {
                self.settings.terminal_scrollback_lines = lines;
                effects.push(UiEffect::SaveSettings);
            }
            UserAction::SetSettingsSaveStatus(status) => {
                self.settings_save_status = status;
            }
            UserAction::ToggleBatchHost(id) => {
                if self.batch_selected_host_ids.contains(&id) {
                    self.batch_selected_host_ids.remove(&id);
                } else {
                    self.batch_selected_host_ids.insert(id);
                }
            }
            UserAction::SelectAllBatchHosts => {
                for h in &self.hosts {
                    self.batch_selected_host_ids.insert(h.id.0.clone());
                }
            }
            UserAction::ClearBatchHosts => {
                self.batch_selected_host_ids.clear();
            }
            UserAction::SetBatchCommand(cmd) => {
                self.batch_command = cmd;
            }
            UserAction::SetBatchRunning(running) => {
                self.batch_is_running = running;
            }
            UserAction::SetBatchResults(res) => {
                self.batch_is_running = false;
                self.batch_results = Some(res);
            }
            UserAction::SelectBatchLogHost(host) => {
                self.batch_selected_log_host = host;
            }
            UserAction::ToggleTerminalSearch => {
                self.terminal_search_active = !self.terminal_search_active;
                if !self.terminal_search_active {
                    self.terminal_search_query.clear();
                    self.terminal_search_match_count = 0;
                } else if !self.terminal_search_query.is_empty() {
                    let q = self.terminal_search_query.to_lowercase();
                    self.terminal_search_match_count = self
                        .terminal_lines
                        .iter()
                        .map(|line| line.to_lowercase().matches(&q).count())
                        .sum();
                }
            }
            UserAction::SetTerminalSearchQuery(query) => {
                self.terminal_search_query = query;
                if self.terminal_search_query.is_empty() {
                    self.terminal_search_match_count = 0;
                } else {
                    let q = self.terminal_search_query.to_lowercase();
                    self.terminal_search_match_count = self
                        .terminal_lines
                        .iter()
                        .map(|line| line.to_lowercase().matches(&q).count())
                        .sum();
                }
            }
            UserAction::CloseTerminalSearch => {
                self.terminal_search_active = false;
                self.terminal_search_query.clear();
                self.terminal_search_match_count = 0;
            }
            UserAction::TriggerRunBatch => {
                if !self.batch_selected_host_ids.is_empty() && !self.batch_command.trim().is_empty()
                {
                    self.batch_is_running = true;
                    effects.push(UiEffect::RunBatch {
                        host_ids: self.batch_selected_host_ids.iter().cloned().collect(),
                        command: self.batch_command.clone(),
                    });
                }
            }
            UserAction::SetHoverPos(pos) => {
                self.hover_pos = pos;
            }
            UserAction::SetFilterFocused(focused) => {
                self.is_filter_focused = focused;
            }
            UserAction::SetCardMetric { host_id, metric } => {
                self.active_card_metrics.insert(host_id, metric);
            }
            UserAction::SetHoveredChartPoint { host_id, point } => match point {
                Some(pt) => {
                    self.hovered_chart_points.insert(host_id, pt);
                }
                None => {
                    self.hovered_chart_points.remove(&host_id);
                }
            },
            UserAction::ClearHoveredChartPoint(host_id) => {
                self.hovered_chart_points.remove(&host_id);
            }
            UserAction::SetHostChartTimeRange { host_id, range } => {
                self.active_time_ranges.insert(host_id, range);
            }
            UserAction::SetModalIsTesting(is_testing) => {
                self.modal_is_testing = is_testing;
            }
            UserAction::SetModalTestStatus(status) => {
                self.modal_test_status = status;
            }
            UserAction::ResizeTerminal { cols, rows } => {
                let clamped_cols = cols.clamp(10, u16::MAX as usize);
                let clamped_rows = rows.clamp(5, u16::MAX as usize);
                if self.terminal_grid.cols != clamped_cols
                    || self.terminal_grid.rows != clamped_rows
                {
                    self.terminal_grid.resize(clamped_cols, clamped_rows);
                    effects.push(UiEffect::ResizeTerminal {
                        cols: clamped_cols as u16,
                        rows: clamped_rows as u16,
                    });
                }
            }
            UserAction::UpdateControlPlaneNodes(nodes) => {
                self.control_plane_nodes = nodes;
            }
            UserAction::ReceiveAgentTelemetry(telemetry) => {
                let node_id = telemetry.node_id.clone();
                let cpu_pct = telemetry.cpu_usage_pct;
                let history = self.cpu_histories.entry(format!("cp-{}", node_id)).or_default();
                history.push(cpu_pct);
                if history.len() > 60 {
                    history.remove(0);
                }
                if let Some(node) = self.control_plane_nodes.iter_mut().find(|n| n.node_id == node_id) {
                    node.status = redash_types::NodeOnlineStatus::Online;
                    node.latest_telemetry = Some(telemetry.clone());
                }
                self.control_plane_telemetries.insert(node_id, telemetry);
            }
            UserAction::SetClientKeypair { public_key, private_key } => {
                self.client_keypair = Some((public_key, private_key));
            }
            UserAction::TriggerRemediation { node_id, action } => {
                if let Some((_, ref priv_key)) = self.client_keypair {
                    let now = 1710000000;
                    let nonce = format!("{:x}", now);
                    if let Ok(signed) = crate::control_plane::ClientSigner::sign_action(
                        priv_key,
                        &node_id,
                        action.clone(),
                        now,
                        &nonce,
                    ) {
                        self.pending_action = Some((node_id, action));
                        effects.push(UiEffect::DispatchControlPlaneAction { signed_action: signed });
                    }
                }
            }
            UserAction::ActionExecutionCompleted(result) => {
                self.last_action_result = Some(result);
                self.pending_action = None;
                effects.push(UiEffect::FetchControlPlaneNodes);
            }
            UserAction::ToggleAgentEnrollModal => {
                self.show_agent_enroll_modal = !self.show_agent_enroll_modal;
            }
            UserAction::SetEnrollNodeId(id) => {
                self.enroll_node_id_input = id;
            }
            UserAction::SetEnrollToken(token) => {
                self.enroll_token_input = token;
            }
            UserAction::SetEnrollTab(tab) => {
                self.enroll_tab = tab;
            }
        }

        effects
    }

    pub fn set_card_metric(&mut self, host_id: String, metric: CardMetricType) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetCardMetric { host_id, metric })
    }

    pub fn get_card_metric(&self, host_id: &str) -> CardMetricType {
        self.active_card_metrics
            .get(host_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn set_host_chart_time_range(
        &mut self,
        host_id: String,
        range: ChartTimeRange,
    ) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetHostChartTimeRange { host_id, range })
    }

    pub fn get_host_chart_time_range(&self, host_id: &str) -> ChartTimeRange {
        self.active_time_ranges
            .get(host_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn slice_history_for_range(history: &[f32], range: ChartTimeRange) -> Vec<f32> {
        let max_samples = range.max_samples();
        let slice = if history.len() > max_samples {
            &history[history.len() - max_samples..]
        } else {
            history
        };
        if slice.len() <= 60 {
            slice.to_vec()
        } else {
            let target_points = 60;
            (0..target_points)
                .map(|i| {
                    let start = i * slice.len() / target_points;
                    let end = ((i + 1) * slice.len() / target_points).min(slice.len());
                    if start >= end {
                        slice.get(start).copied().unwrap_or(0.0)
                    } else {
                        let chunk = &slice[start..end];
                        chunk.iter().sum::<f32>() / chunk.len() as f32
                    }
                })
                .collect()
        }
    }

    pub fn set_hovered_chart_point(
        &mut self,
        host_id: String,
        point: Option<usize>,
    ) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetHoveredChartPoint { host_id, point })
    }

    pub fn clear_hovered_chart_point(&mut self, host_id: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::ClearHoveredChartPoint(host_id))
    }

    pub fn set_modal_is_testing(&mut self, is_testing: bool) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetModalIsTesting(is_testing))
    }

    pub fn set_modal_test_status(&mut self, status: Option<(String, bool)>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetModalTestStatus(status))
    }

    pub fn set_hover_pos(&mut self, pos: Option<(f64, f64)>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetHoverPos(pos))
    }

    pub fn set_filter_focused(&mut self, focused: bool) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetFilterFocused(focused))
    }

    pub fn toggle_batch_host(&mut self, id: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::ToggleBatchHost(id))
    }

    pub fn select_all_batch_hosts(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::SelectAllBatchHosts)
    }

    pub fn clear_batch_hosts(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::ClearBatchHosts)
    }

    pub fn set_batch_command(&mut self, command: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetBatchCommand(command))
    }

    pub fn set_batch_running(&mut self, running: bool) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetBatchRunning(running))
    }

    pub fn set_batch_results(&mut self, results: BatchJobResult) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetBatchResults(results))
    }

    pub fn select_batch_log_host(&mut self, host: Option<String>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SelectBatchLogHost(host))
    }

    pub fn toggle_terminal_search(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::ToggleTerminalSearch)
    }

    pub fn set_terminal_search_query(&mut self, query: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetTerminalSearchQuery(query))
    }

    pub fn close_terminal_search(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::CloseTerminalSearch)
    }

    pub fn trigger_run_batch(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::TriggerRunBatch)
    }

    pub fn switch_settings_category(&mut self, cat: SettingsCategory) {
        self.handle_action(UserAction::SwitchSettingsCategory(cat));
    }

    pub fn set_probe_interval(&mut self, interval: u64) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetProbeInterval(interval))
    }

    pub fn set_ping_target(&mut self, target: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetPingTarget(target))
    }

    pub fn set_cpu_threshold(&mut self, threshold: Option<f32>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetCpuThreshold(threshold))
    }

    pub fn set_mem_threshold(&mut self, threshold: Option<f32>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetMemThreshold(threshold))
    }

    pub fn set_disk_threshold(&mut self, threshold: Option<f32>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetDiskThreshold(threshold))
    }

    pub fn set_webhook_url(&mut self, url: Option<String>) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetWebhookUrl(url))
    }

    pub fn toggle_glow(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::ToggleGlow)
    }

    pub fn reset_settings(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::ResetSettings)
    }

    pub fn set_theme(&mut self, theme_name: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetTheme(theme_name))
    }

    pub fn set_locale(&mut self, locale: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetLocale(locale))
    }

    pub fn set_terminal_font_size(&mut self, size: f32) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetTerminalFontSize(size))
    }

    pub fn set_terminal_cursor_style(&mut self, style: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetTerminalCursorStyle(style))
    }

    pub fn set_terminal_font_family(&mut self, font: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetTerminalFontFamily(font))
    }

    pub fn set_terminal_scrollback(&mut self, lines: usize) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetTerminalScrollback(lines))
    }

    pub fn set_settings_save_status(&mut self, status: Option<(String, bool)>) {
        self.handle_action(UserAction::SetSettingsSaveStatus(status));
    }

    pub fn set_sftp_path(&mut self, path: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::SetSftpPath(path))
    }

    pub fn set_sftp_files(&mut self, files: Vec<RemoteFileItem>) {
        self.handle_action(UserAction::SetSftpFiles(files));
    }

    pub fn set_sftp_loading(&mut self, loading: bool) {
        self.handle_action(UserAction::SetSftpLoading(loading));
    }

    pub fn set_sftp_error(&mut self, error: Option<String>) {
        self.handle_action(UserAction::SetSftpError(error));
    }

    pub fn open_sftp_editor(&mut self, path: String, content: String) {
        self.handle_action(UserAction::OpenSftpEditor { path, content });
    }

    pub fn update_sftp_editor_content(&mut self, content: String) {
        self.handle_action(UserAction::UpdateSftpEditorContent(content));
    }

    pub fn close_sftp_editor(&mut self) {
        self.handle_action(UserAction::CloseSftpEditor);
    }

    pub fn request_read_sftp_file(&mut self, path: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::RequestReadSftpFile(path))
    }

    pub fn request_save_sftp_file(&mut self) -> Vec<UiEffect> {
        self.handle_action(UserAction::RequestSaveSftpFile)
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
        let loc = crate::i18n::Locale::from_code(&self.settings.language)
            .unwrap_or(crate::i18n::Locale::ZhCn);
        crate::i18n::lookup_in_locale(loc, key).unwrap_or(key)
    }

    pub fn delete_host(&mut self, host_id: String) -> Vec<UiEffect> {
        self.handle_action(UserAction::DeleteHost(host_id))
    }

    pub fn resize_terminal(&mut self, cols: usize, rows: usize) -> Vec<UiEffect> {
        self.handle_action(UserAction::ResizeTerminal { cols, rows })
    }

    pub fn generate_current_onboarding_command(&self) -> String {
        let node_id = if self.enroll_node_id_input.trim().is_empty() {
            None
        } else {
            Some(self.enroll_node_id_input.trim())
        };
        let pub_key = self.client_keypair.as_ref().map(|(pk, _)| pk.as_str());

        crate::control_plane::ClientSigner::format_onboarding_command(
            &self.enroll_hub_url,
            node_id,
            &self.enroll_token_input,
            pub_key,
        )
    }

    pub fn generate_current_docker_command(&self) -> String {
        let node_id = if self.enroll_node_id_input.trim().is_empty() {
            None
        } else {
            Some(self.enroll_node_id_input.trim())
        };
        let pub_key = self.client_keypair.as_ref().map(|(pk, _)| pk.as_str());

        crate::control_plane::ClientSigner::format_docker_command(
            &self.enroll_hub_url,
            node_id,
            &self.enroll_token_input,
            pub_key,
        )
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
        sm.set_snippet_output(Some((
            "Test Title".to_string(),
            "Success output".to_string(),
        )));
        assert_eq!(
            sm.snippet_output,
            Some(("Test Title".to_string(), "Success output".to_string()))
        );

        sm.set_snippet_output(None);
        assert_eq!(sm.snippet_output, None);
    }

    #[test]
    fn test_sftp_state_transitions() {
        let mut sm = AppStateMachine::new();

        // 1. Initial State
        assert_eq!(sm.sftp_current_path, "/");
        assert!(sm.sftp_files.is_empty());
        assert!(!sm.sftp_loading);
        assert!(sm.sftp_error.is_none());
        assert!(sm.sftp_editor.is_none());
        assert!(!sm.sftp_editor_modified);

        // 2. Set path without host selected -> no effect
        let effects = sm.set_sftp_path("/var/log".to_string());
        assert_eq!(sm.sftp_current_path, "/var/log");
        assert!(sm.sftp_loading);
        assert!(effects.is_empty());

        // 3. Select host and set path -> emits FetchSftpList
        sm.selected_host_id = Some("srv-prod".to_string());
        let effects = sm.set_sftp_path("/etc/nginx".to_string());
        assert_eq!(sm.sftp_current_path, "/etc/nginx");
        assert_eq!(
            effects,
            vec![UiEffect::FetchSftpList {
                host_id: "srv-prod".to_string(),
                path: "/etc/nginx".to_string(),
            }]
        );

        // 4. Set files -> clears loading and error
        let file_item = RemoteFileItem {
            name: "nginx.conf".to_string(),
            path: "/etc/nginx/nginx.conf".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 2048,
            modified: Some(1727000000),
            permissions: 0o644,
        };
        sm.set_sftp_files(vec![file_item.clone()]);
        assert_eq!(sm.sftp_files.len(), 1);
        assert_eq!(sm.sftp_files[0].name, "nginx.conf");
        assert!(!sm.sftp_loading);
        assert!(sm.sftp_error.is_none());

        // 5. Error handling
        sm.set_sftp_error(Some("Permission denied".to_string()));
        assert_eq!(sm.sftp_error, Some("Permission denied".to_string()));
        assert!(!sm.sftp_loading);

        // 6. Request Read File
        let effects = sm.request_read_sftp_file("/etc/nginx/nginx.conf".to_string());
        assert_eq!(
            effects,
            vec![UiEffect::ReadSftpFile {
                host_id: "srv-prod".to_string(),
                path: "/etc/nginx/nginx.conf".to_string(),
            }]
        );

        // 7. Open Editor
        sm.open_sftp_editor(
            "/etc/nginx/nginx.conf".to_string(),
            "server { listen 80; }".to_string(),
        );
        assert_eq!(
            sm.sftp_editor,
            Some((
                "/etc/nginx/nginx.conf".to_string(),
                "server { listen 80; }".to_string()
            ))
        );
        assert!(!sm.sftp_editor_modified);

        // 8. Update editor content -> marked as modified
        sm.update_sftp_editor_content("server { listen 443 ssl; }".to_string());
        assert!(sm.sftp_editor_modified);
        assert_eq!(
            sm.sftp_editor.as_ref().unwrap().1,
            "server { listen 443 ssl; }"
        );

        // 9. Request Save File -> emits SaveSftpFile
        let effects = sm.request_save_sftp_file();
        assert_eq!(
            effects,
            vec![UiEffect::SaveSftpFile {
                host_id: "srv-prod".to_string(),
                path: "/etc/nginx/nginx.conf".to_string(),
                content: "server { listen 443 ssl; }".to_string(),
            }]
        );
        sm.set_sftp_loading(false);

        // 10. Close Editor
        sm.close_sftp_editor();
        assert!(sm.sftp_editor.is_none());
        assert!(!sm.sftp_editor_modified);

        // 11. Switch view to Sftp auto-triggers fetch if empty
        sm.sftp_files.clear();
        let effects = sm.handle_action(UserAction::SwitchView(ActiveView::Sftp));
        assert_eq!(sm.active_view, ActiveView::Sftp);
        assert_eq!(
            effects,
            vec![UiEffect::FetchSftpList {
                host_id: "srv-prod".to_string(),
                path: "/etc/nginx".to_string(),
            }]
        );
    }

    #[test]
    fn test_settings_state_and_actions() {
        let mut sm = AppStateMachine::new();
        assert_eq!(sm.active_settings_category, SettingsCategory::Appearance);
        assert_eq!(sm.settings_save_status, None);
        assert_eq!(sm.ping_target, "1.1.1.1");

        // 1. Switch categories
        sm.switch_settings_category(SettingsCategory::Terminal);
        assert_eq!(sm.active_settings_category, SettingsCategory::Terminal);
        sm.switch_settings_category(SettingsCategory::Probe);
        assert_eq!(sm.active_settings_category, SettingsCategory::Probe);
        sm.switch_settings_category(SettingsCategory::Alerts);
        assert_eq!(sm.active_settings_category, SettingsCategory::Alerts);
        sm.switch_settings_category(SettingsCategory::Backup);
        assert_eq!(sm.active_settings_category, SettingsCategory::Backup);

        // 2. Theme & Locale switching emits SaveSettings
        let effects = sm.set_theme("CyberpunkNeon".to_string());
        assert_eq!(sm.settings.theme_name, "CyberpunkNeon");
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        let effects = sm.set_locale("en-US".to_string());
        assert_eq!(sm.settings.language, "en-US");
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        // 3. Probe interval & Ping target
        let effects = sm.set_probe_interval(5);
        assert_eq!(sm.settings.probe_interval_secs, 5);
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        sm.set_ping_target("8.8.8.8".to_string());
        assert_eq!(sm.ping_target, "8.8.8.8");

        // 4. Thresholds & Webhook
        let effects = sm.set_cpu_threshold(Some(80.0));
        assert_eq!(sm.settings.alert_cpu_threshold, 80.0);
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        let effects = sm.set_mem_threshold(Some(75.0));
        assert_eq!(sm.settings.alert_mem_threshold, 75.0);
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        let effects = sm.set_disk_threshold(None);
        assert_eq!(sm.settings.alert_disk_threshold, 0.0);
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        let effects = sm.set_webhook_url(Some("https://hooks.slack.com/services/xxx".to_string()));
        assert_eq!(
            sm.settings.alert_webhook_url,
            Some("https://hooks.slack.com/services/xxx".to_string())
        );
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        // 5. Glow toggle
        let initial_glow = sm.settings.glow_effects_enabled;
        let effects = sm.toggle_glow();
        assert_eq!(sm.settings.glow_effects_enabled, !initial_glow);
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        // 6. Terminal Font & Cursor
        let effects = sm.set_terminal_font_size(14.0);
        assert_eq!(sm.settings.terminal_font_size, 14.0);
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        let effects = sm.set_terminal_cursor_style("Bar".to_string());
        assert_eq!(sm.settings.terminal_cursor_style, "Bar");
        assert_eq!(effects, vec![UiEffect::SaveSettings]);

        // 7. Status message
        sm.set_settings_save_status(Some(("Saved successfully".to_string(), true)));
        assert_eq!(
            sm.settings_save_status,
            Some(("Saved successfully".to_string(), true))
        );

        // 8. Reset Settings
        let effects = sm.reset_settings();
        assert_eq!(sm.settings, AppSettings::default());
        assert_eq!(sm.ping_target, "1.1.1.1");
        assert_eq!(effects, vec![UiEffect::SaveSettings]);
    }

    #[test]
    fn test_batch_state_and_actions() {
        use redash_types::batch::{BatchJobResult, HostTaskExecution, TaskState};
        let mut sm = AppStateMachine::new();

        // Check initial state
        assert!(sm.batch_selected_host_ids.is_empty());
        assert_eq!(sm.batch_command, "uptime");
        assert!(!sm.batch_is_running);
        assert!(sm.batch_results.is_none());
        assert!(sm.batch_selected_log_host.is_none());

        // Switch to ActiveView::Batch
        sm.handle_action(UserAction::SwitchView(ActiveView::Batch));
        assert_eq!(sm.active_view, ActiveView::Batch);

        // Add dummy hosts
        let host1 = HostConfig::new("Server 1", "10.0.0.1", "root");
        let host2 = HostConfig::new("Server 2", "10.0.0.2", "root");
        let h1_id = host1.id.0.clone();
        let h2_id = host2.id.0.clone();
        sm.hosts = vec![host1, host2];

        // ToggleBatchHost
        sm.toggle_batch_host(h1_id.clone());
        assert!(sm.batch_selected_host_ids.contains(&h1_id));
        assert_eq!(sm.batch_selected_host_ids.len(), 1);

        sm.toggle_batch_host(h1_id.clone());
        assert!(!sm.batch_selected_host_ids.contains(&h1_id));
        assert!(sm.batch_selected_host_ids.is_empty());

        // SelectAllBatchHosts
        sm.select_all_batch_hosts();
        assert_eq!(sm.batch_selected_host_ids.len(), 2);
        assert!(sm.batch_selected_host_ids.contains(&h1_id));
        assert!(sm.batch_selected_host_ids.contains(&h2_id));

        // ClearBatchHosts
        sm.clear_batch_hosts();
        assert!(sm.batch_selected_host_ids.is_empty());

        // SetBatchCommand
        sm.set_batch_command("df -h".to_string());
        assert_eq!(sm.batch_command, "df -h");

        // trigger_run_batch with empty selection does nothing
        let effects = sm.trigger_run_batch();
        assert!(effects.is_empty());
        assert!(!sm.batch_is_running);

        // trigger_run_batch with selected host emits UiEffect::RunBatch
        sm.toggle_batch_host(h1_id.clone());
        let effects = sm.trigger_run_batch();
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            UiEffect::RunBatch { host_ids, command } => {
                assert_eq!(host_ids, &vec![h1_id.clone()]);
                assert_eq!(command, "df -h");
            }
            _ => panic!("Expected RunBatch effect"),
        }
        assert!(sm.batch_is_running);

        // SetBatchResults
        let mut results = HashMap::new();
        results.insert(
            h1_id.clone(),
            HostTaskExecution {
                host_id: h1_id.clone(),
                host_name: "Server 1".to_string(),
                state: TaskState::Success,
                stdout: "Filesystem 100G".to_string(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: 120,
                duration_us: 120000,
                error: None,
            },
        );
        let job = BatchJobResult {
            job_id: "test-job".to_string(),
            command: "df -h".to_string(),
            hosts_results: results,
            total_duration_ms: 125,
            total_duration_us: 125000,
        };
        sm.set_batch_results(job.clone());
        assert!(!sm.batch_is_running);
        assert_eq!(sm.batch_results, Some(job));

        // SelectBatchLogHost
        sm.select_batch_log_host(Some(h1_id.clone()));
        assert_eq!(sm.batch_selected_log_host, Some(h1_id));
        sm.select_batch_log_host(None);
        assert!(sm.batch_selected_log_host.is_none());
    }

    #[test]
    fn test_terminal_search_state_and_actions() {
        let mut sm = AppStateMachine::new();

        assert!(!sm.terminal_search_active);
        assert!(sm.terminal_search_query.is_empty());
        assert_eq!(sm.terminal_search_match_count, 0);

        // Append some terminal content
        sm.handle_action(UserAction::AppendTerminal("Line 1: Error found in kernel\nLine 2: System running error-free\nLine 3: ERROR: critical".to_string()));

        // ToggleTerminalSearch ON
        sm.toggle_terminal_search();
        assert!(sm.terminal_search_active);

        // Set search query
        sm.set_terminal_search_query("error".to_string());
        assert_eq!(sm.terminal_search_query, "error");
        // "Error", "error-free", "ERROR" -> 3 matches
        assert_eq!(sm.terminal_search_match_count, 3);

        // Append more content while search is active
        sm.handle_action(UserAction::AppendTerminal(
            "Line 4: Another Error here".to_string(),
        ));
        assert_eq!(sm.terminal_search_match_count, 4);

        // Change query
        sm.set_terminal_search_query("kernel".to_string());
        assert_eq!(sm.terminal_search_match_count, 1);

        // Close search
        sm.close_terminal_search();
        assert!(!sm.terminal_search_active);
        assert!(sm.terminal_search_query.is_empty());
        assert_eq!(sm.terminal_search_match_count, 0);
    }

    #[test]
    fn test_card_metrics_and_hover_state() {
        let mut sm = AppStateMachine::new();
        let host_id = "test-host-1".to_string();

        assert_eq!(sm.get_card_metric(&host_id), CardMetricType::All);

        sm.set_card_metric(host_id.clone(), CardMetricType::Cpu);
        assert_eq!(sm.get_card_metric(&host_id), CardMetricType::Cpu);

        sm.set_card_metric(host_id.clone(), CardMetricType::Memory);
        assert_eq!(sm.get_card_metric(&host_id), CardMetricType::Memory);

        sm.set_card_metric(host_id.clone(), CardMetricType::Disk);
        assert_eq!(sm.get_card_metric(&host_id), CardMetricType::Disk);

        // Hover points
        assert_eq!(sm.hovered_chart_points.get(&host_id), None);
        sm.set_hovered_chart_point(host_id.clone(), Some(5));
        assert_eq!(sm.hovered_chart_points.get(&host_id), Some(&5));

        sm.clear_hovered_chart_point(host_id.clone());
        assert_eq!(sm.hovered_chart_points.get(&host_id), None);

        // Metrics history multi-series tracking
        let mut metrics = NodeMetrics::default();
        metrics.cpu.usage_percent = 25.5;
        metrics.mem.usage_percent = 60.0;
        metrics.disks = vec![redash_types::metrics::DiskMetrics {
            usage_percent: 42.0,
            ..Default::default()
        }];

        sm.handle_action(UserAction::UpdateMetrics {
            host_id: host_id.clone(),
            metrics,
        });

        assert_eq!(sm.cpu_histories.get(&host_id).unwrap(), &vec![25.5]);
        assert_eq!(sm.mem_histories.get(&host_id).unwrap(), &vec![60.0]);
        assert_eq!(sm.disk_histories.get(&host_id).unwrap(), &vec![42.0]);
        assert_eq!(sm.metrics_history.get(&host_id).unwrap(), &vec![25.5]);
    }

    #[test]
    fn test_modal_test_connection_state() {
        let mut sm = AppStateMachine::new();
        assert!(!sm.modal_is_testing);
        assert!(sm.modal_test_status.is_none());

        sm.set_modal_is_testing(true);
        assert!(sm.modal_is_testing);

        sm.set_modal_test_status(Some(("连接成功 (24ms)".to_string(), true)));
        assert_eq!(
            sm.modal_test_status,
            Some(("连接成功 (24ms)".to_string(), true))
        );

        // Editing input clears test status
        sm.handle_action(UserAction::ModalInput {
            field: 1,
            text: "192.168.1.100".to_string(),
        });
        assert!(sm.modal_test_status.is_none());

        // Closing modal resets testing flags
        sm.set_modal_is_testing(true);
        sm.set_modal_test_status(Some(("Error".to_string(), false)));
        sm.handle_action(UserAction::CloseAddModal);
        assert!(!sm.modal_is_testing);
        assert!(sm.modal_test_status.is_none());
    }

    #[test]
    fn test_delete_host_cleans_up_all_resources() {
        let mut sm = AppStateMachine::new();
        let mut h1 = HostConfig::new("Host-1", "1.1.1.1", "root");
        h1.id = redash_types::HostId("h1".to_string());
        let mut h2 = HostConfig::new("Host-2", "1.1.1.2", "root");
        h2.id = redash_types::HostId("h2".to_string());
        sm.hosts = vec![h1, h2];
        sm.selected_host_id = Some("h1".to_string());

        let mut metrics = NodeMetrics::default();
        metrics.cpu.usage_percent = 50.0;
        sm.handle_action(UserAction::UpdateMetrics {
            host_id: "h1".to_string(),
            metrics,
        });
        sm.set_card_metric("h1".to_string(), CardMetricType::Disk);
        sm.set_hovered_chart_point("h1".to_string(), Some(5));

        assert!(sm.metrics.contains_key("h1"));
        assert!(sm.cpu_histories.contains_key("h1"));
        assert!(sm.mem_histories.contains_key("h1"));
        assert!(sm.disk_histories.contains_key("h1"));
        assert!(sm.active_card_metrics.contains_key("h1"));
        assert!(sm.hovered_chart_points.contains_key("h1"));

        // Delete h1
        let effects = sm.delete_host("h1".to_string());
        assert_eq!(effects, vec![UiEffect::DeleteHost("h1".to_string())]);
        assert_eq!(sm.hosts.len(), 1);
        assert_eq!(sm.selected_host_id, Some("h2".to_string()));

        // Verify maps are all cleaned up
        assert!(!sm.metrics.contains_key("h1"));
        assert!(!sm.cpu_histories.contains_key("h1"));
        assert!(!sm.mem_histories.contains_key("h1"));
        assert!(!sm.disk_histories.contains_key("h1"));
        assert!(!sm.active_card_metrics.contains_key("h1"));
        assert!(!sm.active_time_ranges.contains_key("h1"));
        assert!(!sm.hovered_chart_points.contains_key("h1"));
    }

    #[test]
    fn test_chart_time_range_and_downsampling() {
        let mut sm = AppStateMachine::new();
        let host_id = "node-101".to_string();

        // Default range should be 1m (R1m)
        assert_eq!(sm.get_host_chart_time_range(&host_id), ChartTimeRange::R1m);

        // Switch to 5m, 30m, 60m
        sm.set_host_chart_time_range(host_id.clone(), ChartTimeRange::R5m);
        assert_eq!(sm.get_host_chart_time_range(&host_id), ChartTimeRange::R5m);

        sm.set_host_chart_time_range(host_id.clone(), ChartTimeRange::R60m);
        assert_eq!(sm.get_host_chart_time_range(&host_id), ChartTimeRange::R60m);

        // Test slice_history_for_range with short data (<= 60 points)
        let short_data: Vec<f32> = (0..20).map(|i| i as f32).collect();
        let sliced = AppStateMachine::slice_history_for_range(&short_data, ChartTimeRange::R1m);
        assert_eq!(sliced.len(), 20);

        // Test slice_history_for_range with 60m data (1800 points downsampled to 60 points)
        let large_data: Vec<f32> = (0..1800).map(|i| (i % 100) as f32).collect();
        let downsampled =
            AppStateMachine::slice_history_for_range(&large_data, ChartTimeRange::R60m);
        assert_eq!(downsampled.len(), 60);

        // Test 1m slice on large data (should take last 30 samples and keep <= 60 points)
        let slice_1m = AppStateMachine::slice_history_for_range(&large_data, ChartTimeRange::R1m);
        assert_eq!(slice_1m.len(), 30);
        assert_eq!(slice_1m, &large_data[1770..]);
    }

    #[test]
    fn test_terminal_resize_effect() {
        let mut sm = AppStateMachine::new();
        assert_eq!(sm.terminal_grid.cols, 120);
        assert_eq!(sm.terminal_grid.rows, 40);

        // Resizing to new dimensions returns effect
        let effects = sm.resize_terminal(160, 50);
        assert_eq!(
            effects,
            vec![UiEffect::ResizeTerminal {
                cols: 160,
                rows: 50
            }]
        );
        assert_eq!(sm.terminal_grid.cols, 160);
        assert_eq!(sm.terminal_grid.rows, 50);

        // Same dimensions returns empty effects (no-op)
        let effects_same = sm.resize_terminal(160, 50);
        assert!(effects_same.is_empty());
    }

    #[test]
    fn test_control_plane_mvi_flow() {
        let mut state = AppStateMachine::new();

        // 1. Set client keypair
        let seed = [9u8; 32];
        let (pub_hex, priv_hex) = crate::control_plane::ClientSigner::keypair_from_seed(&seed);
        state.handle_action(UserAction::SetClientKeypair {
            public_key: pub_hex.clone(),
            private_key: priv_hex.clone(),
        });
        assert_eq!(state.client_keypair, Some((pub_hex, priv_hex)));

        // 2. Receive Agent Telemetry
        let telemetry = redash_types::AgentTelemetry {
            node_id: "node-vps-99".to_string(),
            hostname: "vps-lon".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            timestamp: 1710000000,
            uptime_secs: 7200,
            cpu_usage_pct: 45.0,
            cpu_cores: 2,
            mem_used_bytes: 1024 * 1024 * 500,
            mem_total_bytes: 1024 * 1024 * 1000,
            disk_used_bytes: 1024 * 1024 * 1024 * 5,
            disk_total_bytes: 1024 * 1024 * 1024 * 20,
            net_rx_rate: 100,
            net_tx_rate: 200,
            containers: vec![],
        };
        state.handle_action(UserAction::ReceiveAgentTelemetry(telemetry));
        assert!(state.control_plane_telemetries.contains_key("node-vps-99"));
        assert_eq!(state.cpu_histories.get("cp-node-vps-99").unwrap().len(), 1);

        // 3. Trigger Remediation Action
        let action = redash_types::RemediationAction::RestartContainer {
            container_id: "docker-app-1".to_string(),
        };
        let effects = state.handle_action(UserAction::TriggerRemediation {
            node_id: "node-vps-99".to_string(),
            action,
        });

        assert_eq!(effects.len(), 1);
        if let UiEffect::DispatchControlPlaneAction { signed_action } = &effects[0] {
            assert_eq!(signed_action.node_id, "node-vps-99");
            assert!(!signed_action.signature_hex.is_empty());
        } else {
            panic!("Expected DispatchControlPlaneAction effect");
        }

        // 4. Action completed
        let res = redash_types::ActionResult {
            action_id: "act-1".to_string(),
            node_id: "node-vps-99".to_string(),
            success: true,
            exit_code: Some(0),
            stdout: "Container restarted".to_string(),
            stderr: String::new(),
            duration_ms: 12,
        };
        state.handle_action(UserAction::ActionExecutionCompleted(res.clone()));
        assert_eq!(state.last_action_result, Some(res));
        assert_eq!(state.pending_action, None);

        // 5. Enrollment command generation
        state.handle_action(UserAction::SetEnrollNodeId("vps-node-1".to_string()));
        state.handle_action(UserAction::SetEnrollToken("auth-token-xyz".to_string()));
        let bash_cmd = state.generate_current_onboarding_command();
        assert!(bash_cmd.contains("--node-id vps-node-1"));
        assert!(bash_cmd.contains("--token auth-token-xyz"));

        let docker_cmd = state.generate_current_docker_command();
        assert!(docker_cmd.contains("-e REDASH_NODE_ID=vps-node-1"));
        assert!(docker_cmd.contains("-e REDASH_AUTH_TOKEN=auth-token-xyz"));
    }
}

