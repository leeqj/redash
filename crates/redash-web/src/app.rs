pub use redash_ui_core::state::{
    ActiveView, AppStateMachine, ProcessSortField, SettingsCategory, UiEffect, UserAction,
    WorkbenchTab,
};

pub type AppState = AppStateMachine;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NodeMetrics;

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
        // Buffer preserves up to 1800 points (supporting 60m time range)
        assert_eq!(history.len(), 40);
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

    #[test]
    fn test_mvi_user_action_dispatch() {
        let mut state = AppState::new();
        let effects = state.handle_action(UserAction::SwitchView(ActiveView::Sftp));
        assert_eq!(state.active_view, ActiveView::Sftp);
        assert!(effects.is_empty());

        let effects = state.handle_action(UserAction::SetLocale("en-US".to_string()));
        assert_eq!(state.settings.language, "en-US");
        assert_eq!(effects, vec![UiEffect::SaveSettings]);
    }
}
