use crate::app::{ActiveView, AppState};
use crate::render::LAYOUT;

pub enum UiAction {
    OpenTerminal(String),
    OpenSftp(String),
    DeleteHost(String),
    SaveNewHost,
    SendTerminalInput(String),
}

pub fn handle_mouse_click(
    state: &mut AppState,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Option<UiAction> {
    // 1. If Modal is active, check modal clicks
    if state.show_add_modal {
        let mw = 420.0;
        let mh = 320.0;
        let mx = (width - mw) / 2.0;
        let my = (height - mh) / 2.0;

        // Cancel button
        let btn_y = my + mh - 42.0;
        if (btn_y..=btn_y + 28.0).contains(&y) {
            if (mx + mw - 180.0..=mx + mw - 110.0).contains(&x) {
                state.close_add_modal();
                return None;
            }
            if (mx + mw - 100.0..=mx + mw - 20.0).contains(&x) {
                state.close_add_modal();
                return Some(UiAction::SaveNewHost);
            }
        }

        // Input field selection
        let mut iy = my + 60.0;
        for idx in 0..4 {
            if (iy + 20.0..=iy + 48.0).contains(&y) && (mx + 20.0..=mx + mw - 20.0).contains(&x) {
                state.modal_field_idx = idx;
                return None;
            }
            iy += 56.0;
        }

        return None;
    }

    // 2. Check Left Sidebar clicks
    if x <= LAYOUT.sidebar_width {
        if (50.0..=90.0).contains(&y) {
            state.switch_view(ActiveView::Fleet);
        } else if (100.0..=140.0).contains(&y) {
            state.switch_view(ActiveView::Terminal);
        } else if (150.0..=190.0).contains(&y) {
            state.switch_view(ActiveView::Sftp);
        } else if (200.0..=240.0).contains(&y) {
            state.switch_view(ActiveView::Settings);
        }
        return None;
    }

    // 3. Check Topbar "+ Add Host" button click
    if y <= LAYOUT.topbar_height {
        let btn_x = width - 90.0;
        if (btn_x..=btn_x + 75.0).contains(&x) && (12.0..=38.0).contains(&y) {
            state.open_add_modal();
            return None;
        }
    }

    // 4. Check Fleet View Card button clicks
    if state.active_view == ActiveView::Fleet {
        let padding = 24.0;
        let content_x = LAYOUT.sidebar_width;
        let content_w = width - LAYOUT.sidebar_width;
        let card_w = ((content_w - padding * 3.0) / 2.0).max(340.0);
        let card_h = 160.0;
        let start_y = LAYOUT.topbar_height + 50.0;

        let mut clicked_terminal = None;
        let mut clicked_sftp = None;

        for (idx, host) in state.hosts.iter().enumerate() {
            let col = idx % 2;
            let row = idx / 2;
            let card_x = content_x + padding + (col as f64) * (card_w + padding);
            let card_y = start_y + (row as f64) * (card_h + padding);

            let btn_y = card_y + 126.0;
            if (btn_y..=btn_y + 22.0).contains(&y) {
                // [ Terminal ]
                if (card_x + 16.0..=card_x + 86.0).contains(&x) {
                    clicked_terminal = Some(host.id.0.clone());
                    break;
                }
                // [ SFTP ]
                if (card_x + 96.0..=card_x + 146.0).contains(&x) {
                    clicked_sftp = Some(host.id.0.clone());
                    break;
                }
            }
        }

        if let Some(hid) = clicked_terminal {
            state.selected_host_id = Some(hid.clone());
            state.switch_view(ActiveView::Terminal);
            return Some(UiAction::OpenTerminal(hid));
        }

        if let Some(hid) = clicked_sftp {
            state.selected_host_id = Some(hid.clone());
            state.switch_view(ActiveView::Sftp);
            return Some(UiAction::OpenSftp(hid));
        }
    }

    None
}

pub fn handle_key_down(state: &mut AppState, key: &str, _is_ctrl: bool) -> Option<UiAction> {
    // If modal is open, type into focused field
    if state.show_add_modal {
        match key {
            "Tab" => {
                state.modal_field_idx = (state.modal_field_idx + 1) % 4;
                return None;
            }
            "Escape" => {
                state.close_add_modal();
                return None;
            }
            "Enter" => {
                state.close_add_modal();
                return Some(UiAction::SaveNewHost);
            }
            "Backspace" => {
                let target = match state.modal_field_idx {
                    0 => &mut state.modal_name,
                    1 => &mut state.modal_hostname,
                    2 => &mut state.modal_port,
                    _ => &mut state.modal_user,
                };
                target.pop();
                return None;
            }
            c if c.len() == 1 => {
                let target = match state.modal_field_idx {
                    0 => &mut state.modal_name,
                    1 => &mut state.modal_hostname,
                    2 => &mut state.modal_port,
                    _ => &mut state.modal_user,
                };
                target.push_str(c);
                return None;
            }
            _ => {}
        }
        return None;
    }

    // In Terminal View, forward key inputs
    if state.active_view == ActiveView::Terminal {
        match key {
            "Enter" => Some(UiAction::SendTerminalInput("\r".to_string())),
            "Backspace" => Some(UiAction::SendTerminalInput("\x08".to_string())),
            c if c.len() == 1 => Some(UiAction::SendTerminalInput(c.to_string())),
            _ => None,
        }
    } else {
        None
    }
}
