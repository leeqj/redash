use crate::app::{ActiveView, AppState, ProcessSortField, WorkbenchTab};
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

    // 2. If Docker Log Modal is active, check modal clicks
    if state.docker_log_modal.is_some() {
        let mw = (width - 40.0).min(740.0);
        let mh = (height - 40.0).min(480.0);
        let mx = (width - mw) / 2.0;
        let my = (height - mh) / 2.0;

        // Close button: mx + mw - 86.0, my + 10.0, 70.0, 24.0
        let btn_x = mx + mw - 86.0;
        let btn_y = my + 10.0;
        if (btn_x..=btn_x + 70.0).contains(&x) && (btn_y..=btn_y + 24.0).contains(&y) {
            state.close_docker_logs();
            return None;
        }

        // Click outside modal closes it
        if x < mx || x > mx + mw || y < my || y > my + mh {
            state.close_docker_logs();
            return None;
        }

        return None;
    }

    // 3. Check Left Sidebar clicks
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

    // 4. Check Topbar "+ Add Host" button click
    if y <= LAYOUT.topbar_height {
        let btn_x = width - 90.0;
        if (btn_x..=btn_x + 75.0).contains(&x) && (12.0..=38.0).contains(&y) {
            state.open_add_modal();
            return None;
        }
    }

    // 5. Check Fleet View Card button clicks
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

    // 6. Check Workbench (Terminal View) clicks
    if state.active_view == ActiveView::Terminal {
        let base_x = LAYOUT.sidebar_width;
        let base_y = LAYOUT.topbar_height;

        // Sub-tab bar clicks
        if let Some(tab) = crate::render::get_workbench_tab_at_pos(x, y, base_x, base_y) {
            state.switch_workbench_tab(tab);
            return None;
        }

        let panel_y = base_y + crate::render::WORKBENCH_TAB_BAR_HEIGHT;

        // Processes panel clicks: Sort buttons
        if state.active_workbench_tab == WorkbenchTab::Processes {
            for b_idx in 0..3 {
                let (bx, by, bw, bh) = crate::render::get_process_sort_btn_rect(b_idx, base_x, panel_y);
                if (bx..=bx + bw).contains(&x) && (by..=by + bh).contains(&y) {
                    let sort = match b_idx {
                        0 => ProcessSortField::CpuDesc,
                        1 => ProcessSortField::MemDesc,
                        _ => ProcessSortField::PidAsc,
                    };
                    state.set_process_sort(sort);
                    return None;
                }
            }
        }

        // Docker panel clicks: container action logs button [ 📋 ]
        if state.active_workbench_tab == WorkbenchTab::Docker {
            let metrics = crate::render::get_active_host_metrics(state);
            if let Some(m) = metrics && !m.containers_detail.is_empty() {
                let th_y = panel_y + 42.0;
                let th_h = 28.0;
                let row_h = 36.0;
                let mut row_y = th_y + th_h + 4.0;
                let col_actions_x = base_x + 800.0;

                for c in &m.containers_detail {
                    if row_y + row_h > height - 10.0 {
                        break;
                    }

                    // Check logs button: index 3
                    let (bx, by, bw, bh) = crate::render::get_docker_action_btn_rect(3, col_actions_x, row_y);
                    if (bx..=bx + bw).contains(&x) && (by..=by + bh).contains(&y) {
                        state.open_docker_logs(c.id.clone(), c.name.clone());
                        return None;
                    }

                    row_y += row_h + 4.0;
                }
            }
        }

        // Snippets panel clicks: Categories, Cards buttons, and Drawer close button
        if state.active_workbench_tab == WorkbenchTab::Snippets {
            // Check close button on output drawer if active
            if state.snippet_output.is_some() {
                let drawer_h = 220.0;
                let drawer_y = panel_y + (height - panel_y) - drawer_h;
                let btn_w = 70.0;
                let btn_h = 22.0;
                let btn_x = width - btn_w - 16.0;
                let btn_y = drawer_y + 8.0;
                if (btn_x..=btn_x + btn_w).contains(&x) && (btn_y..=btn_y + btn_h).contains(&y) {
                    state.set_snippet_output(None);
                    return None;
                }
            }

            // Category filter chips
            for (idx, &cat) in crate::render::SNIPPET_CATEGORIES.iter().enumerate() {
                let (cx, cy, cw, ch) = crate::render::get_snippet_category_rect(idx, base_x, panel_y);
                if (cx..=cx + cw).contains(&x) && (cy..=cy + ch).contains(&y) {
                    state.set_snippet_category(cat);
                    return None;
                }
            }

            // Snippet cards buttons: [ 💻 注入终端 ] and [ ⚡ 执行 ]
            let sel_cat = &state.selected_snippet_category;
            let filtered: Vec<&crate::render::SnippetItem> = crate::render::DEFAULT_SNIPPETS
                .iter()
                .filter(|s| {
                    sel_cat == "All" || sel_cat == "全部" || s.category.eq_ignore_ascii_case(sel_cat)
                })
                .collect();

            let grid_y = panel_y + 10.0 + 36.0;
            let card_gap = 12.0;
            let content_w = width - base_x;
            let card_w = ((content_w - 32.0 - card_gap) / 2.0).max(320.0);
            let card_h = 106.0;

            for (idx, s) in filtered.iter().enumerate() {
                let col = idx % 2;
                let row = idx / 2;
                let cx = base_x + 16.0 + (col as f64) * (card_w + card_gap);
                let cy = grid_y + (row as f64) * (card_h + card_gap);

                let bottom_limit = if state.snippet_output.is_some() { height - 230.0 } else { height - 10.0 };
                if cy + card_h > bottom_limit {
                    break;
                }

                let btn_y = cy + 77.0;

                // [ 💻 注入终端 ]
                let inject_w = 95.0;
                let inject_x = cx + card_w - 180.0;
                if (inject_x..=inject_x + inject_w).contains(&x) && (btn_y..=btn_y + 22.0).contains(&y) {
                    state.switch_workbench_tab(WorkbenchTab::Terminal);
                    state.append_terminal_output(&format!("$ {}\r\n", s.command));
                    return Some(UiAction::SendTerminalInput(format!("{}\r", s.command)));
                }

                // [ ⚡ 执行 ]
                let exec_w = 68.0;
                let exec_x = cx + card_w - 78.0;
                if (exec_x..=exec_x + exec_w).contains(&x) && (btn_y..=btn_y + 22.0).contains(&y) {
                    let simulated_output = format!(
                        "$ {}\n[INFO] 正在远端目标主机后台执行脚本...\n[OK] 脚本执行成功 (退出码: 0)\n--- 任务完成 ---",
                        s.command
                    );
                    state.set_snippet_output(Some((s.name.to_string(), simulated_output)));
                    return None;
                }
            }
        }
    }

    None
}

pub fn handle_key_down(state: &mut AppState, key: &str, _is_ctrl: bool) -> Option<UiAction> {
    // If Docker Log Modal is open, Escape closes it
    if state.docker_log_modal.is_some() && key == "Escape" {
        state.close_docker_logs();
        return None;
    }

    // If Snippet Output is open, Escape closes it
    if state.snippet_output.is_some() && key == "Escape" {
        state.set_snippet_output(None);
        return None;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workbench_tab_clicks() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Terminal);
        assert_eq!(state.active_workbench_tab, WorkbenchTab::Terminal);

        // Click on Docker tab (index 1)
        let (x, y, w, h) = crate::render::get_workbench_tab_rect(1, LAYOUT.sidebar_width, LAYOUT.topbar_height);
        let action = handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert_eq!(state.active_workbench_tab, WorkbenchTab::Docker);

        // Click on Processes tab (index 2)
        let (x, y, w, h) = crate::render::get_workbench_tab_rect(2, LAYOUT.sidebar_width, LAYOUT.topbar_height);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_workbench_tab, WorkbenchTab::Processes);

        // Click on Network tab (index 3)
        let (x, y, w, h) = crate::render::get_workbench_tab_rect(3, LAYOUT.sidebar_width, LAYOUT.topbar_height);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_workbench_tab, WorkbenchTab::Network);

        // Click on Tunnels tab (index 4)
        let (x, y, w, h) = crate::render::get_workbench_tab_rect(4, LAYOUT.sidebar_width, LAYOUT.topbar_height);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_workbench_tab, WorkbenchTab::Tunnels);

        // Click on Snippets tab (index 5)
        let (x, y, w, h) = crate::render::get_workbench_tab_rect(5, LAYOUT.sidebar_width, LAYOUT.topbar_height);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_workbench_tab, WorkbenchTab::Snippets);
    }

    #[test]
    fn test_process_sort_clicks() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Terminal);
        state.switch_workbench_tab(WorkbenchTab::Processes);
        assert_eq!(state.process_sort_by, ProcessSortField::CpuDesc);

        let py = LAYOUT.topbar_height + crate::render::WORKBENCH_TAB_BAR_HEIGHT;

        // Click Sort by Mem
        let (x, y, w, h) = crate::render::get_process_sort_btn_rect(1, LAYOUT.sidebar_width, py);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.process_sort_by, ProcessSortField::MemDesc);

        // Click Sort by PID
        let (x, y, w, h) = crate::render::get_process_sort_btn_rect(2, LAYOUT.sidebar_width, py);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.process_sort_by, ProcessSortField::PidAsc);
    }

    #[test]
    fn test_snippet_category_and_drawer_clicks() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Terminal);
        state.switch_workbench_tab(WorkbenchTab::Snippets);
        assert_eq!(state.selected_snippet_category, "All");

        let py = LAYOUT.topbar_height + crate::render::WORKBENCH_TAB_BAR_HEIGHT;

        // Click on Docker category chip (index 2)
        let (x, y, w, h) = crate::render::get_snippet_category_rect(2, LAYOUT.sidebar_width, py);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.selected_snippet_category, "Docker");

        // Click on Network category chip (index 3)
        let (x, y, w, h) = crate::render::get_snippet_category_rect(3, LAYOUT.sidebar_width, py);
        handle_mouse_click(&mut state, x + w / 2.0, y + h / 2.0, 1200.0, 800.0);
        assert_eq!(state.selected_snippet_category, "Network");

        // Set output and close drawer via click
        state.set_snippet_output(Some(("Test Title".to_string(), "Output data".to_string())));
        assert!(state.snippet_output.is_some());

        let drawer_y = py + (800.0 - py) - 220.0;
        let btn_x = 1200.0 - 70.0 - 16.0;
        let btn_y = drawer_y + 8.0;
        handle_mouse_click(&mut state, btn_x + 10.0, btn_y + 10.0, 1200.0, 800.0);
        assert!(state.snippet_output.is_none());
    }

    #[test]
    fn test_docker_log_modal_clicks_and_escape() {
        let mut state = AppState::new();
        state.open_docker_logs("c-123".to_string(), "web-server".to_string());
        assert!(state.docker_log_modal.is_some());

        // Escape closes it
        handle_key_down(&mut state, "Escape", false);
        assert!(state.docker_log_modal.is_none());

        // Re-open and click outside
        state.open_docker_logs("c-123".to_string(), "web-server".to_string());
        assert!(state.docker_log_modal.is_some());
        handle_mouse_click(&mut state, 10.0, 10.0, 1200.0, 800.0);
        assert!(state.docker_log_modal.is_none());
    }
}
