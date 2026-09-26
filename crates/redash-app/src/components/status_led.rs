use crate::components::theme::DarkTechTheme;
use gpui::*;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostLedState {
    Online,
    Warning,
    Critical,
    Offline,
}

#[allow(dead_code)]
impl HostLedState {
    /// Determines the host status based on connection state and resource metrics.
    pub fn from_metrics(is_connected: bool, cpu_usage: f32, mem_usage: f32) -> Self {
        if !is_connected {
            Self::Offline
        } else if cpu_usage.is_nan() || mem_usage.is_nan() {
            Self::Warning
        } else if cpu_usage > 85.0 || mem_usage > 90.0 {
            Self::Critical
        } else if cpu_usage > 70.0 || mem_usage > 75.0 {
            Self::Warning
        } else {
            Self::Online
        }
    }

    /// Returns the (core_color, halo_color) tuple for this state.
    pub fn colors(&self) -> (Hsla, Hsla) {
        match self {
            Self::Online => (
                DarkTechTheme::status_online(),
                DarkTechTheme::status_online_halo(),
            ),
            Self::Warning => (
                DarkTechTheme::status_warn(),
                DarkTechTheme::status_warn_halo(),
            ),
            Self::Critical => (
                DarkTechTheme::status_crit(),
                DarkTechTheme::status_crit_halo(),
            ),
            Self::Offline => (
                DarkTechTheme::status_offline(),
                DarkTechTheme::status_offline_halo(),
            ),
        }
    }

    /// Descriptive human-readable label localized via active locale.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Online => crate::t!("status.online"),
            Self::Warning => crate::t!("status.warn"),
            Self::Critical => crate::t!("status.crit"),
            Self::Offline => crate::t!("status.offline"),
        }
    }
}

#[allow(dead_code)]
pub struct StatusLed {
    pub state: HostLedState,
    pub size: Pixels,
}

#[allow(dead_code)]
impl StatusLed {
    pub fn new(state: HostLedState) -> Self {
        Self {
            state,
            size: px(10.0),
        }
    }

    pub fn with_size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for StatusLed {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (core_color, halo_color) = self.state.colors();
        let core_size = px(f32::from(self.size) * 0.5);

        div()
            .size(self.size)
            .rounded_full()
            .bg(halo_color)
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(core_size).rounded_full().bg(core_color))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_host_led_state_transitions() {
        assert_eq!(
            HostLedState::from_metrics(false, 0.0, 0.0),
            HostLedState::Offline
        );
        assert_eq!(
            HostLedState::from_metrics(true, 45.0, 50.0),
            HostLedState::Online
        );
        assert_eq!(
            HostLedState::from_metrics(true, 72.0, 50.0),
            HostLedState::Warning
        );
        assert_eq!(
            HostLedState::from_metrics(true, 50.0, 78.0),
            HostLedState::Warning
        );
        assert_eq!(
            HostLedState::from_metrics(true, 86.0, 50.0),
            HostLedState::Critical
        );
        assert_eq!(
            HostLedState::from_metrics(true, 50.0, 92.0),
            HostLedState::Critical
        );
    }

    #[core::prelude::v1::test]
    fn test_led_labels() {
        let _guard = redash_core::i18n::TEST_LOCALE_MUTEX.lock().unwrap();
        crate::i18n::I18n::set_locale(crate::i18n::Locale::ZhCn);
        assert_eq!(HostLedState::Online.label(), "在线");
        assert_eq!(HostLedState::Warning.label(), "警告");
        assert_eq!(HostLedState::Critical.label(), "紧急");
        assert_eq!(HostLedState::Offline.label(), "离线");

        crate::i18n::I18n::set_locale(crate::i18n::Locale::EnUs);
        assert_eq!(HostLedState::Online.label(), "Online");
        assert_eq!(HostLedState::Warning.label(), "Warning");
        assert_eq!(HostLedState::Critical.label(), "Critical");
        assert_eq!(HostLedState::Offline.label(), "Offline");
        crate::i18n::I18n::set_locale(crate::i18n::Locale::ZhCn);
    }
}
