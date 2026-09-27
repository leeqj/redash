use crate::app::{ActiveView, AppState, ProcessSortField, SettingsCategory, WorkbenchTab};
use crate::render::LAYOUT;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    OpenTerminal(String),
    OpenSftp(String),
    DeleteHost(String),
    SaveNewHost,
    SendTerminalInput(String),
    FetchSftpList { host_id: String, path: String },
    ReadSftpFile { host_id: String, path: String },
    SaveSftpFile { host_id: String, path: String, content: String },
    SaveSettings,
    ExportSettingsJson,
    ImportSettingsJson,
    PromptPingTarget,
    PromptWebhookUrl,
    RunBatch { host_ids: Vec<String>, command: String },
    ApplyAgentSuggestion,
    AbortAgentTask,
}

pub fn handle_mouse_click(
    state: &mut AppState,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Option<UiAction> {
    // 0. If SFTP Editor Modal is active, check editor modal clicks
    if state.sftp_editor.is_some() {
        let (save_x, save_y, save_w, save_h) = crate::render::get_sftp_editor_save_btn_rect(width, height);
        if (save_x..=save_x + save_w).contains(&x) && (save_y..=save_y + save_h).contains(&y) {
            if let (Some(host_id), Some((path, content))) = (&state.selected_host_id, &state.sftp_editor) {
                return Some(UiAction::SaveSftpFile {
                    host_id: host_id.clone(),
                    path: path.clone(),
                    content: content.clone(),
                });
            }
            return None;
        }

        let (close_x, close_y, close_w, close_h) = crate::render::get_sftp_editor_close_btn_rect(width, height);
        if (close_x..=close_x + close_w).contains(&x) && (close_y..=close_y + close_h).contains(&y) {
            state.close_sftp_editor();
            return None;
        }

        // Absorbs all clicks when editor modal is active
        return None;
    }

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

    // 2.5. If Batch Log Modal is active, check modal clicks
    if state.batch_selected_log_host.is_some() {
        let (close_x, close_y, close_w, close_h) = crate::render::get_batch_log_modal_close_btn_rect(width, height);
        if (close_x..=close_x + close_w).contains(&x) && (close_y..=close_y + close_h).contains(&y) {
            state.select_batch_log_host(None);
            return None;
        }

        let mw = (width - 40.0).min(780.0);
        let mh = (height - 40.0).min(520.0);
        let mx = (width - mw) / 2.0;
        let my = (height - mh) / 2.0;
        if x < mx || x > mx + mw || y < my || y > my + mh {
            state.select_batch_log_host(None);
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
            state.switch_view(ActiveView::Batch);
        } else if (200.0..=240.0).contains(&y) {
            state.switch_view(ActiveView::Sftp);
        } else if (250.0..=290.0).contains(&y) {
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

    // 5. Check Fleet View Card button clicks and search bar
    if state.active_view == ActiveView::Fleet {
        let padding = 24.0;
        let content_x = LAYOUT.sidebar_width;
        let content_w = width - LAYOUT.sidebar_width;
        let card_w = ((content_w - padding * 3.0) / 2.0).max(340.0);
        let card_h = 160.0;
        let start_y = LAYOUT.topbar_height + 54.0;

        // Check Fleet Search Bar
        let (sb_x, sb_y, sb_w, sb_h) = crate::render::get_fleet_search_bar_rect(content_x, LAYOUT.topbar_height, content_w);
        if (sb_x..=sb_x + sb_w).contains(&x) && (sb_y..=sb_y + sb_h).contains(&y) {
            state.set_filter_focused(true);
            return None;
        } else {
            state.set_filter_focused(false);
        }

        let q = state.filter_query.trim().to_lowercase();
        let filtered_ids: Vec<String> = state.hosts.iter().filter_map(|h| {
            if q.is_empty()
                || h.name.to_lowercase().contains(&q)
                || h.hostname.to_lowercase().contains(&q)
                || h.user.to_lowercase().contains(&q)
                || h.tags.iter().any(|t| t.to_lowercase().contains(&q))
            {
                Some(h.id.0.clone())
            } else {
                None
            }
        }).collect();

        for (grid_idx, host_id) in filtered_ids.into_iter().enumerate() {
            let col = grid_idx % 2;
            let row = grid_idx / 2;
            let card_x = content_x + padding + (col as f64) * (card_w + padding);
            let card_y = start_y + (row as f64) * (card_h + padding);

            // 1. Delete button [ ✕ ]
            let (del_x, del_y, del_w, del_h) = crate::render::get_fleet_host_delete_btn_rect(card_x, card_y, card_w);
            if (del_x..=del_x + del_w).contains(&x) && (del_y..=del_y + del_h).contains(&y) {
                return Some(UiAction::DeleteHost(host_id));
            }

            // 2. Action buttons
            let btn_y = card_y + 126.0;
            if (btn_y..=btn_y + 22.0).contains(&y) {
                // [ Terminal ]
                if (card_x + 16.0..=card_x + 86.0).contains(&x) {
                    state.selected_host_id = Some(host_id.clone());
                    state.switch_view(ActiveView::Terminal);
                    return Some(UiAction::OpenTerminal(host_id));
                }
                // [ SFTP ]
                if (card_x + 96.0..=card_x + 146.0).contains(&x) {
                    state.selected_host_id = Some(host_id.clone());
                    state.switch_view(ActiveView::Sftp);
                    return Some(UiAction::OpenSftp(host_id));
                }
            }

            // 3. Card click selects host
            if (card_x..=card_x + card_w).contains(&x) && (card_y..=card_y + card_h).contains(&y) {
                state.selected_host_id = Some(host_id);
                return None;
            }
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

        // Terminal sub-panel clicks: HUD action buttons and search bar
        if state.active_workbench_tab == WorkbenchTab::Terminal {
            let panel_w = width - base_x;
            // 1. Check AI Agent HUD action buttons if agent is active
            if state.agent.is_some() {
                let (ax, ay, aw, ah) = crate::render::get_agent_hud_apply_btn_rect(base_x, panel_y, panel_w);
                if (ax..=ax + aw).contains(&x) && (ay..=ay + ah).contains(&y) {
                    return Some(UiAction::ApplyAgentSuggestion);
                }

                let (ox, oy, ow, oh) = crate::render::get_agent_hud_abort_btn_rect(base_x, panel_y, panel_w);
                if (ox..=ox + ow).contains(&x) && (oy..=oy + oh).contains(&y) {
                    return Some(UiAction::AbortAgentTask);
                }
            }

            // 2. Check Terminal Search Bar close button if active
            if state.terminal_search_active {
                let term_y = panel_y + if state.agent.is_some() { 32.0 } else { 0.0 };
                let (sb_x, sb_y, sb_w, _sb_h) = crate::render::get_terminal_search_bar_rect(base_x, term_y, panel_w);
                let (cx, cy, cw, ch) = crate::render::get_terminal_search_close_btn_rect(sb_x, sb_y, sb_w);
                if (cx..=cx + cw).contains(&x) && (cy..=cy + ch).contains(&y) {
                    state.close_terminal_search();
                    return None;
                }
                if (sb_x..=sb_x + sb_w).contains(&x) && (sb_y..=sb_y + _sb_h).contains(&y) {
                    return None;
                }
            }
        }

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

    // 6.5. Check Batch View clicks
    if state.active_view == ActiveView::Batch {
        let content_x = LAYOUT.sidebar_width;
        let content_y = LAYOUT.topbar_height;
        let content_w = width - LAYOUT.sidebar_width;

        // "全选" button
        let (all_x, all_y, all_w, all_h) = crate::render::get_batch_select_all_btn_rect(content_x, content_y);
        if (all_x..=all_x + all_w).contains(&x) && (all_y..=all_y + all_h).contains(&y) {
            state.select_all_batch_hosts();
            return None;
        }

        // "清空" button
        let (clr_x, clr_y, clr_w, clr_h) = crate::render::get_batch_clear_btn_rect(content_x, content_y);
        if (clr_x..=clr_x + clr_w).contains(&x) && (clr_y..=clr_y + clr_h).contains(&y) {
            state.clear_batch_hosts();
            return None;
        }

        // Host checklist rows
        for (idx, host) in state.hosts.iter().enumerate() {
            let (rx, ry, rw, rh) = crate::render::get_batch_host_row_rect(idx, content_x, content_y);
            if (rx..=rx + rw).contains(&x) && (ry..=ry + rh).contains(&y) {
                state.toggle_batch_host(host.id.0.clone());
                return None;
            }
        }

        // Quick command pills: uptime, df -h, docker ps, free -m
        for (pidx, &cmd) in ["uptime", "df -h", "docker ps", "free -m"].iter().enumerate() {
            let (px, py, pw, ph) = crate::render::get_batch_pill_rect(pidx, content_x, content_y);
            if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                state.set_batch_command(cmd.to_string());
                return None;
            }
        }

        // [ 🚀 并发执行 (Run Batch) ] button
        let (btn_x, btn_y, btn_w, btn_h) = crate::render::get_batch_run_btn_rect(content_x, content_y, content_w);
        if (btn_x..=btn_x + btn_w).contains(&x) && (btn_y..=btn_y + btn_h).contains(&y) {
            if !state.batch_is_running && !state.batch_selected_host_ids.is_empty() && !state.batch_command.trim().is_empty() {
                state.set_batch_running(true);
                return Some(UiAction::RunBatch {
                    host_ids: state.batch_selected_host_ids.iter().cloned().collect(),
                    command: state.batch_command.clone(),
                });
            }
            return None;
        }

        // Detail Log buttons in Execution Waterfall
        for (idx, host) in state.hosts.iter().enumerate() {
            let (lx, ly, lw, lh) = crate::render::get_batch_log_btn_rect(idx, content_x, content_y, content_w);
            if (lx..=lx + lw).contains(&x) && (ly..=ly + lh).contains(&y) {
                state.select_batch_log_host(Some(host.id.0.clone()));
                return None;
            }
        }

        return None;
    }

    // 7. Check SFTP View clicks
    if state.active_view == ActiveView::Sftp {
        let content_x = LAYOUT.sidebar_width;
        let content_y = LAYOUT.topbar_height;
        let content_w = width - LAYOUT.sidebar_width;

        // Check Refresh Button
        let (ref_x, ref_y, ref_w, ref_h) = crate::render::get_sftp_refresh_btn_rect(content_x, content_y, content_w);
        if (ref_x..=ref_x + ref_w).contains(&x) && (ref_y..=ref_y + ref_h).contains(&y) {
            if let Some(host_id) = state.selected_host_id.clone() {
                let path = state.sftp_current_path.clone();
                state.set_sftp_loading(true);
                return Some(UiAction::FetchSftpList {
                    host_id,
                    path,
                });
            }
            return None;
        }

        // Check Parent Directory Button (if not root)
        let is_root = state.sftp_current_path.trim() == "/" || state.sftp_current_path.trim().is_empty();
        if !is_root {
            let (p_x, p_y, p_w, p_h) = crate::render::get_sftp_parent_dir_btn_rect(content_x, content_y);
            if (p_x..=p_x + p_w).contains(&x) && (p_y..=p_y + p_h).contains(&y) {
                let parent = crate::render::get_parent_dir(&state.sftp_current_path);
                state.sftp_current_path = parent.clone();
                state.set_sftp_loading(true);
                if let Some(host_id) = state.selected_host_id.clone() {
                    return Some(UiAction::FetchSftpList {
                        host_id,
                        path: parent,
                    });
                }
                return None;
            }
        }

        // Check Breadcrumb Segment clicks
        let mut clicked_target = None;
        let breadcrumbs = crate::render::get_sftp_breadcrumb_rects(content_x, content_y, &state.sftp_current_path);
        for (target_path, seg_x, seg_y, seg_w, seg_h) in breadcrumbs {
            if (seg_x..=seg_x + seg_w).contains(&x) && (seg_y..=seg_y + seg_h).contains(&y) {
                if target_path != state.sftp_current_path {
                    clicked_target = Some(target_path);
                }
                break;
            }
        }
        if let Some(target_path) = clicked_target {
            state.sftp_current_path = target_path.clone();
            state.set_sftp_loading(true);
            if let Some(host_id) = state.selected_host_id.clone() {
                return Some(UiAction::FetchSftpList {
                    host_id,
                    path: target_path,
                });
            }
            return None;
        }

        // Check File Row clicks
        let mut clicked_file = None;
        for (idx, file) in state.sftp_files.iter().enumerate() {
            let (rx, ry, rw, rh) = crate::render::get_sftp_file_row_rect(idx, content_x, content_y, content_w);
            if (rx..=rx + rw).contains(&x) && (ry..=ry + rh).contains(&y) {
                clicked_file = Some((file.is_dir, file.path.clone()));
                break;
            }
        }
        if let Some((is_dir, file_path)) = clicked_file {
            if is_dir {
                state.sftp_current_path = file_path.clone();
                state.set_sftp_loading(true);
                if let Some(host_id) = state.selected_host_id.clone() {
                    return Some(UiAction::FetchSftpList {
                        host_id,
                        path: file_path,
                    });
                }
            } else if let Some(host_id) = state.selected_host_id.clone() {
                state.set_sftp_loading(true);
                return Some(UiAction::ReadSftpFile {
                    host_id,
                    path: file_path,
                });
            }
            return None;
        }
    }

    // 8. Check Settings View clicks
    if state.active_view == ActiveView::Settings {
        let base_x = LAYOUT.sidebar_width;
        let base_y = LAYOUT.topbar_height;

        // Check Settings Category Sidebar clicks
        if x >= base_x && x <= base_x + crate::render::SETTINGS_SIDEBAR_WIDTH {
            for (idx, (cat, _, _, _)) in crate::render::SETTINGS_CATEGORIES.iter().enumerate() {
                let (ix, iy, iw, ih) = crate::render::get_settings_category_rect(idx, base_x, base_y);
                if (ix..=ix + iw).contains(&x) && (iy..=iy + ih).contains(&y) {
                    state.switch_settings_category(*cat);
                    return None;
                }
            }
        }

        let right_x = base_x + crate::render::SETTINGS_SIDEBAR_WIDTH + 32.0;
        let right_y = base_y + 24.0;

        match state.active_settings_category {
            SettingsCategory::Appearance => {
                let sec1_y = right_y + 56.0;

                // Theme preset cards
                for (idx, preset) in crate::render::THEME_PRESETS.iter().enumerate() {
                    let (cx, cy, cw, ch) = crate::render::get_settings_theme_card_rect(idx, right_x, sec1_y);
                    if (cx..=cx + cw).contains(&x) && (cy..=cy + ch).contains(&y) {
                        state.set_theme(preset.key.to_string());
                        state.settings_save_status = Some((format!("已切换主题至 {}", preset.title), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Language pills
                let sec2_y = sec1_y + 240.0;
                for (idx, (code, name)) in crate::render::LANG_PRESETS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_lang_pill_rect(idx, right_x, sec2_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_locale(code.to_string());
                        state.settings_save_status = Some((format!("已切换语言至 {}", name), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Glow toggle
                let sec3_y = sec2_y + 76.0;
                let (gx, gy, gw, gh) = crate::render::get_settings_glow_toggle_rect(right_x, sec3_y + 16.0);
                if (gx..=gx + gw).contains(&x) && (gy..=gy + gh).contains(&y) {
                    state.toggle_glow();
                    let status_txt = if state.settings.glow_effects_enabled {
                        "微光呼吸动效已开启"
                    } else {
                        "微光呼吸动效已关闭"
                    };
                    state.settings_save_status = Some((status_txt.to_string(), true));
                    return Some(UiAction::SaveSettings);
                }
            }
            SettingsCategory::Terminal => {
                let sec1_y = right_y + 56.0;

                // Font size pills
                for (idx, &size) in crate::render::FONT_SIZES.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_font_size_pill_rect(idx, right_x, sec1_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_terminal_font_size(size);
                        state.settings_save_status = Some((format!("终端字体大小已设置为 {}px", size as u32), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Cursor style pills
                let sec2_y = sec1_y + 76.0;
                for (idx, (style_val, style_label)) in crate::render::CURSOR_STYLES.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_cursor_style_pill_rect(idx, right_x, sec2_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_terminal_cursor_style(style_val.to_string());
                        state.settings_save_status = Some((format!("终端光标样式已设置为 {}", style_label), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Font family pills
                let sec3_y = sec2_y + 76.0;
                for (idx, &font) in crate::render::FONT_FAMILIES.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_font_family_pill_rect(idx, right_x, sec3_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_terminal_font_family(font.to_string());
                        state.settings_save_status = Some((format!("终端字体族已设置为 {}", font), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Scrollback lines pills
                let sec4_y = sec3_y + 76.0;
                for (idx, &lines) in crate::render::SCROLLBACK_OPTIONS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_scrollback_pill_rect(idx, right_x, sec4_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_terminal_scrollback(lines);
                        state.settings_save_status = Some((format!("终端回滚上限已设置为 {} 行", lines), true));
                        return Some(UiAction::SaveSettings);
                    }
                }
            }
            SettingsCategory::Probe => {
                let sec1_y = right_y + 56.0;

                // Probe interval pills
                for (idx, &interval) in crate::render::PROBE_INTERVALS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_probe_interval_pill_rect(idx, right_x, sec1_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_probe_interval(interval);
                        state.settings_save_status = Some((format!("遥测轮询周期已设置为 {} 秒", interval), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Ping target preset pills
                let sec2_y = sec1_y + 76.0;
                for (idx, (ip, _)) in crate::render::PING_PRESETS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_ping_preset_pill_rect(idx, right_x, sec2_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_ping_target(ip.to_string());
                        state.settings_save_status = Some((format!("探测节点已切换为 {}", ip), true));
                        return None;
                    }
                }

                // Ping custom target button
                let (cx, cy, cw, ch) = crate::render::get_settings_ping_custom_btn_rect(right_x, sec2_y);
                if (cx..=cx + cw).contains(&x) && (cy..=cy + ch).contains(&y) {
                    return Some(UiAction::PromptPingTarget);
                }
            }
            SettingsCategory::Alerts => {
                let sec1_y = right_y + 56.0;

                // CPU threshold pills
                for (idx, &opt) in crate::render::ALERT_THRESHOLDS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_cpu_threshold_pill_rect(idx, right_x, sec1_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_cpu_threshold(opt);
                        let label = opt.map(|v| format!("{}%", v as u32)).unwrap_or_else(|| "禁用".to_string());
                        state.settings_save_status = Some((format!("CPU 告警阈值已设置为 {}", label), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Memory threshold pills
                let sec2_y = sec1_y + 76.0;
                for (idx, &opt) in crate::render::ALERT_THRESHOLDS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_mem_threshold_pill_rect(idx, right_x, sec2_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_mem_threshold(opt);
                        let label = opt.map(|v| format!("{}%", v as u32)).unwrap_or_else(|| "禁用".to_string());
                        state.settings_save_status = Some((format!("内存告警阈值已设置为 {}", label), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Disk threshold pills
                let sec3_y = sec2_y + 76.0;
                for (idx, &opt) in crate::render::ALERT_THRESHOLDS.iter().enumerate() {
                    let (px, py, pw, ph) = crate::render::get_settings_disk_threshold_pill_rect(idx, right_x, sec3_y);
                    if (px..=px + pw).contains(&x) && (py..=py + ph).contains(&y) {
                        state.set_disk_threshold(opt);
                        let label = opt.map(|v| format!("{}%", v as u32)).unwrap_or_else(|| "禁用".to_string());
                        state.settings_save_status = Some((format!("磁盘告警阈值已设置为 {}", label), true));
                        return Some(UiAction::SaveSettings);
                    }
                }

                // Webhook buttons: Set & Clear
                let sec4_y = sec3_y + 76.0;
                let (wx, wy, ww, wh) = crate::render::get_settings_webhook_set_btn_rect(right_x, sec4_y);
                if (wx..=wx + ww).contains(&x) && (wy..=wy + wh).contains(&y) {
                    return Some(UiAction::PromptWebhookUrl);
                }

                let (cx, cy, cw, ch) = crate::render::get_settings_webhook_clear_btn_rect(right_x, sec4_y);
                if (cx..=cx + cw).contains(&x) && (cy..=cy + ch).contains(&y) {
                    state.set_webhook_url(None);
                    state.settings_save_status = Some(("已清除 Webhook 推送地址".to_string(), true));
                    return Some(UiAction::SaveSettings);
                }
            }
            SettingsCategory::Backup => {
                let sec1_y = right_y + 56.0;

                // Export JSON button
                let (ex, ey, ew, eh) = crate::render::get_settings_export_json_btn_rect(right_x, sec1_y + 16.0);
                if (ex..=ex + ew).contains(&x) && (ey..=ey + eh).contains(&y) {
                    return Some(UiAction::ExportSettingsJson);
                }

                // Import JSON button
                let (ix, iy, iw, ih) = crate::render::get_settings_import_json_btn_rect(right_x, sec1_y + 16.0);
                if (ix..=ix + iw).contains(&x) && (iy..=iy + ih).contains(&y) {
                    return Some(UiAction::ImportSettingsJson);
                }

                // Reset defaults button
                let sec2_y = sec1_y + 106.0;
                let (rx, ry, rw, rh) = crate::render::get_settings_reset_btn_rect(right_x, sec2_y + 16.0);
                if (rx..=rx + rw).contains(&x) && (ry..=ry + rh).contains(&y) {
                    state.reset_settings();
                    state.settings_save_status = Some(("已成功恢复出厂默认设置！".to_string(), true));
                    return Some(UiAction::SaveSettings);
                }
            }
        }
    }

    None
}

pub fn handle_key_down(state: &mut AppState, key: &str, is_ctrl: bool) -> Option<UiAction> {
    // If Batch Log Modal is open, Escape closes it
    if state.batch_selected_log_host.is_some() {
        if key == "Escape" {
            state.select_batch_log_host(None);
        }
        return None;
    }

    // If SFTP Editor Modal is open:
    if state.sftp_editor.is_some() {
        if key == "Escape" {
            state.close_sftp_editor();
            return None;
        }

        if (key == "s" || key == "S") && is_ctrl {
            if let (Some(host_id), Some((path, content))) = (&state.selected_host_id, &state.sftp_editor) {
                return Some(UiAction::SaveSftpFile {
                    host_id: host_id.clone(),
                    path: path.clone(),
                    content: content.clone(),
                });
            }
            return None;
        }

        if key == "Backspace" {
            if let Some((_, ref mut content)) = state.sftp_editor {
                content.pop();
                state.sftp_editor_modified = true;
            }
            return None;
        }

        if key == "Enter" {
            if let Some((_, ref mut content)) = state.sftp_editor {
                content.push('\n');
                state.sftp_editor_modified = true;
            }
            return None;
        }

        if key.len() == 1 && !is_ctrl {
            if let Some((_, ref mut content)) = state.sftp_editor {
                content.push_str(key);
                state.sftp_editor_modified = true;
            }
            return None;
        }

        return None;
    }

    // If Fleet filter search is focused:
    if state.is_filter_focused {
        match key {
            "Escape" => {
                state.set_filter_focused(false);
                return None;
            }
            "Backspace" => {
                let mut q = state.filter_query.clone();
                q.pop();
                state.filter_query = q;
                return None;
            }
            "Enter" => {
                state.set_filter_focused(false);
                return None;
            }
            c if c.len() == 1 && !is_ctrl => {
                let mut q = state.filter_query.clone();
                q.push_str(c);
                state.filter_query = q;
                return None;
            }
            _ => return None,
        }
    }

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

    // In Terminal View, forward key inputs or handle search
    if state.active_view == ActiveView::Terminal {
        if (key == "f" || key == "F") && is_ctrl {
            state.toggle_terminal_search();
            return None;
        }

        if state.terminal_search_active {
            match key {
                "Escape" => {
                    state.close_terminal_search();
                    return None;
                }
                "Backspace" => {
                    let mut q = state.terminal_search_query.clone();
                    q.pop();
                    state.set_terminal_search_query(q);
                    return None;
                }
                "Enter" => {
                    return None;
                }
                c if c.len() == 1 && !is_ctrl => {
                    let mut q = state.terminal_search_query.clone();
                    q.push_str(c);
                    state.set_terminal_search_query(q);
                    return None;
                }
                _ => return None,
            }
        }

        match key {
            "Enter" => Some(UiAction::SendTerminalInput("\r".to_string())),
            "Backspace" => Some(UiAction::SendTerminalInput("\x08".to_string())),
            c if c.len() == 1 => Some(UiAction::SendTerminalInput(c.to_string())),
            _ => None,
        }
    } else if state.active_view == ActiveView::Batch {
        if key == "Enter" && is_ctrl {
            if !state.batch_is_running && !state.batch_selected_host_ids.is_empty() && !state.batch_command.trim().is_empty() {
                state.set_batch_running(true);
                return Some(UiAction::RunBatch {
                    host_ids: state.batch_selected_host_ids.iter().cloned().collect(),
                    command: state.batch_command.clone(),
                });
            }
            return None;
        }

        match key {
            "Backspace" => {
                let mut cmd = state.batch_command.clone();
                cmd.pop();
                state.set_batch_command(cmd);
                None
            }
            "Enter" => {
                let mut cmd = state.batch_command.clone();
                cmd.push('\n');
                state.set_batch_command(cmd);
                None
            }
            c if c.len() == 1 && !is_ctrl => {
                let mut cmd = state.batch_command.clone();
                cmd.push_str(c);
                state.set_batch_command(cmd);
                None
            }
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

    #[test]
    fn test_sftp_input_and_editor_interactions() {
        let mut state = AppState::new();
        state.selected_host_id = Some("host-sftp-1".to_string());
        state.switch_view(ActiveView::Sftp);
        state.sftp_current_path = "/var/log".to_string();

        let content_x = LAYOUT.sidebar_width;
        let content_y = LAYOUT.topbar_height;
        let content_w = 1200.0 - LAYOUT.sidebar_width;

        // 1. Refresh button click
        let (rx, ry, rw, rh) = crate::render::get_sftp_refresh_btn_rect(content_x, content_y, content_w);
        let action = handle_mouse_click(&mut state, rx + rw / 2.0, ry + rh / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::FetchSftpList {
                host_id: "host-sftp-1".to_string(),
                path: "/var/log".to_string(),
            })
        );
        assert!(state.sftp_loading);

        // 2. Parent directory button click
        state.set_sftp_loading(false);
        let (px, py, pw, ph) = crate::render::get_sftp_parent_dir_btn_rect(content_x, content_y);
        let action = handle_mouse_click(&mut state, px + pw / 2.0, py + ph / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::FetchSftpList {
                host_id: "host-sftp-1".to_string(),
                path: "/var".to_string(),
            })
        );
        assert_eq!(state.sftp_current_path, "/var");

        // 3. Breadcrumb navigation click
        state.set_sftp_loading(false);
        state.sftp_current_path = "/var/log/nginx".to_string();
        let breadcrumbs = crate::render::get_sftp_breadcrumb_rects(content_x, content_y, &state.sftp_current_path);
        // Click on segment 1 which is "/var"
        let (ref target, bx, by, bw, bh) = breadcrumbs[1];
        assert_eq!(target, "/var");
        let action = handle_mouse_click(&mut state, bx + bw / 2.0, by + bh / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::FetchSftpList {
                host_id: "host-sftp-1".to_string(),
                path: "/var".to_string(),
            })
        );
        assert_eq!(state.sftp_current_path, "/var");

        // 4. File Row clicks
        state.set_sftp_loading(false);
        state.sftp_files = vec![
            redash_types::sftp::RemoteFileItem {
                name: "log".to_string(),
                path: "/var/log".to_string(),
                is_dir: true,
                is_symlink: false,
                size: 4096,
                modified: None,
                permissions: 0o755,
            },
            redash_types::sftp::RemoteFileItem {
                name: "app.conf".to_string(),
                path: "/var/app.conf".to_string(),
                is_dir: false,
                is_symlink: false,
                size: 512,
                modified: None,
                permissions: 0o644,
            },
        ];

        // Click directory row (index 0)
        let (r0x, r0y, r0w, r0h) = crate::render::get_sftp_file_row_rect(0, content_x, content_y, content_w);
        let action = handle_mouse_click(&mut state, r0x + r0w / 2.0, r0y + r0h / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::FetchSftpList {
                host_id: "host-sftp-1".to_string(),
                path: "/var/log".to_string(),
            })
        );
        assert_eq!(state.sftp_current_path, "/var/log");

        // Click file row (index 1)
        let (r1x, r1y, r1w, r1h) = crate::render::get_sftp_file_row_rect(1, content_x, content_y, content_w);
        let action = handle_mouse_click(&mut state, r1x + r1w / 2.0, r1y + r1h / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::ReadSftpFile {
                host_id: "host-sftp-1".to_string(),
                path: "/var/app.conf".to_string(),
            })
        );

        // 5. SFTP Editor Modal interaction
        state.open_sftp_editor("/var/app.conf".to_string(), "PORT=8080".to_string());
        assert!(state.sftp_editor.is_some());
        assert!(!state.sftp_editor_modified);

        // Typing characters
        handle_key_down(&mut state, "H", false);
        handle_key_down(&mut state, "O", false);
        handle_key_down(&mut state, "S", false);
        handle_key_down(&mut state, "T", false);
        handle_key_down(&mut state, "=", false);
        assert!(state.sftp_editor_modified);
        assert_eq!(
            state.sftp_editor.as_ref().unwrap().1,
            "PORT=8080HOST="
        );

        // Backspace
        handle_key_down(&mut state, "Backspace", false);
        assert_eq!(
            state.sftp_editor.as_ref().unwrap().1,
            "PORT=8080HOST"
        );

        // Ctrl+S to save
        let action = handle_key_down(&mut state, "s", true);
        assert_eq!(
            action,
            Some(UiAction::SaveSftpFile {
                host_id: "host-sftp-1".to_string(),
                path: "/var/app.conf".to_string(),
                content: "PORT=8080HOST".to_string(),
            })
        );

        // Click Save button
        let (sx, sy, sw, sh) = crate::render::get_sftp_editor_save_btn_rect(1200.0, 800.0);
        let action = handle_mouse_click(&mut state, sx + sw / 2.0, sy + sh / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::SaveSftpFile {
                host_id: "host-sftp-1".to_string(),
                path: "/var/app.conf".to_string(),
                content: "PORT=8080HOST".to_string(),
            })
        );

        // Click Close button
        let (cx, cy, cw, ch) = crate::render::get_sftp_editor_close_btn_rect(1200.0, 800.0);
        let action = handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert!(state.sftp_editor.is_none());

        // Re-open and test Escape key to close
        state.open_sftp_editor("/var/app.conf".to_string(), "test".to_string());
        assert!(state.sftp_editor.is_some());
        let action = handle_key_down(&mut state, "Escape", false);
        assert!(action.is_none());
        assert!(state.sftp_editor.is_none());
    }

    #[test]
    fn test_settings_view_clicks_and_actions() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Settings);
        assert_eq!(state.active_view, ActiveView::Settings);
        assert_eq!(state.active_settings_category, SettingsCategory::Appearance);

        let base_x = LAYOUT.sidebar_width;
        let base_y = LAYOUT.topbar_height;
        let right_x = base_x + crate::render::SETTINGS_SIDEBAR_WIDTH + 32.0;
        let right_y = base_y + 24.0;

        // 1. Switch categories via category sidebar clicks
        // Click Terminal category (idx 1)
        let (cx, cy, cw, ch) = crate::render::get_settings_category_rect(1, base_x, base_y);
        let action = handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert_eq!(state.active_settings_category, SettingsCategory::Terminal);

        // Click Probe category (idx 2)
        let (cx, cy, cw, ch) = crate::render::get_settings_category_rect(2, base_x, base_y);
        handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_settings_category, SettingsCategory::Probe);

        // Click Alerts category (idx 3)
        let (cx, cy, cw, ch) = crate::render::get_settings_category_rect(3, base_x, base_y);
        handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_settings_category, SettingsCategory::Alerts);

        // Click Backup category (idx 4)
        let (cx, cy, cw, ch) = crate::render::get_settings_category_rect(4, base_x, base_y);
        handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_settings_category, SettingsCategory::Backup);

        // Click Appearance category (idx 0)
        let (cx, cy, cw, ch) = crate::render::get_settings_category_rect(0, base_x, base_y);
        handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert_eq!(state.active_settings_category, SettingsCategory::Appearance);

        // 2. Appearance Tab: Theme card clicks
        let sec1_y = right_y + 56.0;
        // Click Cyberpunk card (idx 1)
        let (tx, ty, tw, th) = crate::render::get_settings_theme_card_rect(1, right_x, sec1_y);
        let action = handle_mouse_click(&mut state, tx + tw / 2.0, ty + th / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.theme_name, "Cyberpunk");
        assert!(state.settings_save_status.is_some());

        // Click HighContrast card (idx 3)
        let (tx, ty, tw, th) = crate::render::get_settings_theme_card_rect(3, right_x, sec1_y);
        let action = handle_mouse_click(&mut state, tx + tw / 2.0, ty + th / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.theme_name, "HighContrast");

        // 3. Appearance Tab: Language pill clicks
        let sec2_y = sec1_y + 240.0;
        // Click English (idx 1)
        let (lx, ly, lw, lh) = crate::render::get_settings_lang_pill_rect(1, right_x, sec2_y);
        let action = handle_mouse_click(&mut state, lx + lw / 2.0, ly + lh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.language, "en-US");

        // Click Japanese (idx 3)
        let (lx, ly, lw, lh) = crate::render::get_settings_lang_pill_rect(3, right_x, sec2_y);
        let action = handle_mouse_click(&mut state, lx + lw / 2.0, ly + lh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.language, "ja-JP");

        // 4. Appearance Tab: Glow toggle
        let sec3_y = sec2_y + 76.0;
        let initial_glow = state.settings.glow_effects_enabled;
        let (gx, gy, gw, gh) = crate::render::get_settings_glow_toggle_rect(right_x, sec3_y + 16.0);
        let action = handle_mouse_click(&mut state, gx + gw / 2.0, gy + gh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.glow_effects_enabled, !initial_glow);

        // 5. Terminal Tab: Font size, Cursor style, Font Family, Scrollback pills
        state.switch_settings_category(SettingsCategory::Terminal);
        let term_sec1_y = right_y + 56.0;
        // Click 16px (idx 3)
        let (fx, fy, fw, fh) = crate::render::get_settings_font_size_pill_rect(3, right_x, term_sec1_y);
        let action = handle_mouse_click(&mut state, fx + fw / 2.0, fy + fh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.terminal_font_size, 16.0);

        let term_sec2_y = term_sec1_y + 76.0;
        // Click Underline cursor (idx 2)
        let (ux, uy, uw, uh) = crate::render::get_settings_cursor_style_pill_rect(2, right_x, term_sec2_y);
        let action = handle_mouse_click(&mut state, ux + uw / 2.0, uy + uh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.terminal_cursor_style, "Underline");

        let term_sec3_y = term_sec2_y + 76.0;
        // Click Fira Code font (idx 1)
        let (ffx, ffy, ffw, ffh) = crate::render::get_settings_font_family_pill_rect(1, right_x, term_sec3_y);
        let action = handle_mouse_click(&mut state, ffx + ffw / 2.0, ffy + ffh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.terminal_font_family, "Fira Code");

        let term_sec4_y = term_sec3_y + 76.0;
        // Click 50000 scrollback (idx 3)
        let (sx, sy, sw, sh) = crate::render::get_settings_scrollback_pill_rect(3, right_x, term_sec4_y);
        let action = handle_mouse_click(&mut state, sx + sw / 2.0, sy + sh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.terminal_scrollback_lines, 50000);

        // 6. Probe Tab: Interval pills, Ping presets, Custom button
        state.switch_settings_category(SettingsCategory::Probe);
        let probe_sec1_y = right_y + 56.0;
        // Click 5s interval (idx 2)
        let (ix, iy, iw, ih) = crate::render::get_settings_probe_interval_pill_rect(2, right_x, probe_sec1_y);
        let action = handle_mouse_click(&mut state, ix + iw / 2.0, iy + ih / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.probe_interval_secs, 5);

        let probe_sec2_y = probe_sec1_y + 76.0;
        // Click 8.8.8.8 Ping preset (idx 1)
        let (px, py, pw, ph) = crate::render::get_settings_ping_preset_pill_rect(1, right_x, probe_sec2_y);
        let action = handle_mouse_click(&mut state, px + pw / 2.0, py + ph / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert_eq!(state.ping_target, "8.8.8.8");

        // Click Custom Ping target button
        let (cx, cy, cw, ch) = crate::render::get_settings_ping_custom_btn_rect(right_x, probe_sec2_y);
        let action = handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::PromptPingTarget));

        // 7. Alerts Tab: CPU, Memory, Disk thresholds, Webhook buttons
        state.switch_settings_category(SettingsCategory::Alerts);
        let alert_sec1_y = right_y + 56.0;
        // Click 80% CPU (idx 1)
        let (ax, ay, aw, ah) = crate::render::get_settings_cpu_threshold_pill_rect(1, right_x, alert_sec1_y);
        let action = handle_mouse_click(&mut state, ax + aw / 2.0, ay + ah / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.alert_cpu_threshold, 80.0);

        let alert_sec2_y = alert_sec1_y + 76.0;
        // Click 禁用 Memory (idx 3)
        let (mx, my, mw, mh) = crate::render::get_settings_mem_threshold_pill_rect(3, right_x, alert_sec2_y);
        let action = handle_mouse_click(&mut state, mx + mw / 2.0, my + mh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.alert_mem_threshold, 0.0);

        let alert_sec3_y = alert_sec2_y + 76.0;
        // Click 90% Disk (idx 2)
        let (dx, dy, dw, dh) = crate::render::get_settings_disk_threshold_pill_rect(2, right_x, alert_sec3_y);
        let action = handle_mouse_click(&mut state, dx + dw / 2.0, dy + dh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings.alert_disk_threshold, 90.0);

        let alert_sec4_y = alert_sec3_y + 76.0;
        // Click Set Webhook button
        let (wx, wy, ww, wh) = crate::render::get_settings_webhook_set_btn_rect(right_x, alert_sec4_y);
        let action = handle_mouse_click(&mut state, wx + ww / 2.0, wy + wh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::PromptWebhookUrl));

        // Click Clear Webhook button
        state.settings.alert_webhook_url = Some("https://example.com/webhook".to_string());
        let (cx, cy, cw, ch) = crate::render::get_settings_webhook_clear_btn_rect(right_x, alert_sec4_y);
        let action = handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert!(state.settings.alert_webhook_url.is_none());

        // 8. Backup Tab: Export JSON, Import JSON, Reset Defaults
        state.switch_settings_category(SettingsCategory::Backup);
        let backup_sec1_y = right_y + 56.0;
        // Click Export JSON
        let (ex, ey, ew, eh) = crate::render::get_settings_export_json_btn_rect(right_x, backup_sec1_y + 16.0);
        let action = handle_mouse_click(&mut state, ex + ew / 2.0, ey + eh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::ExportSettingsJson));

        // Click Import JSON
        let (ix, iy, iw, ih) = crate::render::get_settings_import_json_btn_rect(right_x, backup_sec1_y + 16.0);
        let action = handle_mouse_click(&mut state, ix + iw / 2.0, iy + ih / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::ImportSettingsJson));

        // Click Reset Defaults
        let backup_sec2_y = backup_sec1_y + 106.0;
        let (rx, ry, rw, rh) = crate::render::get_settings_reset_btn_rect(right_x, backup_sec2_y + 16.0);
        let action = handle_mouse_click(&mut state, rx + rw / 2.0, ry + rh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::SaveSettings));
        assert_eq!(state.settings, redash_types::settings::AppSettings::default());
    }

    #[test]
    fn test_batch_view_clicks_and_interactions() {
        let mut state = AppState::new();
        let mut h1 = crate::models::HostConfig::new("Web Server 01", "192.168.1.10", "root");
        h1.id = redash_types::HostId("srv-1".to_string());
        let mut h2 = crate::models::HostConfig::new("DB Server 01", "192.168.1.20", "root");
        h2.id = redash_types::HostId("srv-2".to_string());
        state.hosts = vec![h1, h2];
        state.switch_view(ActiveView::Batch);
        assert_eq!(state.active_view, ActiveView::Batch);

        let content_x = LAYOUT.sidebar_width;
        let content_y = LAYOUT.topbar_height;
        let content_w = 1200.0 - LAYOUT.sidebar_width;

        // 1. Select all hosts
        let (ax, ay, aw, ah) = crate::render::get_batch_select_all_btn_rect(content_x, content_y);
        let action = handle_mouse_click(&mut state, ax + aw / 2.0, ay + ah / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert_eq!(state.batch_selected_host_ids.len(), 2);

        // 2. Clear all hosts
        let (cx, cy, cw, ch) = crate::render::get_batch_clear_btn_rect(content_x, content_y);
        let action = handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert!(state.batch_selected_host_ids.is_empty());

        // 3. Toggle host row 0
        let (rx, ry, rw, rh) = crate::render::get_batch_host_row_rect(0, content_x, content_y);
        handle_mouse_click(&mut state, rx + rw / 2.0, ry + rh / 2.0, 1200.0, 800.0);
        assert!(state.batch_selected_host_ids.contains("srv-1"));
        assert_eq!(state.batch_selected_host_ids.len(), 1);

        // Toggle host row 0 again (uncheck)
        handle_mouse_click(&mut state, rx + rw / 2.0, ry + rh / 2.0, 1200.0, 800.0);
        assert!(!state.batch_selected_host_ids.contains("srv-1"));
        assert!(state.batch_selected_host_ids.is_empty());

        // 4. Quick command pills
        let (px, py, pw, ph) = crate::render::get_batch_pill_rect(1, content_x, content_y);
        handle_mouse_click(&mut state, px + pw / 2.0, py + ph / 2.0, 1200.0, 800.0);
        assert_eq!(state.batch_command, "df -h");

        // 5. Run Batch button click
        let (bx, by, bw, bh) = crate::render::get_batch_run_btn_rect(content_x, content_y, content_w);
        // With no hosts selected, should do nothing
        let action = handle_mouse_click(&mut state, bx + bw / 2.0, by + bh / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert!(!state.batch_is_running);

        // Select host 1 and run
        state.toggle_batch_host("srv-2".to_string());
        let action = handle_mouse_click(&mut state, bx + bw / 2.0, by + bh / 2.0, 1200.0, 800.0);
        assert_eq!(
            action,
            Some(UiAction::RunBatch {
                host_ids: vec!["srv-2".to_string()],
                command: "df -h".to_string(),
            })
        );
        assert!(state.batch_is_running);

        // Clicking again while running does nothing
        let action = handle_mouse_click(&mut state, bx + bw / 2.0, by + bh / 2.0, 1200.0, 800.0);
        assert!(action.is_none());

        // 6. Waterfall host log button click
        let (lx, ly, lw, lh) = crate::render::get_batch_log_btn_rect(0, content_x, content_y, content_w);
        let action = handle_mouse_click(&mut state, lx + lw / 2.0, ly + lh / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert_eq!(state.batch_selected_log_host, Some("srv-1".to_string()));

        // 7. Close log modal via modal close button
        let (cl_x, cl_y, cl_w, cl_h) = crate::render::get_batch_log_modal_close_btn_rect(1200.0, 800.0);
        let action = handle_mouse_click(&mut state, cl_x + cl_w / 2.0, cl_y + cl_h / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert!(state.batch_selected_log_host.is_none());

        // Re-open log modal and test Escape key to close
        state.select_batch_log_host(Some("srv-2".to_string()));
        assert_eq!(state.batch_selected_log_host, Some("srv-2".to_string()));
        let action = handle_key_down(&mut state, "Escape", false);
        assert!(action.is_none());
        assert!(state.batch_selected_log_host.is_none());
    }

    #[test]
    fn test_batch_keyboard_interactions() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Batch);
        state.batch_command = "echo".to_string();

        // Type characters
        handle_key_down(&mut state, " ", false);
        handle_key_down(&mut state, "o", false);
        handle_key_down(&mut state, "k", false);
        assert_eq!(state.batch_command, "echo ok");

        // Backspace
        handle_key_down(&mut state, "Backspace", false);
        assert_eq!(state.batch_command, "echo o");

        // Enter adds newline
        handle_key_down(&mut state, "Enter", false);
        assert_eq!(state.batch_command, "echo o\n");

        // Ctrl+Enter triggers RunBatch when hosts are selected
        state.toggle_batch_host("host-alpha".to_string());
        let action = handle_key_down(&mut state, "Enter", true);
        assert_eq!(
            action,
            Some(UiAction::RunBatch {
                host_ids: vec!["host-alpha".to_string()],
                command: "echo o\n".to_string(),
            })
        );
        assert!(state.batch_is_running);
    }

    #[test]
    fn test_terminal_search_and_hud_clicks() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Terminal);
        state.terminal_lines = vec![
            "Starting server...".to_string(),
            "Error: port already in use".to_string(),
            "Retrying...".to_string(),
            "Error: cannot bind socket".to_string(),
        ];

        // 1. Cmd+F / Ctrl+F opens search
        assert!(!state.terminal_search_active);
        let action = handle_key_down(&mut state, "f", true);
        assert!(action.is_none());
        assert!(state.terminal_search_active);

        // 2. Type search query "error"
        for c in ["e", "r", "r", "o", "r"] {
            handle_key_down(&mut state, c, false);
        }
        assert_eq!(state.terminal_search_query, "error");
        assert_eq!(state.terminal_search_match_count, 2);

        // 3. Backspace in search
        handle_key_down(&mut state, "Backspace", false);
        assert_eq!(state.terminal_search_query, "erro");
        assert_eq!(state.terminal_search_match_count, 2);

        // 4. Click search close button
        let base_x = LAYOUT.sidebar_width;
        let panel_y = LAYOUT.topbar_height + crate::render::WORKBENCH_TAB_BAR_HEIGHT;
        let panel_w = 1200.0 - base_x;
        let (sb_x, sb_y, sb_w, _) = crate::render::get_terminal_search_bar_rect(base_x, panel_y, panel_w);
        let (cx, cy, cw, ch) = crate::render::get_terminal_search_close_btn_rect(sb_x, sb_y, sb_w);
        let action = handle_mouse_click(&mut state, cx + cw / 2.0, cy + ch / 2.0, 1200.0, 800.0);
        assert!(action.is_none());
        assert!(!state.terminal_search_active);
        assert!(state.terminal_search_query.is_empty());

        // 5. Escape closes search
        handle_key_down(&mut state, "f", true);
        assert!(state.terminal_search_active);
        handle_key_down(&mut state, "Escape", false);
        assert!(!state.terminal_search_active);

        // 6. Agent HUD buttons
        state.agent = Some(redash_types::agent::DetectedAgent {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            category: "Top Frontier".to_string(),
            status: redash_types::agent::AgentStatus::NeedsInput,
            detail: "Suggested command: rm -rf /tmp/cache".to_string(),
            cost_usd: None,
            tokens: None,
        });

        // Click HUD Apply button
        let (ax, ay, aw, ah) = crate::render::get_agent_hud_apply_btn_rect(base_x, panel_y, panel_w);
        let action = handle_mouse_click(&mut state, ax + aw / 2.0, ay + ah / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::ApplyAgentSuggestion));

        // Click HUD Abort button
        let (ox, oy, ow, oh) = crate::render::get_agent_hud_abort_btn_rect(base_x, panel_y, panel_w);
        let action = handle_mouse_click(&mut state, ox + ow / 2.0, oy + oh / 2.0, 1200.0, 800.0);
        assert_eq!(action, Some(UiAction::AbortAgentTask));
    }

    #[test]
    fn test_fleet_search_filter_and_delete_interaction() {
        let mut state = AppState::new();
        state.switch_view(ActiveView::Fleet);

        let mut h1 = crate::models::HostConfig::new("Alpha-Server", "192.168.1.10", "root");
        h1.id = redash_types::HostId("h1".to_string());
        let mut h2 = crate::models::HostConfig::new("Beta-Worker", "10.0.0.5", "ubuntu");
        h2.id = redash_types::HostId("h2".to_string());
        state.hosts = vec![h1, h2];

        let width = 1200.0;
        let height = 800.0;
        let content_x = LAYOUT.sidebar_width;
        let content_y = LAYOUT.topbar_height;
        let content_w = width - content_x;

        // 1. Click search bar to focus
        let (sb_x, sb_y, _sb_w, sb_h) = crate::render::get_fleet_search_bar_rect(content_x, content_y, content_w);
        assert!(!state.is_filter_focused);
        let action = handle_mouse_click(&mut state, sb_x + 10.0, sb_y + sb_h / 2.0, width, height);
        assert!(action.is_none());
        assert!(state.is_filter_focused);

        // 2. Type "beta" to filter
        for c in ["b", "e", "t", "a"] {
            handle_key_down(&mut state, c, false);
        }
        assert_eq!(state.filter_query, "beta");

        // 3. Backspace
        handle_key_down(&mut state, "Backspace", false);
        assert_eq!(state.filter_query, "bet");

        // Clear query for delete test
        state.filter_query.clear();

        // 4. Click delete button [ ✕ ] on host card 0 (Alpha-Server, "h1")
        let padding = 24.0;
        let card_w = ((content_w - padding * 3.0) / 2.0).max(340.0);
        let card_x = content_x + padding;
        let card_y = content_y + 54.0;
        let (del_x, del_y, del_w, del_h) = crate::render::get_fleet_host_delete_btn_rect(card_x, card_y, card_w);

        let action = handle_mouse_click(&mut state, del_x + del_w / 2.0, del_y + del_h / 2.0, width, height);
        assert_eq!(action, Some(UiAction::DeleteHost("h1".to_string())));

        // 5. Click host card body (select host)
        let action = handle_mouse_click(&mut state, card_x + 50.0, card_y + 50.0, width, height);
        assert!(action.is_none());
        assert_eq!(state.selected_host_id, Some("h1".to_string()));
    }
}
