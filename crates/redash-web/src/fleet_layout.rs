//! One coordinate system for fleet rendering, scrolling and hit testing.
use crate::app::AppState;
use crate::models::HostConfig;
use crate::render::LAYOUT;

pub fn filtered_hosts(state: &AppState) -> Vec<&HostConfig> {
    let q = state.filter_query.trim().to_lowercase();
    state
        .hosts
        .iter()
        .filter(|h| {
            q.is_empty()
                || h.name.to_lowercase().contains(&q)
                || h.hostname.to_lowercase().contains(&q)
                || h.user.to_lowercase().contains(&q)
                || h.tags.iter().any(|t| t.to_lowercase().contains(&q))
        })
        .collect()
}
pub struct FleetLayout {
    pub top: f64,
    pub bottom: f64,
    pub card_width: f64,
    pub card_height: f64,
    pub columns: usize,
    pub max_scroll: f64,
    pub scroll: f64,
}
impl FleetLayout {
    pub fn new(width: f64, height: f64, count: usize, scroll: f64) -> Self {
        let available = (width - LAYOUT.sidebar_width).max(1.0);
        let columns = if available >= 752.0 { 2 } else { 1 };
        let card_width = ((available - 24.0 * (columns + 1) as f64) / columns as f64).max(1.0);
        let top = LAYOUT.topbar_height + if available < 650.0 { 68.0 } else { 36.0 };
        let bottom = height.max(top);
        let rows = count.div_ceil(columns);
        let max_scroll = (rows as f64 * 220.0 - (bottom - top)).max(0.0);
        Self {
            top,
            bottom,
            card_width,
            card_height: 196.0,
            columns,
            max_scroll,
            scroll: scroll.clamp(0.0, max_scroll),
        }
    }
    pub fn card(&self, index: usize) -> (f64, f64) {
        (
            LAYOUT.sidebar_width + 24.0 + (index % self.columns) as f64 * (self.card_width + 24.0),
            self.top + (index / self.columns) as f64 * 220.0 - self.scroll,
        )
    }
    pub fn visible(&self, y: f64) -> bool {
        y < self.bottom && y + self.card_height > self.top
    }
    pub fn contains_y(&self, y: f64) -> bool {
        (self.top..self.bottom).contains(&y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{UiAction, handle_mouse_click};
    #[test]
    fn last_card_can_be_scrolled_into_view_and_clicked_at_all_widths() {
        for width in [480.0, 768.0, 1024.0, 1512.0] {
            let mut state = AppState::new();
            state.hosts = (0..10)
                .map(|n| HostConfig::new(format!("node-{n}"), "localhost", "user"))
                .collect();
            let layout = FleetLayout::new(width, 600.0, 10, f64::MAX);
            assert!(layout.max_scroll > 0.0);
            let (x, y) = layout.card(9);
            assert!(x + layout.card_width <= width);
            assert!(y + layout.card_height <= 600.0);
            state.fleet_scroll = layout.scroll;
            let (bx, by, _, _) = crate::render::get_fleet_host_action_term_btn_rect(x, y);
            let expected = state.hosts[9].id.0.clone();
            assert_eq!(
                handle_mouse_click(&mut state, bx + 5.0, by + 5.0, width, 600.0),
                Some(UiAction::OpenTerminal(expected))
            );
        }
    }
    #[test]
    fn filter_and_resize_clamp_old_scroll() {
        let layout = FleetLayout::new(1024.0, 800.0, 1, 9999.0);
        assert_eq!(layout.scroll, 0.0);
        assert!(!layout.contains_y(layout.top - 1.0));
    }
}
