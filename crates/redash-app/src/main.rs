#![recursion_limit = "2048"]

mod components;
pub mod i18n;
mod terminal;
pub mod theme;
mod views;

#[cfg(test)]
mod tests;

use crate::i18n::I18n;
use gpui::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use redash_core::config::alert::{AlertDispatcher, AlertRule};
use redash_core::config::{
    AppSettings, AppSettingsExt, AuthMethod, CredentialVault, HostConfig, HostId, HostStore,
    TargetOs,
};
use redash_core::probe::ProbeScheduler;
use redash_core::session::SessionManager;

use crate::components::agent_enroll_modal::{AgentEnrollModal, AgentEnrollModalAction};
use crate::components::host_modal::{HostModal, HostModalAction};
use redash_ui_core::control_plane::ClientSigner;
use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use crate::views::{
    BatchView, FleetAction, FleetView, SettingsAction, SettingsView, SftpView, WorkbenchView,
};

actions!(
    redash,
    [
        Quit,
        About,
        Preferences,
        NewHost,
        OpenFleet,
        OpenBatch,
        OpenSettings,
        CloseTab,
        Cut,
        Copy,
        Paste,
        SelectAll,
        SplitRight,
        SplitDown,
        SwapPanes,
        ClosePane,
        ClearTerminal,
        SearchTerminal,
        OpenDocs,
        OpenGitHub,
    ]
);

#[cfg(target_os = "macos")]
fn set_macos_dock_icon() {
    use cocoa::appkit::{NSApp, NSApplication, NSImage};
    use cocoa::base::nil;
    use cocoa::foundation::NSData;

    unsafe {
        let app = NSApp();
        if app != nil {
            let icon_bytes = include_bytes!("../../../assets/icon.png");
            let data = NSData::dataWithBytes_length_(
                nil,
                icon_bytes.as_ptr() as *const std::ffi::c_void,
                icon_bytes.len() as u64,
            );
            let image = NSImage::initWithData_(NSImage::alloc(nil), data);
            if image != nil {
                app.setApplicationIconImage_(image);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_macos_dock_icon() {}

fn with_active_app<F>(cx: &mut App, f: F)
where
    F: FnOnce(&mut ReDashApp, &mut Window, &mut Context<ReDashApp>),
{
    let target_window = cx.active_window().or_else(|| cx.windows().first().copied());
    if let Some(window) = target_window
        && let Some(handle) = window.downcast::<ReDashApp>()
    {
        let _ = handle.update(cx, |app, window, cx| {
            f(app, window, cx);
        });
    }
}

enum TabContent {
    Fleet(Entity<FleetView>),
    Workbench(HostId, Entity<WorkbenchView>),
    Sftp(HostId, Entity<SftpView>),
    Batch(Entity<BatchView>),
    Settings(Entity<SettingsView>),
}

struct TabItem {
    id: String,
    title: String,
    content: TabContent,
}

struct ReDashApp {
    error_msg: Option<String>,
    config_generation: Arc<std::sync::atomic::AtomicU64>,
    session_mgr: Arc<SessionManager>,
    probe_scheduler: Arc<ProbeScheduler>,
    host_store: HostStore,
    fleet_view: Entity<FleetView>,
    monitored_hosts: Arc<std::sync::RwLock<Vec<HostConfig>>>,
    host_resolver_cache: Arc<std::sync::RwLock<std::collections::HashMap<HostId, HostConfig>>>,
    app_settings: Arc<tokio::sync::RwLock<AppSettings>>,
    active_modal: Option<Entity<HostModal>>,
    agent_enroll_modal: Option<Entity<AgentEnrollModal>>,
    client_keypair: Option<(String, String)>,
    #[allow(dead_code)]
    control_plane_nodes: Vec<redash_types::ManagedNodeDetail>,
    tabs: Vec<TabItem>,
    active_tab_index: usize,
}

impl ReDashApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let session_mgr = Arc::new(SessionManager::new());
        let probe_scheduler = Arc::new(ProbeScheduler::new(Arc::clone(&session_mgr)));

        let store_path = HostStore::default_path();
        let host_store = HostStore::load_from_file(&store_path).unwrap_or_else(|error| {
            let mut store = HostStore::with_path(&store_path);
            store.load_error = Some(format!(
                "主机配置读取失败，已禁止写入。请修复 {} 后重启：{error:#}",
                store_path.display()
            ));
            store
        });
        let mut startup_error = host_store.load_error.clone();
        if let Err(error) = CredentialVault::migrate_legacy() {
            startup_error.get_or_insert(format!("旧凭据迁移失败，原文件已保留：{error:#}"));
        }

        // Initialize host resolver cache for transparent JumpHost resolution
        let initial_cache: std::collections::HashMap<HostId, HostConfig> = host_store
            .hosts
            .iter()
            .map(|h| (h.id.clone(), h.clone()))
            .collect();
        let host_resolver_cache = Arc::new(std::sync::RwLock::new(initial_cache));
        let resolver_cache_clone = Arc::clone(&host_resolver_cache);
        session_mgr.set_host_resolver(Arc::new(move |id| {
            resolver_cache_clone
                .read()
                .ok()
                .and_then(|map| map.get(id).cloned())
        }));

        // Initialize FleetView
        let hosts_for_fleet = host_store.hosts.clone();
        let fleet_view = cx.new(|_cx| FleetView::new(hosts_for_fleet));

        let monitored_hosts = Arc::new(std::sync::RwLock::new(host_store.hosts.clone()));

        let settings_path = AppSettings::default_path();
        let app_settings_val = AppSettings::load_from_file(&settings_path).unwrap_or_default();
        DarkTechTheme::set_active_theme(&app_settings_val.theme_name);
        I18n::set_locale_by_code(&app_settings_val.language);
        let app_settings = Arc::new(tokio::sync::RwLock::new(app_settings_val));

        let mut app = Self {
            error_msg: startup_error,
            config_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            session_mgr,
            probe_scheduler,
            host_store,
            fleet_view: fleet_view.clone(),
            monitored_hosts,
            host_resolver_cache,
            app_settings,
            active_modal: None,
            agent_enroll_modal: None,
            client_keypair: {
                let key_path = HostStore::default_path().with_file_name("control_plane_key.json");
                Some(ClientSigner::load_or_generate_keypair(&key_path))
            },
            control_plane_nodes: Vec::new(),
            tabs: vec![TabItem {
                id: "fleet".to_string(),
                title: "全局脉搏大盘".to_string(),
                content: TabContent::Fleet(fleet_view.clone()),
            }],
            active_tab_index: 0,
        };

        // Wire FleetView actions
        app.bind_fleet_actions(fleet_view, cx);
        app.start_monitoring_loop(cx);
        app.start_control_plane_sync_loop(cx);

        app
    }

    fn bind_fleet_actions(&mut self, fleet_view: Entity<FleetView>, cx: &mut Context<Self>) {
        let app_entity = cx.entity().downgrade();
        fleet_view.update(cx, |view, _cx| {
            view.set_on_action(move |action, window, cx| {
                let app_entity = app_entity.clone();
                // Release the FleetView event borrow before updating it again.
                window.defer(cx, move |window, cx| {
                    if let Some(app) = app_entity.upgrade() {
                        match action {
                            FleetAction::OpenTerminal(host) => {
                                app.update(cx, |app, cx| {
                                    app.open_workbench_tab(host, window, cx);
                                });
                            }
                            FleetAction::OpenSftp(host) => {
                                app.update(cx, |app, cx| {
                                    app.open_sftp_tab(host, cx);
                                });
                            }
                            FleetAction::OpenBatch(hosts) => {
                                app.update(cx, |app, cx| {
                                    app.open_batch_tab(hosts, cx);
                                });
                            }
                            FleetAction::AddNewHost => {
                                app.update(cx, |app, cx| {
                                    app.open_add_host_modal(window, cx);
                                });
                            }
                            FleetAction::EditHost(host) => {
                                app.update(cx, |app, cx| {
                                    app.open_edit_host_modal(host, window, cx);
                                });
                            }
                            FleetAction::CloneHost(host) => {
                                app.update(cx, |app, cx| {
                                    app.clone_host(host, cx);
                                });
                            }
                            FleetAction::DeleteHost(host_id) => {
                                app.update(cx, |app, cx| {
                                    app.delete_host(host_id, cx);
                                });
                            }
                            FleetAction::DeleteBatch(host_ids) => {
                                app.update(cx, |app, cx| {
                                    app.delete_batch(host_ids, cx);
                                });
                            }
                            FleetAction::MoveHostUp(host_id) => {
                                app.update(cx, |app, cx| {
                                    app.move_host(host_id, true, cx);
                                });
                            }
                            FleetAction::MoveHostDown(host_id) => {
                                app.update(cx, |app, cx| {
                                    app.move_host(host_id, false, cx);
                                });
                            }
                            FleetAction::MoveHostToTop(host_id) => {
                                app.update(cx, |app, cx| {
                                    app.move_host_to_top(host_id, cx);
                                });
                            }
                            FleetAction::ReorderHosts(host_ids) => {
                                app.update(cx, |app, cx| {
                                    app.reorder_hosts(host_ids, cx);
                                });
                            }
                            FleetAction::AddNewAgentNode => {
                                app.update(cx, |app, cx| {
                                    app.open_agent_enroll_modal(window, cx);
                                });
                            }
                            FleetAction::TriggerAgentRemediation { node_id, action } => {
                                app.update(cx, |app, cx| {
                                    app.trigger_agent_remediation(&node_id, action, cx);
                                });
                            }
                            FleetAction::OpenAgentTty { node_id } => {
                                app.update(cx, |app, cx| {
                                    app.open_agent_tty_tab(&node_id, window, cx);
                                });
                            }
                        }
                    }
                });
            });
        });
    }

    fn show_error(&mut self, message: String, cx: &mut Context<Self>) {
        log::error!("{message}");
        self.error_msg = Some(message.clone());
        if let Some(modal) = &self.active_modal {
            modal.update(cx, |view, cx| {
                view.error_msg = Some(message.clone());
                cx.notify();
            });
        }
        for tab in &self.tabs {
            if let TabContent::Settings(view) = &tab.content {
                view.update(cx, |view, cx| {
                    view.set_status_message(message.clone(), false, cx)
                });
            }
        }
        cx.notify();
    }

    fn sync_monitored_hosts(&mut self, cx: &mut Context<Self>) {
        let hosts = self.host_store.hosts.clone();
        let mut cache = self.host_resolver_cache.write().unwrap();
        let mut changed: HashSet<HostId> = cache
            .values()
            .filter(|old| !hosts.iter().any(|new| old.same_connection(new)))
            .map(|h| h.id.clone())
            .collect();
        loop {
            let before = changed.len();
            for host in cache.values().chain(hosts.iter()) {
                if host.proxy_id().is_some_and(|id| changed.contains(id)) {
                    changed.insert(host.id.clone());
                }
            }
            if changed.len() == before {
                break;
            }
        }
        *cache = hosts.iter().map(|h| (h.id.clone(), h.clone())).collect();
        drop(cache);
        self.config_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.monitored_hosts.write().unwrap() = hosts.clone();
        // Tabs retain live PTYs/SFTP handles, so close affected tabs instead of retargeting them.
        self.tabs.retain(|tab| match &tab.content {
            TabContent::Workbench(id, _) | TabContent::Sftp(id, _) => !changed.contains(id),
            _ => true,
        });
        self.active_tab_index = self.active_tab_index.min(self.tabs.len().saturating_sub(1));
        for tab in &self.tabs {
            if let TabContent::Batch(view) = &tab.content {
                view.update(cx, |view, cx| {
                    if view.targets.iter().any(|h| changed.contains(&h.id)) {
                        view.cancel(cx);
                    }
                    let targets = view
                        .targets
                        .iter()
                        .filter_map(|old| hosts.iter().find(|h| h.id == old.id).cloned())
                        .collect();
                    view.set_hosts(targets, cx);
                });
            }
        }
        let scheduler = Arc::clone(&self.probe_scheduler);
        let ids: Vec<_> = changed.iter().cloned().collect();
        cx.spawn(async move |_, _| scheduler.forget_hosts(&ids).await)
            .detach();
        // Identity checks in SessionManager also cover in-flight connections and proxy changes.
        self.fleet_view.update(cx, |view, cx| {
            for id in &changed {
                view.clear_host_metrics(id);
            }
            view.set_hosts(hosts, cx);
        });
    }

    fn open_agent_enroll_modal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let pub_key = self.client_keypair.as_ref().map(|(pk, _)| pk.clone());
        let http_base = self.session_mgr.hub_http_base();
        let ws_base = if let Some(stripped) = http_base.strip_prefix("http://") {
            format!("ws://{}", stripped)
        } else if let Some(stripped) = http_base.strip_prefix("https://") {
            format!("wss://{}", stripped)
        } else {
            http_base
        };
        let hub_url = format!("{}/v1/agent/ws", ws_base.trim_end_matches('/'));
        let modal = cx.new(|_cx| AgentEnrollModal::new(hub_url, pub_key));

        let app_entity = cx.entity().downgrade();
        cx.subscribe(
            &modal,
            move |_subscriber, _emitter, event: &AgentEnrollModalAction, cx| {
                if let Some(app) = app_entity.upgrade() {
                    match event {
                        AgentEnrollModalAction::Close => {
                            app.update(cx, |app, cx| {
                                app.agent_enroll_modal = None;
                                cx.notify();
                            });
                        }
                        AgentEnrollModalAction::CopyCommand(_cmd) => {
                            app.update(cx, |app, cx| {
                                app.error_msg = Some("📋 接入安装命令已复制到剪贴板，请在受控节点终端执行".to_string());
                                cx.notify();
                            });
                        }
                    }
                }
            },
        )
        .detach();

        self.agent_enroll_modal = Some(modal);
        cx.notify();
    }

    fn trigger_agent_remediation(
        &mut self,
        node_id: &str,
        action: redash_types::RemediationAction,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, ref priv_key)) = self.client_keypair {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let nonce = format!("{:x}", now);
            if let Ok(signed_action) = ClientSigner::sign_action(
                priv_key,
                node_id,
                action,
                now,
                &nonce,
            ) {
                let action_summary = match &signed_action.action {
                    redash_types::RemediationAction::PruneContainers => "清理废弃容器 (docker prune)".to_string(),
                    redash_types::RemediationAction::RestartContainer { container_id } => format!("重启容器 {}", container_id),
                    redash_types::RemediationAction::StopContainer { container_id } => format!("停止容器 {}", container_id),
                    redash_types::RemediationAction::VacuumLogs { max_size_mb } => format!("清理日志 (限额 {}MB)", max_size_mb),
                    redash_types::RemediationAction::KillPortConflict { port } => format!("释放冲突端口 :{}", port),
                    redash_types::RemediationAction::KillProcess { pid, .. } => format!("终止进程 PID {}", pid),
                    redash_types::RemediationAction::RestartService { service_name } => format!("重启服务 {}", service_name),
                    _ => "执行自动化运维".to_string(),
                };

                self.error_msg = Some(format!("⚡ 正在下发治理指令 [{}]: {}...", signed_action.node_id, action_summary));
                cx.notify();

                let http_base = self.session_mgr.hub_http_base();
                let url = format!("{}/v1/control/actions", http_base.trim_end_matches('/'));
                if let Ok(payload) = serde_json::to_string(&signed_action) {
                    cx.spawn(async move |this, cx| {
                        let res = tokio::process::Command::new("curl")
                            .arg("-s")
                            .arg("-X")
                            .arg("POST")
                            .arg("-H")
                            .arg("Content-Type: application/json")
                            .arg("-d")
                            .arg(&payload)
                            .arg(&url)
                            .output()
                            .await;

                        match res {
                            Ok(out) if out.status.success() => {
                                let body = String::from_utf8_lossy(&out.stdout);
                                let _ = this.update(cx, |app, cx| {
                                    app.error_msg = Some(format!("✅ 治理指令成功响应: {}", body.trim()));
                                    cx.notify();
                                });
                            }
                            Ok(out) => {
                                let err_body = String::from_utf8_lossy(&out.stderr);
                                let out_body = String::from_utf8_lossy(&out.stdout);
                                let detail = if !err_body.trim().is_empty() { err_body } else { out_body };
                                let _ = this.update(cx, |app, cx| {
                                    app.error_msg = Some(format!("⚠️ 治理指令执行异常: {}", detail.trim()));
                                    cx.notify();
                                });
                            }
                            Err(e) => {
                                let _ = this.update(cx, |app, cx| {
                                    app.error_msg = Some(format!("❌ 指令分发失败: {}", e));
                                    cx.notify();
                                });
                            }
                        }
                    })
                    .detach();
                }
            }
        }
    }

    fn open_agent_tty_tab(
        &mut self,
        node_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tty_tab_id = format!("agent_tty_{}", node_id);
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tty_tab_id) {
            self.active_tab_index = idx;
            if let TabContent::Workbench(_, wb) = &self.tabs[idx].content {
                wb.read(cx).focus_terminal(window, cx);
            }
            cx.notify();
            return;
        }

        let mut dummy_host = HostConfig::new(
            format!("⚡ {}", node_id),
            node_id,
            "root",
        );
        dummy_host.id = HostId(tty_tab_id.clone());
        let session_mgr = Arc::clone(&self.session_mgr);
        let workbench_view = cx.new(|cx| WorkbenchView::new(dummy_host, session_mgr, cx));
        workbench_view.read(cx).focus_terminal(window, cx);

        self.tabs.push(TabItem {
            id: tty_tab_id,
            title: format!("⚡ TTY · {}", node_id),
            content: TabContent::Workbench(HostId(node_id.to_string()), workbench_view),
        });
        self.active_tab_index = self.tabs.len() - 1;
        cx.notify();
    }

    fn open_add_host_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fh = cx.focus_handle();
        let fh_clone = fh.clone();
        let modal_entity = cx.new(|_cx| {
            HostModal::new_create()
                .with_session_manager(Arc::clone(&self.session_mgr))
                .with_focus_handle(fh_clone)
        });
        let app_entity = cx.entity().downgrade();
        modal_entity.update(cx, |modal, _cx| {
            modal.set_on_action(move |action, window, cx| {
                let app_entity = app_entity.clone();
                // Saving may need to show an error in this same modal.
                window.defer(cx, move |window, cx| {
                    if let Some(app) = app_entity.upgrade() {
                        app.update(cx, |app, cx| {
                            app.handle_modal_action(action, window, cx);
                        });
                    }
                });
            });
        });

        window.focus(&fh);
        self.active_modal = Some(modal_entity);
        cx.notify();
    }

    fn open_edit_host_modal(
        &mut self,
        host: HostConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fh = cx.focus_handle();
        let fh_clone = fh.clone();
        let credential_missing = self
            .fleet_view
            .read(cx)
            .probe_errors
            .get(&host.id)
            .is_some_and(|failure| failure.needs_credentials);
        let modal_entity = cx.new(|_cx| {
            HostModal::new_edit(host)
                .with_session_manager(Arc::clone(&self.session_mgr))
                .with_missing_credential(credential_missing)
                .with_focus_handle(fh_clone)
        });
        let app_entity = cx.entity().downgrade();
        modal_entity.update(cx, |modal, _cx| {
            modal.set_on_action(move |action, window, cx| {
                let app_entity = app_entity.clone();
                // Saving may need to show an error in this same modal.
                window.defer(cx, move |window, cx| {
                    if let Some(app) = app_entity.upgrade() {
                        app.update(cx, |app, cx| {
                            app.handle_modal_action(action, window, cx);
                        });
                    }
                });
            });
        });

        window.focus(&fh);
        self.active_modal = Some(modal_entity);
        cx.notify();
    }

    fn handle_modal_action(
        &mut self,
        action: HostModalAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            HostModalAction::Cancel => {
                self.active_modal = None;
                cx.notify();
            }
            HostModalAction::Delete(host_id) => {
                if self.delete_host(host_id, cx) {
                    self.active_modal = None;
                }
                cx.notify();
            }
            HostModalAction::Clone(host) => {
                self.clone_host(host, cx);
                self.active_modal = None;
                cx.notify();
            }
            HostModalAction::Save {
                host,
                password,
                passphrase,
            } => {
                let mut host = *host;
                let secret = password.or(passphrase);
                let new_credential = if let Some(secret) = secret {
                    let id = format!("redash_{}", uuid::Uuid::new_v4());
                    if let Err(error) = CredentialVault::save_secret(&id, &secret) {
                        self.show_error(format!("凭据保存失败：{error:#}"), cx);
                        return;
                    }
                    match &mut host.auth {
                        AuthMethod::Password { credential_id } => *credential_id = id.clone(),
                        AuthMethod::PrivateKey { passphrase_id, .. } => {
                            *passphrase_id = Some(id.clone())
                        }
                        AuthMethod::Agent => {}
                    }
                    Some(id)
                } else {
                    None
                };
                if let Err(error) = self.host_store.save_host(host) {
                    if let Some(id) = new_credential {
                        let _ = CredentialVault::delete_secret(&id);
                    }
                    self.show_error(format!("主机保存失败：{error}"), cx);
                    return;
                }

                // Publish the saved configuration before closing the editor.
                self.error_msg = None;
                self.sync_monitored_hosts(cx);

                self.active_modal = None;
                cx.notify();
            }
        }
    }

    fn clone_host(&mut self, host: HostConfig, cx: &mut Context<Self>) {
        let outcome = self.host_store.clone_host(&host.id);
        if let Err(error) = &outcome {
            self.show_error(format!("配置操作失败：{error}"), cx);
            let hosts = self.host_store.hosts.clone();
            self.fleet_view
                .update(cx, |view, cx| view.set_hosts(hosts, cx));
            return;
        }
        if let Ok(_cloned) = outcome {
            self.sync_monitored_hosts(cx);
            cx.notify();
        }
    }

    fn delete_host(&mut self, host_id: HostId, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.host_store.delete_host(&host_id) {
            self.show_error(format!("删除失败：{error}"), cx);
            return false;
        }
        self.error_msg = None;
        // 1. Disconnect session
        let session_mgr = Arc::clone(&self.session_mgr);
        let hid = host_id.clone();
        cx.spawn(async move |_, _| {
            session_mgr.disconnect(&hid).await;
        })
        .detach();

        // 2. Close open tabs for this host
        self.tabs.retain(|t| match &t.content {
            TabContent::Workbench(id, _) => id != &host_id,
            TabContent::Sftp(id, _) => id != &host_id,
            _ => true,
        });
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len().saturating_sub(1);
        }

        // Publish only after the configuration was persisted successfully.
        self.sync_monitored_hosts(cx);
        cx.notify();
        true
    }

    fn delete_batch(&mut self, host_ids: Vec<HostId>, cx: &mut Context<Self>) {
        if let Err(error) = self.host_store.remove_batch(&host_ids) {
            self.show_error(format!("删除失败：{error}"), cx);
            return;
        }
        self.error_msg = None;
        // 1. Disconnect sessions
        for hid in &host_ids {
            let session_mgr = Arc::clone(&self.session_mgr);
            let id_clone = hid.clone();
            cx.spawn(async move |_, _| {
                session_mgr.disconnect(&id_clone).await;
            })
            .detach();
        }

        // 2. Close open tabs
        let id_set: HashSet<HostId> = host_ids.iter().cloned().collect();
        self.tabs.retain(|t| match &t.content {
            TabContent::Workbench(id, _) => !id_set.contains(id),
            TabContent::Sftp(id, _) => !id_set.contains(id),
            _ => true,
        });
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len().saturating_sub(1);
        }

        // Publish only after the configuration was persisted successfully.
        self.sync_monitored_hosts(cx);
        cx.notify();
    }

    fn move_host(&mut self, host_id: HostId, move_up: bool, cx: &mut Context<Self>) {
        let outcome = self.host_store.move_host(&host_id, move_up);
        if let Err(error) = &outcome {
            self.show_error(format!("配置操作失败：{error}"), cx);
            let hosts = self.host_store.hosts.clone();
            self.fleet_view
                .update(cx, |view, cx| view.set_hosts(hosts, cx));
            return;
        }
        if let Ok(true) = outcome {
            self.sync_monitored_hosts(cx);
            cx.notify();
        }
    }

    fn move_host_to_top(&mut self, host_id: HostId, cx: &mut Context<Self>) {
        let outcome = self.host_store.move_host_to_top(&host_id);
        if let Err(error) = &outcome {
            self.show_error(format!("配置操作失败：{error}"), cx);
            let hosts = self.host_store.hosts.clone();
            self.fleet_view
                .update(cx, |view, cx| view.set_hosts(hosts, cx));
            return;
        }
        if let Ok(true) = outcome {
            self.sync_monitored_hosts(cx);
            cx.notify();
        }
    }

    fn reorder_hosts(&mut self, new_order: Vec<HostId>, cx: &mut Context<Self>) {
        let outcome = self.host_store.reorder_hosts(&new_order);
        if let Err(error) = &outcome {
            self.show_error(format!("配置操作失败：{error}"), cx);
            let hosts = self.host_store.hosts.clone();
            self.fleet_view
                .update(cx, |view, cx| view.set_hosts(hosts, cx));
            return;
        }
        if let Ok(()) = outcome {
            self.sync_monitored_hosts(cx);
            cx.notify();
        }
    }

    fn open_workbench_tab(
        &mut self,
        host: HostConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host_id = host.id.clone();
        let host_name = host.name.clone();

        // Check if tab already exists
        if let Some(idx) = self.tabs.iter().position(|t| t.id == host_id.0) {
            self.active_tab_index = idx;
            if let TabContent::Workbench(_, wb) = &self.tabs[idx].content {
                wb.read(cx).focus_terminal(window, cx);
            }
            cx.notify();
            return;
        }

        let session_mgr = Arc::clone(&self.session_mgr);
        let workbench_view = cx.new(|cx| WorkbenchView::new(host, session_mgr, cx));
        if let Ok(settings) = self.app_settings.try_read() {
            workbench_view.update(cx, |wb, cx| {
                wb.update_settings(&settings, cx);
            });
        }
        workbench_view.read(cx).focus_terminal(window, cx);

        self.tabs.push(TabItem {
            id: host_id.0.clone(),
            title: host_name.clone(),
            content: TabContent::Workbench(host_id, workbench_view),
        });
        self.active_tab_index = self.tabs.len() - 1;
        cx.notify();
    }

    fn open_sftp_tab(&mut self, host: HostConfig, cx: &mut Context<Self>) {
        let sftp_tab_id = format!("sftp_{}", host.id.0);
        if let Some(idx) = self.tabs.iter().position(|t| t.id == sftp_tab_id) {
            self.active_tab_index = idx;
            cx.notify();
            return;
        }

        let host_name = host.name.clone();
        let host_id = host.id.clone();
        let default_path = SftpView::default_path_for_host(&host);
        let session_mgr = Arc::clone(&self.session_mgr);

        let sftp_view = cx.new(|cx| SftpView::new(host, session_mgr, cx));
        if let Ok(settings) = self.app_settings.try_read() {
            sftp_view.update(cx, |view, _cx| {
                view.show_hidden = settings.sftp_show_hidden_files;
                view.confirm_delete = settings.sftp_confirm_delete;
            });
        }
        sftp_view.update(cx, |view, cx| {
            view.init_connection_and_load(default_path, cx);
        });

        self.tabs.push(TabItem {
            id: sftp_tab_id,
            title: format!("SFTP - {}", host_name),
            content: TabContent::Sftp(host_id, sftp_view),
        });
        self.active_tab_index = self.tabs.len() - 1;
        cx.notify();
    }

    fn open_batch_tab(&mut self, hosts: Vec<HostConfig>, cx: &mut Context<Self>) {
        let batch_tab_id = "batch_exec".to_string();
        if let Some(idx) = self.tabs.iter().position(|t| t.id == batch_tab_id) {
            self.active_tab_index = idx;
            if let TabContent::Batch(ref bv) = self.tabs[idx].content {
                bv.update(cx, |b, cx| b.set_hosts(hosts, cx));
            }
            cx.notify();
            return;
        }

        let session_mgr = Arc::clone(&self.session_mgr);
        let batch_view = cx.new(|_cx| BatchView::new(hosts, session_mgr));

        self.tabs.push(TabItem {
            id: batch_tab_id,
            title: "批量执行".to_string(),
            content: TabContent::Batch(batch_view),
        });
        self.active_tab_index = self.tabs.len() - 1;
        cx.notify();
    }

    fn open_settings_tab(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == "settings") {
            self.active_tab_index = idx;
            cx.notify();
            return;
        }

        let settings = {
            if let Ok(guard) = self.app_settings.try_read() {
                guard.clone()
            } else {
                AppSettings::default()
            }
        };

        let app_entity = cx.entity().downgrade();
        let app_entity_theme = cx.entity().downgrade();
        let app_entity_locale = cx.entity().downgrade();
        let settings_view = cx.new(|cx| {
            let mut view = SettingsView::new(settings, cx);
            view.set_on_theme_preview(move |theme_name, cx| {
                DarkTechTheme::set_active_theme(theme_name);
                if let Some(app) = app_entity_theme.upgrade() {
                    app.update(cx, |_app, cx| {
                        cx.notify();
                    });
                }
            });
            view.set_on_locale_preview(move |lang_code, cx| {
                I18n::set_locale_by_code(lang_code);
                cx.set_menus(build_app_menus());
                if let Some(app) = app_entity_locale.upgrade() {
                    app.update(cx, |_app, cx| {
                        cx.notify();
                    });
                }
            });
            view.on_action(move |action, window, cx| {
                let app_entity = app_entity.clone();
                window.defer(cx, move |window, cx| {
                    if let Some(app) = app_entity.upgrade() {
                        app.update(cx, |this, cx| match action {
                            SettingsAction::Save(new_settings) => {
                                this.handle_save_settings(new_settings, false, window, cx);
                            }
                            SettingsAction::ResetDefaults => {
                                this.handle_save_settings(AppSettings::default(), true, window, cx);
                            }
                            SettingsAction::ResetDemoHosts => {
                                this.handle_reset_demo_hosts(window, cx);
                            }
                            SettingsAction::ExportHosts => {
                                this.handle_export_hosts(window, cx);
                            }
                            SettingsAction::ImportHosts => {
                                this.handle_import_hosts(window, cx);
                            }
                        });
                    }
                });
            });
            view
        });

        self.tabs.push(TabItem {
            id: "settings".to_string(),
            title: crate::t!("nav.settings").to_string(),
            content: TabContent::Settings(settings_view),
        });
        self.active_tab_index = self.tabs.len() - 1;
        cx.notify();
    }

    fn handle_save_settings(
        &mut self,
        new_settings: AppSettings,
        is_reset: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = new_settings.save_to_file(&AppSettings::default_path()) {
            self.show_error(format!("设置保存失败：{error:#}"), cx);
            return;
        }
        DarkTechTheme::set_active_theme(&new_settings.theme_name);
        I18n::set_locale_by_code(&new_settings.language);
        cx.set_menus(build_app_menus());

        // Update local app_settings synchronously
        if let Ok(mut lock) = self.app_settings.try_write() {
            *lock = new_settings.clone();
        } else {
            let app_settings = Arc::clone(&self.app_settings);
            let settings_clone = new_settings.clone();
            cx.spawn(async move |_this, _cx| {
                let mut lock = app_settings.write().await;
                *lock = settings_clone;
            })
            .detach();
        }

        let status_msg = if is_reset {
            crate::t!("settings.reset_msg").to_string()
        } else {
            crate::t!("settings.saved_msg").to_string()
        };

        // Update settings view state and propagate to all open tabs immediately
        for tab in &self.tabs {
            match &tab.content {
                TabContent::Workbench(_, wb) => {
                    wb.update(cx, |wb, cx| {
                        wb.update_settings(&new_settings, cx);
                    });
                }
                TabContent::Sftp(_, sftp) => {
                    sftp.update(cx, |sftp, cx| {
                        sftp.show_hidden = new_settings.sftp_show_hidden_files;
                        sftp.confirm_delete = new_settings.sftp_confirm_delete;
                        cx.notify();
                    });
                }
                TabContent::Settings(s_view) => {
                    s_view.update(cx, |v, cx| {
                        v.set_settings(new_settings.clone(), cx);
                        v.set_status_message(status_msg.clone(), true, cx);
                    });
                }
                _ => {}
            }
        }
        cx.notify();
    }

    fn handle_reset_demo_hosts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mut h1 = HostConfig::new("Web-Production-01", "192.168.1.101", "root");
        h1.target_os = TargetOs::Linux;
        h1.group = "Production".to_string();

        let mut h2 = HostConfig::new("DB-Postgres-Primary", "192.168.1.102", "postgres");
        h2.target_os = TargetOs::Linux;
        h2.group = "Production".to_string();

        let mut h3 = HostConfig::new("Mac-Mini-Build-M2", "192.168.1.105", "builder");
        h3.target_os = TargetOs::Darwin;
        h3.group = "CI/CD".to_string();

        let mut h4 = HostConfig::new("Win-Worker-OpenSSH", "192.168.1.110", "Administrator");
        h4.target_os = TargetOs::Windows;
        h4.group = "Workers".to_string();

        let mut h5 = HostConfig::new("Redis-Cache-01", "192.168.1.115", "root");
        h5.target_os = TargetOs::Linux;
        h5.group = "Cache".to_string();

        if let Err(error) = self.host_store.replace_hosts(vec![h1, h2, h3, h4, h5]) {
            self.show_error(format!("示例配置保存失败：{error}"), cx);
            return;
        }
        self.sync_monitored_hosts(cx);

        for tab in &self.tabs {
            if let TabContent::Settings(ref s_view) = tab.content {
                s_view.update(cx, |v, cx| {
                    v.set_status_message(crate::t!("app.reset_demo_success").to_string(), true, cx);
                });
            }
        }
        cx.notify();
    }

    fn handle_export_hosts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let json = serde_json::to_string_pretty(&self.host_store.hosts).unwrap_or_default();
        cx.write_to_clipboard(ClipboardItem::new_string(json.clone()));

        // Also save a backup copy
        let backup_path = HostStore::default_path().with_file_name("hosts_backup.json");
        let _ = std::fs::write(&backup_path, json);

        for tab in &self.tabs {
            if let TabContent::Settings(ref s_view) = tab.content {
                s_view.update(cx, |v, cx| {
                    v.set_status_message(crate::t!("app.export_success").to_string(), true, cx);
                });
            }
        }
        cx.notify();
    }

    fn handle_import_hosts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard().and_then(|item| item.text());
        if let Some(json) = text
            && let Ok(imported) = serde_json::from_str::<Vec<HostConfig>>(&json)
        {
            for host in &imported {
                if let Err(error) = host.validate() {
                    self.show_error(format!("导入失败（{}）：{error}", host.name), cx);
                    return;
                }
            }
            if !imported.is_empty() {
                let mut candidate = self.host_store.clone();
                for h in imported {
                    candidate.add_or_update(h);
                }
                if let Err(error) = self.host_store.replace_hosts(candidate.hosts) {
                    self.show_error(format!("导入失败：{error}"), cx);
                    return;
                }
                self.sync_monitored_hosts(cx);

                for tab in &self.tabs {
                    if let TabContent::Settings(ref s_view) = tab.content {
                        s_view.update(cx, |v, cx| {
                            v.set_status_message(
                                crate::t!("app.import_success").to_string(),
                                true,
                                cx,
                            );
                        });
                    }
                }
                cx.notify();
                return;
            }
        }

        for tab in &self.tabs {
            if let TabContent::Settings(ref s_view) = tab.content {
                s_view.update(cx, |v, cx| {
                    v.set_status_message(crate::t!("app.import_failed").to_string(), false, cx);
                });
            }
        }
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index > 0 && index < self.tabs.len() {
            let removed = self.tabs.remove(index);
            if removed.id == "settings"
                && let Ok(guard) = self.app_settings.try_read()
            {
                DarkTechTheme::set_active_theme(&guard.theme_name);
            }
            if self.active_tab_index == index {
                self.active_tab_index = index.saturating_sub(1);
            } else if self.active_tab_index > index {
                self.active_tab_index = self.active_tab_index.saturating_sub(1);
            }
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len().saturating_sub(1);
            }
            cx.notify();
        }
    }

    fn split_active_terminal(
        &mut self,
        direction: crate::terminal::split::SplitDirection,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tabs.get(self.active_tab_index)
            && let TabContent::Workbench(_, wb) = &tab.content
        {
            wb.update(cx, |wb, cx| {
                wb.split_terminal(direction, cx);
            });
        }
    }

    fn swap_active_terminal_panes(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active_tab_index)
            && let TabContent::Workbench(_, wb) = &tab.content
        {
            wb.update(cx, |wb, cx| {
                wb.swap_active_pane(cx);
            });
        }
    }

    fn close_active_terminal_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active_tab_index)
            && let TabContent::Workbench(_, wb) = &tab.content
        {
            wb.update(cx, |wb, cx| {
                wb.close_active_pane(cx);
            });
        }
    }

    fn clear_active_terminal(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active_tab_index)
            && let TabContent::Workbench(_, wb) = &tab.content
        {
            wb.update(cx, |wb, cx| {
                if let Some(term) = wb.terminal_views.get(&wb.split_manager.active_pane_id) {
                    term.update(cx, |term, _cx| {
                        term.clear_screen();
                    });
                }
            });
        }
    }

    fn toggle_active_terminal_search(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active_tab_index)
            && let TabContent::Workbench(_, wb) = &tab.content
        {
            wb.update(cx, |wb, cx| {
                if let Some(term) = wb.terminal_views.get(&wb.split_manager.active_pane_id) {
                    term.update(cx, |term, cx| {
                        term.toggle_search(cx);
                    });
                }
            });
        }
    }

    fn start_control_plane_sync_loop(&self, cx: &mut Context<Self>) {
        let http_base = self.session_mgr.hub_http_base();
        let url = format!("{}/v1/control/nodes", http_base.trim_end_matches('/'));
        cx.spawn(async move |this, cx| {
            loop {
                smol::Timer::after(Duration::from_secs(2)).await;

                let output = tokio::process::Command::new("curl")
                    .arg("-s")
                    .arg("--connect-timeout")
                    .arg("1")
                    .arg(&url)
                    .output()
                    .await;

                if let Ok(out) = output
                    && out.status.success()
                    && let Ok(nodes) = serde_json::from_slice::<Vec<redash_types::ManagedNodeDetail>>(&out.stdout)
                {
                    let _ = this.update(cx, |app, cx| {
                        app.control_plane_nodes = nodes.clone();
                        app.fleet_view.update(cx, |fleet, cx| {
                            fleet.set_control_plane_nodes(nodes, cx);
                        });
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn start_monitoring_loop(&self, cx: &mut Context<Self>) {
        use futures::StreamExt;
        let scheduler = Arc::clone(&self.probe_scheduler);
        let monitored = Arc::clone(&self.monitored_hosts);
        let settings = Arc::clone(&self.app_settings);
        let generation = Arc::clone(&self.config_generation);
        cx.spawn(async move |this, cx| {
            let cooldowns = Arc::new(tokio::sync::Mutex::new(HashMap::<
                (String, String),
                (Instant, bool),
            >::new()));
            loop {
                smol::Timer::after(Duration::from_secs(
                    settings.read().await.probe_interval_secs.clamp(1, 3600),
                ))
                .await;
                let current = settings.read().await.clone();
                if !current.auto_refresh {
                    continue;
                }
                let revision = generation.load(std::sync::atomic::Ordering::SeqCst);
                let hosts = monitored.read().unwrap().clone();
                let mut polls = futures::stream::iter(hosts.into_iter().map(|host| {
                    let scheduler = Arc::clone(&scheduler);
                    let current = current.clone();
                    async move {
                        let result = scheduler.poll_host_fleet(&host, &current).await;
                        (host, result)
                    }
                }))
                .buffer_unordered(8);
                let rule = AlertRule {
                    cpu_threshold_percent: Some(current.alert_cpu_threshold),
                    mem_threshold_percent: Some(current.alert_mem_threshold),
                    disk_threshold_percent: Some(current.alert_disk_threshold),
                    notify_offline: current.alert_notify_offline,
                    macos_notification: current.alert_macos_notification,
                    webhook_url: current.alert_webhook_url.clone(),
                };
                while let Some((host, result)) = polls.next().await {
                    if generation.load(std::sync::atomic::Ordering::SeqCst) != revision {
                        break;
                    }
                    let events = match &result {
                        Ok(metrics) => AlertDispatcher::evaluate_metrics(
                            &rule, &host.id.0, &host.name, metrics,
                        ),
                        Err(error) if rule.notify_offline => {
                            vec![redash_core::config::AlertEvent {
                                host_id: host.id.0.clone(),
                                host_name: host.name.clone(),
                                alert_type: "offline".into(),
                                message: format!("{} 采集失败，当前状态未知：{error:#}", host.name),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            }]
                        }
                        Err(_) => Vec::new(),
                    };
                    if this
                        .update(cx, |app, cx| match result {
                            Ok(metrics) => {
                                app.fleet_view.update(cx, |view, cx| {
                                    view.history_limit =
                                        current.history_points.max(1800).clamp(1, 3600);
                                    view.update_metrics(host.id.clone(), metrics.clone(), cx);
                                });
                                for tab in &app.tabs {
                                    if let TabContent::Workbench(id, workbench) = &tab.content
                                        && id == &host.id
                                    {
                                        workbench.update(cx, |view, cx| {
                                            view.set_metrics(metrics.clone(), cx)
                                        });
                                    }
                                }
                            }
                            Err(error) => {
                                let failure = views::fleet_view::ProbeFailure::from_error(&error);
                                app.fleet_view.update(cx, |view, cx| {
                                    view.set_probe_error(host.id.clone(), failure.clone(), cx)
                                });
                                for tab in &app.tabs {
                                    if let TabContent::Workbench(id, workbench) = &tab.content
                                        && id == &host.id
                                    {
                                        workbench.update(cx, |view, cx| {
                                            view.probe_error = Some(failure.message.into());
                                            cx.notify();
                                        });
                                    }
                                }
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                    for event in events {
                        let key = (event.host_id.clone(), event.alert_type.clone());
                        let mut attempts = cooldowns.lock().await;
                        attempts.retain(|_, (last, _)| last.elapsed() < Duration::from_secs(600));
                        if attempts.get(&key).is_some_and(|(last, success)| {
                            last.elapsed() < Duration::from_secs(if *success { 300 } else { 30 })
                        }) {
                            continue;
                        }
                        attempts.insert(key.clone(), (Instant::now(), false));
                        drop(attempts);
                        let cooldowns = Arc::clone(&cooldowns);
                        let rule = rule.clone();
                        tokio::spawn(async move {
                            let outcome = AlertDispatcher::dispatch(&rule, &event).await;
                            if let Err(error) = &outcome {
                                log::error!(
                                    "Alert delivery failed for {}: {error:#}",
                                    event.host_name
                                );
                            }
                            cooldowns
                                .lock()
                                .await
                                .insert(key, (Instant::now(), outcome.is_ok()));
                        });
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for ReDashApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_idx = self.active_tab_index;
        let is_settings_active = matches!(
            self.tabs.get(active_idx).map(|t| &t.content),
            Some(TabContent::Settings(_))
        );

        div()
            .size_full()
            .relative()
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .overflow_hidden()
            .children(self.error_msg.as_ref().map(|message| {
                div()
                    .id("app_global_notification_banner")
                    .px_3()
                    .py_1p5()
                    .bg(DarkTechTheme::bg_popup())
                    .border_b_1()
                    .border_color(DarkTechTheme::border_muted())
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(if message.starts_with("❌") {
                                DarkTechTheme::status_crit()
                            } else if message.starts_with("⚠️") {
                                DarkTechTheme::status_warn()
                            } else if message.starts_with("✅") || message.starts_with("📋") {
                                DarkTechTheme::status_online()
                            } else {
                                DarkTechTheme::accent_cyan()
                            })
                            .child(message.clone()),
                    )
                    .child(
                        div()
                            .id("btn_dismiss_notification_banner")
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .cursor_pointer()
                            .text_size(px(10.0))
                            .text_color(DarkTechTheme::text_muted())
                            .hover(|s| s.text_color(DarkTechTheme::text_primary()))
                            .child("✕")
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.error_msg = None;
                                cx.notify();
                            })),
                    )
            }))
            // 1. Full-Width Top Window Bar (increased 10% to 30px)
            .child(
                div()
                    .id("top_window_bar")
                    .h(px(30.0))
                    .flex_shrink_0()
                    .w_full()
                    .bg(DarkTechTheme::bg_panel())
                    .border_b_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_row()
                    .items_center()
                    // 1a. Safe Area for macOS traffic light buttons (Close/Min/Zoom)
                    .child(
                        div()
                            .id("traffic_light_spacer")
                            .w(px(78.0))
                            .flex_shrink_0()
                            .h_full(),
                    )
                    // 1b. Horizontal Tab Bar
                    .child(
                        div()
                            .id("top_tab_bar")
                            .flex_1()
                            .h_full()
                            .px_2()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .overflow_x_scroll()
                            .children(self.tabs.iter().enumerate().map(|(idx, tab)| {
                                let is_active = idx == active_idx;
                                let tab_title = match &tab.content {
                                    TabContent::Fleet(_) => crate::t!("nav.fleet").to_string(),
                                    TabContent::Batch(_) => crate::t!("nav.batch").to_string(),
                                    TabContent::Settings(_) => {
                                        crate::t!("nav.settings").to_string()
                                    }
                                    _ => tab.title.clone(),
                                };
                                let tab_icon = match &tab.content {
                                    TabContent::Fleet(_) => Icon::server(),
                                    TabContent::Workbench(_, _) => Icon::terminal(),
                                    TabContent::Sftp(_, _) => Icon::folder(),
                                    TabContent::Batch(_) => Icon::activity(),
                                    TabContent::Settings(_) => Icon::settings(),
                                };
                                let icon_color = if is_active {
                                    DarkTechTheme::accent_cyan()
                                } else {
                                    DarkTechTheme::text_muted()
                                };

                                div()
                                    .id(ElementId::NamedInteger("tab_item".into(), idx as u64))
                                    .flex_shrink_0()
                                    .h(px(22.0))
                                    .px_2p5()
                                    .rounded_t_md()
                                    .bg(if is_active {
                                        DarkTechTheme::bg_root()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_t_2()
                                    .border_color(if is_active {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .cursor_pointer()
                                    .on_click(cx.listener(
                                        move |this, _e: &ClickEvent, window, cx| {
                                            this.active_tab_index = idx;
                                            if let TabContent::Workbench(_, wb) =
                                                &this.tabs[idx].content
                                            {
                                                wb.read(cx).focus_terminal(window, cx);
                                            }
                                            cx.notify();
                                        },
                                    ))
                                    .child(tab_icon.with_size(px(10.0)).with_color(icon_color))
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(if is_active {
                                                DarkTechTheme::text_primary()
                                            } else {
                                                DarkTechTheme::text_secondary()
                                            })
                                            .child(tab_title),
                                    )
                                    .child(if idx > 0 {
                                        div()
                                            .id(ElementId::NamedInteger(
                                                "btn_close_tab".into(),
                                                idx as u64,
                                            ))
                                            .size(px(13.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_xs()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(
                                                move |this, _e: &ClickEvent, _window, cx| {
                                                    cx.stop_propagation();
                                                    this.close_tab(idx, cx);
                                                },
                                            ))
                                            .child(
                                                Icon::close()
                                                    .with_size(px(8.0))
                                                    .with_color(DarkTechTheme::text_muted()),
                                            )
                                    } else {
                                        div().id(ElementId::NamedInteger(
                                            "empty_tab_action".into(),
                                            idx as u64,
                                        ))
                                    })
                            })),
                    ),
            )
            // 2. Main Body Area (Sidebar on left, Tab content on right)
            .child(
                div()
                    .id("main_body")
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    // 2a. Leftmost Mini Navigation Bar (52px)
                    .child(
                        div()
                            .w(px(52.0))
                            .flex_shrink_0()
                            .h_full()
                            .bg(DarkTechTheme::bg_panel())
                            .border_r_1()
                            .border_color(DarkTechTheme::border_default())
                            .py_3()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap_3()
                                    // Fleet button
                                    .child(
                                        div()
                                            .id("btn_nav_fleet")
                                            .size(px(36.0))
                                            .rounded_md()
                                            .bg(if active_idx == 0 {
                                                DarkTechTheme::bg_panel_hover()
                                            } else {
                                                DarkTechTheme::bg_panel()
                                            })
                                            .border_1()
                                            .border_color(if active_idx == 0 {
                                                DarkTechTheme::border_active()
                                            } else {
                                                DarkTechTheme::border_muted()
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .on_click(cx.listener(
                                                |this, _e: &ClickEvent, _window, cx| {
                                                    this.active_tab_index = 0;
                                                    cx.notify();
                                                },
                                            ))
                                            .child(Icon::server().with_size(px(18.0)).with_color(
                                                if active_idx == 0 {
                                                    DarkTechTheme::accent_cyan()
                                                } else {
                                                    DarkTechTheme::text_muted()
                                                },
                                            )),
                                    )
                                    // "+" Button directly under Fleet icon to trigger Add Host modal
                                    .child(
                                        div()
                                            .id("btn_nav_add_host")
                                            .size(px(36.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| {
                                                s.bg(DarkTechTheme::bg_panel_hover())
                                                    .border_color(DarkTechTheme::border_active())
                                            })
                                            .cursor_pointer()
                                            .on_click(cx.listener(
                                                |this, _e: &ClickEvent, window, cx| {
                                                    this.open_add_host_modal(window, cx);
                                                },
                                            ))
                                            .child(
                                                Icon::plus()
                                                    .with_size(px(14.0))
                                                    .with_color(DarkTechTheme::accent_cyan()),
                                            ),
                                    )
                                    // Batch Runner button
                                    .child(
                                        div()
                                            .id("btn_nav_batch")
                                            .size(px(36.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_panel())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_muted())
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| {
                                                s.bg(DarkTechTheme::bg_panel_hover())
                                                    .border_color(DarkTechTheme::border_active())
                                            })
                                            .cursor_pointer()
                                            .on_click(cx.listener(
                                                |this, _e: &ClickEvent, _window, cx| {
                                                    let hosts = this.host_store.hosts.clone();
                                                    this.open_batch_tab(hosts, cx);
                                                },
                                            ))
                                            .child(
                                                Icon::terminal()
                                                    .with_size(px(16.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn_nav_settings")
                                    .size(px(36.0))
                                    .rounded_md()
                                    .bg(if is_settings_active {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if is_settings_active {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_muted()
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .border_color(DarkTechTheme::border_active())
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        this.open_settings_tab(window, cx);
                                    }))
                                    .child(Icon::settings().with_size(px(16.0)).with_color(
                                        if is_settings_active {
                                            DarkTechTheme::text_accent()
                                        } else {
                                            DarkTechTheme::text_muted()
                                        },
                                    )),
                            ),
                    )
                    // 2b. Tab Content Body
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .h_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(match &self.tabs[self.active_tab_index].content {
                                TabContent::Fleet(view) => view.clone().into_any_element(),
                                TabContent::Workbench(_, view) => view.clone().into_any_element(),
                                TabContent::Sftp(_, view) => view.clone().into_any_element(),
                                TabContent::Batch(view) => view.clone().into_any_element(),
                                TabContent::Settings(view) => view.clone().into_any_element(),
                            }),
                    ),
            )
            // 3. Modal Overlay if active
            .children(self.active_modal.clone())
            .children(self.agent_enroll_modal.clone())
    }
}

fn main() {
    env_logger::init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _guard = rt.enter();

    Application::new().run(|cx: &mut App| {
        // 1. Set macOS Dock Icon dynamically from embedded asset
        set_macos_dock_icon();

        // 2. Bring application and menu bar to foreground
        cx.activate(true);

        // 3. Register Global Keyboard Shortcuts
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-,", Preferences, None),
            KeyBinding::new("cmd-n", NewHost, None),
            KeyBinding::new("cmd-w", CloseTab, None),
            KeyBinding::new("cmd-1", OpenFleet, None),
            KeyBinding::new("cmd-2", OpenBatch, None),
            KeyBinding::new("cmd-3", OpenSettings, None),
            KeyBinding::new("cmd-d", SplitRight, None),
            KeyBinding::new("cmd-shift-d", SplitDown, None),
            KeyBinding::new("cmd-alt-s", SwapPanes, None),
            KeyBinding::new("cmd-shift-w", ClosePane, None),
            KeyBinding::new("cmd-k", ClearTerminal, None),
            KeyBinding::new("cmd-f", SearchTerminal, None),
        ]);

        // 4. Register Action Handlers
        cx.on_action(|_: &Quit, cx| {
            cx.quit();
        });
        cx.on_action(|_: &About, cx| {
            with_active_app(cx, |app, window, cx| {
                app.open_settings_tab(window, cx);
            });
        });
        cx.on_action(|_: &Preferences, cx| {
            with_active_app(cx, |app, window, cx| {
                app.open_settings_tab(window, cx);
            });
        });
        cx.on_action(|_: &NewHost, cx| {
            with_active_app(cx, |app, window, cx| {
                app.open_add_host_modal(window, cx);
            });
        });
        cx.on_action(|_: &OpenFleet, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.active_tab_index = 0;
                cx.notify();
            });
        });
        cx.on_action(|_: &OpenBatch, cx| {
            with_active_app(cx, |app, _window, cx| {
                let hosts = app.host_store.hosts.clone();
                app.open_batch_tab(hosts, cx);
            });
        });
        cx.on_action(|_: &OpenSettings, cx| {
            with_active_app(cx, |app, window, cx| {
                app.open_settings_tab(window, cx);
            });
        });
        cx.on_action(|_: &CloseTab, cx| {
            with_active_app(cx, |app, _window, cx| {
                if app.active_tab_index > 0 && app.active_tab_index < app.tabs.len() {
                    app.close_tab(app.active_tab_index, cx);
                }
            });
        });
        cx.on_action(|_: &SplitRight, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.split_active_terminal(crate::terminal::split::SplitDirection::Horizontal, cx);
            });
        });
        cx.on_action(|_: &SplitDown, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.split_active_terminal(crate::terminal::split::SplitDirection::Vertical, cx);
            });
        });
        cx.on_action(|_: &SwapPanes, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.swap_active_terminal_panes(cx);
            });
        });
        cx.on_action(|_: &ClosePane, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.close_active_terminal_pane(cx);
            });
        });
        cx.on_action(|_: &ClearTerminal, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.clear_active_terminal(cx);
            });
        });
        cx.on_action(|_: &SearchTerminal, cx| {
            with_active_app(cx, |app, _window, cx| {
                app.toggle_active_terminal_search(cx);
            });
        });
        cx.on_action(|_: &OpenDocs, _cx| {
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open")
                .arg("https://github.com/reways/redash#readme")
                .spawn();
        });
        cx.on_action(|_: &OpenGitHub, _cx| {
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open")
                .arg("https://github.com/reways/redash")
                .spawn();
        });

        // 5. Configure Native Application Menus
        cx.set_menus(build_app_menus());

        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(crate::t!("app.window_title").into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.0), px(8.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(ReDashApp::new),
        )
        .unwrap();
    });
}

fn build_app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: crate::t!("nav.brand").into(),
            items: vec![
                MenuItem::action(crate::t!("menu.about"), About),
                MenuItem::separator(),
                MenuItem::action(crate::t!("menu.preferences"), Preferences),
                MenuItem::separator(),
                MenuItem::os_submenu(crate::t!("menu.services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(crate::t!("menu.quit"), Quit),
            ],
        },
        Menu {
            name: crate::t!("menu.file").into(),
            items: vec![
                MenuItem::action(crate::t!("menu.new_host"), NewHost),
                MenuItem::action(crate::t!("menu.batch"), OpenBatch),
                MenuItem::separator(),
                MenuItem::action(crate::t!("menu.close_tab"), CloseTab),
            ],
        },
        Menu {
            name: crate::t!("menu.edit").into(),
            items: vec![
                MenuItem::os_action(crate::t!("menu.cut"), Cut, OsAction::Cut),
                MenuItem::os_action(crate::t!("menu.copy"), Copy, OsAction::Copy),
                MenuItem::os_action(crate::t!("menu.paste"), Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action(crate::t!("menu.select_all"), SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: crate::t!("menu.view").into(),
            items: vec![
                MenuItem::action(crate::t!("nav.fleet"), OpenFleet),
                MenuItem::action(crate::t!("nav.batch"), OpenBatch),
                MenuItem::action(crate::t!("nav.settings"), OpenSettings),
            ],
        },
        Menu {
            name: crate::t!("menu.terminal").into(),
            items: vec![
                MenuItem::action(crate::t!("menu.split_right"), SplitRight),
                MenuItem::action(crate::t!("menu.split_down"), SplitDown),
                MenuItem::action(crate::t!("menu.swap_panes"), SwapPanes),
                MenuItem::separator(),
                MenuItem::action(crate::t!("menu.clear_screen"), ClearTerminal),
                MenuItem::action(crate::t!("menu.find"), SearchTerminal),
                MenuItem::separator(),
                MenuItem::action(crate::t!("menu.close_pane"), ClosePane),
            ],
        },
        Menu {
            name: crate::t!("menu.help").into(),
            items: vec![
                MenuItem::action(crate::t!("menu.docs"), OpenDocs),
                MenuItem::action(crate::t!("menu.github"), OpenGitHub),
                MenuItem::separator(),
                MenuItem::action(crate::t!("menu.about"), About),
            ],
        },
    ]
}
