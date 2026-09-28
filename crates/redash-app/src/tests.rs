use super::{ReDashApp, TabContent, TabItem};
use crate::components::host_modal::{AuthTypeSelection, HostModalAction, HostModalField};
use crate::views::fleet_view::ProbeFailure;
use crate::views::{FleetAction, FleetView};
use gpui::{AppContext, Context, TestAppContext};
use redash_core::config::{AppSettings, AuthMethod, HostConfig, HostStore, MissingCredential};
use redash_core::probe::ProbeScheduler;
use redash_core::session::SessionManager;
use std::sync::Arc;

// Exercise real view callbacks with GPUI's test dispatcher, without loading the
// user's configuration, starting probes or touching system credentials.
fn test_app(store: HostStore, cx: &mut Context<ReDashApp>) -> ReDashApp {
    let hosts = store.hosts.clone();
    let session_mgr = Arc::new(SessionManager::new());
    let fleet = cx.new(|_| FleetView::new(hosts.clone()));
    let mut app = ReDashApp {
        error_msg: None,
        config_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        probe_scheduler: Arc::new(ProbeScheduler::new(Arc::clone(&session_mgr))),
        session_mgr,
        host_store: store,
        fleet_view: fleet.clone(),
        monitored_hosts: Arc::new(std::sync::RwLock::new(hosts.clone())),
        host_resolver_cache: Arc::new(std::sync::RwLock::new(
            hosts
                .into_iter()
                .map(|host| (host.id.clone(), host))
                .collect(),
        )),
        app_settings: Arc::new(tokio::sync::RwLock::new(AppSettings::default())),
        active_modal: None,
        agent_enroll_modal: None,
        client_keypair: None,
        control_plane_nodes: Vec::new(),
        tabs: vec![TabItem {
            id: "fleet".into(),
            title: "Test fleet".into(),
            content: TabContent::Fleet(fleet.clone()),
        }],
        active_tab_index: 0,
    };
    app.bind_fleet_actions(fleet, cx);
    app
}

#[gpui::test]
fn deleting_from_fleet_callback_updates_view_and_disk(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("redash-ui-delete-{}", uuid::Uuid::new_v4()));
    let path = dir.join("hosts.json");
    let mut store = HostStore::with_path(&path);
    let mut host = HostConfig::new("temporary", "localhost", "test");
    host.auth = AuthMethod::Agent;
    let id = host.id.clone();
    store.save_host(host).unwrap();
    let mut batch_ids = Vec::new();
    for name in ["batch-a", "batch-b"] {
        let mut host = HostConfig::new(name, "localhost", "test");
        host.auth = AuthMethod::Agent;
        batch_ids.push(host.id.clone());
        store.save_host(host).unwrap();
    }
    let (app, cx) = cx.add_window_view(|_, cx| test_app(store, cx));
    let fleet = app.read_with(cx, |app, _| app.fleet_view.clone());
    cx.update(|window, cx| {
        fleet.update(cx, |view, cx| {
            view.on_action.as_ref().unwrap()(FleetAction::DeleteHost(id), window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(app.read_with(cx, |app, _| app.host_store.hosts.len()), 2);
    cx.update(|window, cx| {
        fleet.update(cx, |view, cx| {
            view.on_action.as_ref().unwrap()(FleetAction::DeleteBatch(batch_ids), window, cx);
        });
    });
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.host_store.hosts.is_empty()));
    assert!(fleet.read_with(cx, |view, _| view.hosts.is_empty()));
    assert!(HostStore::load_from_file(&path).unwrap().hosts.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn failed_modal_save_keeps_dialog_and_shows_error(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("redash-ui-error-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let blocker = dir.join("not-a-directory");
    std::fs::write(&blocker, b"blocker").unwrap();
    let mut store = HostStore::with_path(blocker.join("hosts.json"));
    let mut host = HostConfig::new("temporary", "localhost", "test");
    host.auth = AuthMethod::Agent;
    store.add_or_update(host.clone());
    let (app, cx) = cx.add_window_view(|_, cx| test_app(store, cx));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.open_edit_host_modal(host, window, cx));
    });
    let modal = app.read_with(cx, |app, _| app.active_modal.clone().unwrap());
    cx.update(|window, cx| {
        modal.update(cx, |view, cx| {
            view.name = "changed".into();
            let action = view.submit().unwrap();
            view.on_action.as_ref().unwrap()(action, window, cx);
        });
    });
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.active_modal.is_some()));
    assert!(modal.read_with(cx, |view, _| view.error_msg.is_some()));
    assert_eq!(
        app.read_with(cx, |app, _| app.host_store.hosts[0].name.clone()),
        "temporary"
    );
    let host_id = app.read_with(cx, |app, _| app.host_store.hosts[0].id.clone());
    // The delete error must stay visible instead of closing the dialog.
    cx.update(|window, cx| {
        modal.update(cx, |view, cx| {
            view.on_action.as_ref().unwrap()(HostModalAction::Delete(host_id.clone()), window, cx);
        });
    });
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.active_modal.is_some()));
    assert!(modal.read_with(cx, |view, _| {
        view.error_msg.as_ref().unwrap().contains("删除失败")
    }));
    assert_eq!(app.read_with(cx, |app, _| app.host_store.hosts.len()), 1);

    // Retry after restoring a writable path; deletion now commits and closes the dialog.
    app.update(cx, |app, _| app.host_store.set_path(dir.join("hosts.json")));
    cx.update(|window, cx| {
        modal.update(cx, |view, cx| {
            view.on_action.as_ref().unwrap()(HostModalAction::Delete(host_id), window, cx);
        });
    });
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.active_modal.is_none()));
    assert!(app.read_with(cx, |app, _| app.error_msg.is_none()));
    assert!(
        HostStore::load_from_file(&dir.join("hosts.json"))
            .unwrap()
            .hosts
            .is_empty()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn missing_credentials_open_a_repairable_modal(cx: &mut TestAppContext) {
    let mut host = HostConfig::new("missing", "localhost", "test");
    host.auth = AuthMethod::Password {
        credential_id: "missing-test-reference".into(),
    };
    let mut store = HostStore::new();
    store.add_or_update(host.clone());
    let (app, cx) = cx.add_window_view(|_, cx| test_app(store, cx));
    let fleet = app.read_with(cx, |app, _| app.fleet_view.clone());
    cx.update(|window, cx| {
        fleet.update(cx, |view, cx| {
            view.set_probe_error(
                host.id.clone(),
                ProbeFailure::from_error(&MissingCredential.into()),
                cx,
            );
            view.on_action.as_ref().unwrap()(FleetAction::EditHost(host), window, cx);
        });
    });
    cx.run_until_parked();
    let modal = app.read_with(cx, |app, _| app.active_modal.clone().unwrap());
    modal.update(cx, |view, _| {
        assert_eq!(view.active_field, HostModalField::Password);
        assert!(view.credential_missing);
        assert!(view.submit().is_none());
        assert!(view.error_msg.as_ref().unwrap().contains("密码缺失"));
        view.password = "replacement-for-test-only".into();
        assert!(matches!(
            view.submit(),
            Some(HostModalAction::Save {
                password: Some(_),
                ..
            })
        ));
        view.password.clear();
        view.auth_type = AuthTypeSelection::Agent;
        assert!(view.submit().is_some());
    });
}
