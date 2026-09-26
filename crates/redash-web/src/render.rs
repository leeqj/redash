//! Canvas 2D GPUI Web Rendering Engine
//! Renders the entire ReDash UI directly to the HTML5 Canvas in 100% Rust WASM.

use crate::app::{ActiveView, AppState};
use crate::theme::ThemeColors;
use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;

pub struct LayoutBounds {
    pub sidebar_width: f64,
    pub topbar_height: f64,
}

pub const LAYOUT: LayoutBounds = LayoutBounds {
    sidebar_width: 60.0,
    topbar_height: 50.0,
};

pub fn render_frame(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    width: f64,
    height: f64,
) -> Result<(), JsValue> {
    let theme = ThemeColors::from_palette(state.current_palette());

    // 1. Clear background
    ctx.set_fill_style_str(theme.bg_root);
    ctx.fill_rect(0.0, 0.0, width, height);

    // 2. Draw Left Sidebar
    render_sidebar(ctx, state, &theme, height);

    // 3. Draw Topbar
    render_topbar(ctx, state, &theme, width);

    // 4. Draw Active View Area
    let content_x = LAYOUT.sidebar_width;
    let content_y = LAYOUT.topbar_height;
    let content_w = width - LAYOUT.sidebar_width;
    let content_h = height - LAYOUT.topbar_height;

    match state.active_view {
        ActiveView::Fleet => render_fleet_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
        ActiveView::Terminal => render_terminal_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
        ActiveView::Sftp => render_sftp_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
        ActiveView::Settings => render_settings_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
    }

    // 5. Draw Modal if active
    if state.show_add_modal {
        render_add_modal(ctx, state, &theme, width, height);
    }

    Ok(())
}

fn render_sidebar(ctx: &CanvasRenderingContext2d, state: &AppState, theme: &ThemeColors, height: f64) {
    let w = LAYOUT.sidebar_width;

    // Sidebar background
    ctx.set_fill_style_str(theme.bg_sidebar);
    ctx.fill_rect(0.0, 0.0, w, height);

    // Sidebar right border
    ctx.set_stroke_style_str(theme.border_default);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(w, 0.0);
    ctx.line_to(w, height);
    ctx.stroke();

    // Brand "R" Icon
    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.set_font("bold 20px 'JetBrains Mono', monospace");
    ctx.set_text_align("center");
    let _ = ctx.fill_text("R", w / 2.0, 34.0);

    // Nav Items
    let items = [
        (ActiveView::Fleet, "⚡", 70.0),
        (ActiveView::Terminal, ">_", 120.0),
        (ActiveView::Sftp, "📁", 170.0),
        (ActiveView::Settings, "⚙", 220.0),
    ];

    for (view, icon, y) in items {
        let is_active = state.active_view == view;

        if is_active {
            // Active glow indicator pill on left edge
            ctx.set_fill_style_str(theme.accent_cyan);
            ctx.fill_rect(2.0, y - 16.0, 3.0, 32.0);

            // Active item background
            ctx.set_fill_style_str(theme.bg_card);
            ctx.fill_rect(10.0, y - 16.0, 40.0, 32.0);
        }

        ctx.set_fill_style_str(if is_active {
            theme.accent_cyan
        } else {
            theme.text_secondary
        });
        ctx.set_font("14px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(icon, w / 2.0, y + 5.0);
    }
}

fn render_topbar(ctx: &CanvasRenderingContext2d, state: &AppState, theme: &ThemeColors, width: f64) {
    let x = LAYOUT.sidebar_width;
    let h = LAYOUT.topbar_height;
    let w = width - x;

    // Topbar background
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(x, 0.0, w, h);

    // Bottom border
    ctx.set_stroke_style_str(theme.border_default);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(x, h);
    ctx.line_to(width, h);
    ctx.stroke();

    // App Title
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 15px -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("ReDash Web", x + 20.0, 31.0);

    // View Badge with shared i18n
    let badge_text = match state.active_view {
        ActiveView::Fleet => state.t("nav.fleet"),
        ActiveView::Terminal => state.t("nav.terminal"),
        ActiveView::Sftp => state.t("nav.sftp"),
        ActiveView::Settings => state.t("nav.settings"),
    };
    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.set_font("12px 'JetBrains Mono', monospace");
    let _ = ctx.fill_text(badge_text, x + 130.0, 31.0);

    // Right Status Indicator
    let status_x = width - 200.0;
    // Green LED dot
    ctx.set_fill_style_str(theme.status_online);
    ctx.begin_path();
    let _ = ctx.arc(status_x, 25.0, 4.0, 0.0, std::f64::consts::PI * 2.0);
    ctx.fill();

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px -apple-system, sans-serif");
    let _ = ctx.fill_text("Gateway Online", status_x + 10.0, 29.0);

    // Add Host Button
    let btn_x = width - 110.0;
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(btn_x, 12.0, 95.0, 26.0);
    ctx.set_stroke_style_str(theme.accent_cyan);
    ctx.stroke_rect(btn_x, 12.0, 95.0, 26.0);

    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("center");
    let btn_title = format!("+ {}", state.t("host.add_title"));
    let _ = ctx.fill_text(&btn_title, btn_x + 47.5, 29.0);
}

fn render_fleet_view(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let padding = 24.0;
    let card_w = ((w - padding * 3.0) / 2.0).max(340.0);
    let card_h = 160.0;

    // Header Summary with shared i18n
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 16px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text(state.t("nav.fleet"), x + padding, y + 30.0);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px sans-serif");
    let count_text = format!("{} Nodes", state.hosts.len());
    let _ = ctx.fill_text(&count_text, x + padding + 180.0, 30.0 + y);

    let start_y = y + 50.0;

    if state.hosts.is_empty() {
        ctx.set_fill_style_str(theme.text_muted);
        ctx.set_font("14px sans-serif");
        ctx.set_text_align("center");
        let empty_msg = format!("暂无主机节点，请点击上方 '+ {}' 添加服务器", state.t("host.add_title"));
        let _ = ctx.fill_text(
            &empty_msg,
            x + w / 2.0,
            y + 120.0,
        );
        return;
    }

    for (idx, host) in state.hosts.iter().enumerate() {
        let col = idx % 2;
        let row = idx / 2;
        let card_x = x + padding + (col as f64) * (card_w + padding);
        let card_y = start_y + (row as f64) * (card_h + padding);

        if card_y + card_h > y + h {
            break;
        }

        render_host_card(ctx, state, theme, host, card_x, card_y, card_w, card_h);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_host_card(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    host: &crate::models::HostConfig,
    cx: f64,
    cy: f64,
    cw: f64,
    ch: f64,
) {
    // Card Box
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(cx, cy, cw, ch);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(cx, cy, cw, ch);

    // Online Status LED
    ctx.set_fill_style_str(theme.status_online);
    ctx.begin_path();
    let _ = ctx.arc(cx + 16.0, cy + 22.0, 4.0, 0.0, std::f64::consts::PI * 2.0);
    ctx.fill();

    // Host Name
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 14px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text(&host.name, cx + 28.0, cy + 26.0);

    // Host Endpoint
    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px 'JetBrains Mono', monospace");
    let endpoint = format!("{}@{}:{}", host.user, host.hostname, host.port);
    let _ = ctx.fill_text(&endpoint, cx + 28.0, cy + 44.0);

    // Metrics (if available)
    let metrics = state.metrics.get(&host.id.0);
    let cpu_pct = metrics.map(|m| m.cpu_percent()).unwrap_or(0.0);
    let mem_pct = metrics.map(|m| m.mem_percent()).unwrap_or(0.0);

    // CPU Gauge Bar
    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("11px sans-serif");
    let _ = ctx.fill_text("CPU", cx + 16.0, cy + 72.0);
    let _ = ctx.fill_text(&format!("{:.1}%", cpu_pct), cx + 60.0, cy + 72.0);

    let bar_w = cw - 32.0;
    ctx.set_fill_style_str(theme.bg_input);
    ctx.fill_rect(cx + 16.0, cy + 78.0, bar_w, 6.0);

    let cpu_fill_w = (bar_w * (cpu_pct as f64 / 100.0)).clamp(0.0, bar_w);
    ctx.set_fill_style_str(if cpu_pct > 80.0 {
        theme.status_crit
    } else {
        theme.accent_cyan
    });
    ctx.fill_rect(cx + 16.0, cy + 78.0, cpu_fill_w, 6.0);

    // Memory Gauge Bar
    ctx.set_fill_style_str(theme.text_secondary);
    let _ = ctx.fill_text("MEM", cx + 16.0, cy + 102.0);
    let _ = ctx.fill_text(&format!("{:.1}%", mem_pct), cx + 60.0, cy + 102.0);

    ctx.set_fill_style_str(theme.bg_input);
    ctx.fill_rect(cx + 16.0, cy + 108.0, bar_w, 6.0);

    let mem_fill_w = (bar_w * (mem_pct as f64 / 100.0)).clamp(0.0, bar_w);
    ctx.set_fill_style_str(theme.accent_purple);
    ctx.fill_rect(cx + 16.0, cy + 108.0, mem_fill_w, 6.0);

    // Live Sparkline Chart using shared redash-types math
    if let Some(history) = state.metrics_history.get(&host.id.0)
        && history.len() >= 2
    {
        let spark_x = cx + cw - 120.0;
        let spark_y = cy + 20.0;
        let spark_w = 100.0;
        let spark_h = 36.0;

        let points = redash_types::math::normalize_sparkline(history, spark_x, spark_y, spark_w, spark_h);
        if points.len() >= 2 {
            ctx.set_stroke_style_str(theme.accent_cyan);
            ctx.set_line_width(1.5);
            ctx.begin_path();
            ctx.move_to(points[0].0, points[0].1);
            for pt in &points[1..] {
                ctx.line_to(pt.0, pt.1);
            }
            ctx.stroke();
        }
    }

    // Action Buttons
    let btn_y = cy + 126.0;
    // [ Terminal ]
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(cx + 16.0, btn_y, 70.0, 22.0);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(cx + 16.0, btn_y, 70.0, 22.0);
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("11px sans-serif");
    ctx.set_text_align("center");
    let _ = ctx.fill_text(state.t("nav.terminal"), cx + 51.0, btn_y + 15.0);

    // [ SFTP ]
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(cx + 96.0, btn_y, 50.0, 22.0);
    ctx.stroke_rect(cx + 96.0, btn_y, 50.0, 22.0);
    let _ = ctx.fill_text(state.t("nav.sftp"), cx + 121.0, btn_y + 15.0);
}

fn render_terminal_view(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let mut term_y = y;

    // AI Agent HUD banner (if agent is active)
    if let Some(agent) = &state.agent {
        let hud_h = 32.0;
        ctx.set_fill_style_str(theme.bg_card);
        ctx.fill_rect(x, term_y, w, hud_h);
        ctx.set_stroke_style_str(theme.accent_cyan);
        ctx.stroke_rect(x, term_y, w, hud_h);

        // Agent Name
        ctx.set_fill_style_str(theme.accent_cyan);
        ctx.set_font("bold 12px 'JetBrains Mono', monospace");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(&format!("🤖 Agent Active: {}", agent.name), x + 16.0, term_y + 22.0);

        // Status Badge
        ctx.set_fill_style_str(agent.status.color_hex());
        let _ = ctx.fill_text(&format!("Status: {}", agent.status.label()), x + 240.0, term_y + 22.0);

        // Cost / Tokens
        if let Some(cost) = agent.cost_usd {
            ctx.set_fill_style_str(theme.text_secondary);
            let _ = ctx.fill_text(&format!("Cost: ${:.4}", cost), x + 380.0, term_y + 22.0);
        }
        if let Some(tokens) = agent.tokens {
            ctx.set_fill_style_str(theme.text_secondary);
            let _ = ctx.fill_text(&format!("Tokens: {}", tokens), x + 500.0, term_y + 22.0);
        }

        term_y += hud_h;
    }

    // Terminal Screen Background
    let term_h = h - (term_y - y);
    ctx.set_fill_style_str("#090d13");
    ctx.fill_rect(x, term_y, w, term_h);

    // Terminal ANSI Grid Renderer (renders colors, bold, cursor from shared redash-ui-core engine)
    let line_height = 18.0;
    let char_width = 7.8;
    let grid_lines_count = state.terminal_grid.line_count();
    let max_visible_lines = (term_h / line_height) as usize;
    let start_idx = grid_lines_count.saturating_sub(max_visible_lines);

    let mut line_y = term_y + 20.0;
    for line_idx in start_idx..grid_lines_count {
        let cells = state.terminal_grid.line_cells(line_idx);
        let mut char_x = x + 16.0;
        for cell in cells {
            // Draw background if not default
            let bg_color = cell.bg.to_css_color(false);
            if bg_color != "transparent" {
                ctx.set_fill_style_str(bg_color);
                ctx.fill_rect(char_x, line_y - 14.0, char_width, line_height);
            }

            // Draw character with foreground color
            let fg_color = if cell.fg == redash_ui_core::terminal::AnsiColor::Default {
                theme.text_primary
            } else {
                cell.fg.to_css_color(true)
            };
            ctx.set_fill_style_str(fg_color);

            let font_weight = if cell.bold { "bold " } else { "" };
            ctx.set_font(&format!("{}13px 'JetBrains Mono', monospace", font_weight));
            ctx.set_text_align("left");
            let mut s = String::new();
            s.push(cell.c);
            let _ = ctx.fill_text(&s, char_x, line_y);
            char_x += char_width;
        }
        line_y += line_height;
    }

    // Blinking Cursor
    ctx.set_fill_style_str(theme.accent_cyan);
    let last_col = state.terminal_grid.cursor_col;
    let cursor_x = x + 16.0 + (last_col as f64) * char_width;
    ctx.fill_rect(cursor_x, line_y - line_height + 4.0, 8.0, 14.0);
}

fn render_sftp_view(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    _h: f64,
) {
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 16px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text(state.t("nav.sftp"), x + 24.0, y + 36.0);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("13px 'JetBrains Mono', monospace");
    let _ = ctx.fill_text("Current Path: /var/log/redash", x + 24.0, y + 68.0);

    // Mock Directory Table
    let files = [
        ("📁 app/", "DIR", "4.0 KB", "drwxr-xr-x"),
        ("📁 config/", "DIR", "4.0 KB", "drwxr-xr-x"),
        ("📄 server.log", "FILE", "1.2 MB", "-rw-r--r--"),
        ("📄 daemon.log", "FILE", "482 KB", "-rw-r--r--"),
        ("⚙ config.toml", "FILE", "2.1 KB", "-rw-r--r--"),
    ];

    let mut row_y = y + 100.0;
    for (name, ftype, size, perm) in files {
        ctx.set_fill_style_str(theme.bg_card);
        ctx.fill_rect(x + 24.0, row_y, w - 48.0, 32.0);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(x + 24.0, row_y, w - 48.0, 32.0);

        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("13px 'JetBrains Mono', monospace");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(name, x + 36.0, row_y + 20.0);

        ctx.set_fill_style_str(theme.text_secondary);
        let _ = ctx.fill_text(ftype, x + 240.0, row_y + 20.0);
        let _ = ctx.fill_text(size, x + 340.0, row_y + 20.0);
        let _ = ctx.fill_text(perm, x + 460.0, row_y + 20.0);

        row_y += 38.0;
    }
}

fn render_settings_view(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    _w: f64,
    _h: f64,
) {
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 16px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text(state.t("settings.title"), x + 24.0, y + 36.0);

    let items = [
        (state.t("settings.theme_title"), state.settings.theme_name.as_str()),
        (state.t("settings.language_title"), state.settings.language.as_str()),
        (state.t("settings.interval_label"), "2s"),
        ("Terminal Font", "JetBrains Mono (13px)"),
        ("Render Engine", "Canvas 2D / WebGPU 120 FPS"),
    ];

    let mut row_y = y + 70.0;
    for (label, val) in items {
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("13px sans-serif");
        let _ = ctx.fill_text(label, x + 24.0, row_y + 16.0);

        ctx.set_fill_style_str(theme.accent_cyan);
        ctx.set_font("13px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(val, x + 280.0, row_y + 16.0);

        row_y += 36.0;
    }
}

fn render_add_modal(ctx: &CanvasRenderingContext2d, state: &AppState, theme: &ThemeColors, w: f64, h: f64) {
    // Backdrop overlay
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.7)");
    ctx.fill_rect(0.0, 0.0, w, h);

    // Modal Box
    let mw = 420.0;
    let mh = 320.0;
    let mx = (w - mw) / 2.0;
    let my = (h - mh) / 2.0;

    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(mx, my, mw, mh);
    ctx.set_stroke_style_str(theme.accent_cyan);
    ctx.set_line_width(2.0);
    ctx.stroke_rect(mx, my, mw, mh);

    // Title with shared i18n
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 15px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text(state.t("host.add_title"), mx + 20.0, my + 32.0);

    // Input fields with shared i18n labels
    let fields = [
        (state.t("host.name_label"), state.modal_name.as_str()),
        (state.t("host.addr_label"), state.modal_hostname.as_str()),
        (state.t("host.port_label"), state.modal_port.as_str()),
        (state.t("host.user_label"), state.modal_user.as_str()),
    ];

    let mut iy = my + 60.0;
    for (idx, (label, val)) in fields.iter().enumerate() {
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("12px sans-serif");
        let _ = ctx.fill_text(label, mx + 20.0, iy + 14.0);

        let is_focused = state.modal_field_idx == idx;
        ctx.set_fill_style_str(theme.bg_input);
        ctx.fill_rect(mx + 20.0, iy + 20.0, mw - 40.0, 28.0);
        ctx.set_stroke_style_str(if is_focused {
            theme.accent_cyan
        } else {
            theme.border_default
        });
        ctx.set_line_width(1.0);
        ctx.stroke_rect(mx + 20.0, iy + 20.0, mw - 40.0, 28.0);

        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("13px 'JetBrains Mono', monospace");
        let display_val = if val.is_empty() && is_focused { "|" } else { val };
        let _ = ctx.fill_text(display_val, mx + 28.0, iy + 39.0);

        iy += 56.0;
    }

    // Modal Action Buttons with shared i18n
    let btn_y = my + mh - 42.0;

    // [ Cancel ]
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(mx + mw - 180.0, btn_y, 70.0, 28.0);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(mx + mw - 180.0, btn_y, 70.0, 28.0);
    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px sans-serif");
    ctx.set_text_align("center");
    let _ = ctx.fill_text(state.t("host.btn_cancel"), mx + mw - 145.0, btn_y + 18.0);

    // [ Save ]
    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.fill_rect(mx + mw - 100.0, btn_y, 80.0, 28.0);
    ctx.set_fill_style_str(theme.bg_root);
    ctx.set_font("bold 12px sans-serif");
    let _ = ctx.fill_text(state.t("host.btn_submit"), mx + mw - 60.0, btn_y + 18.0);
}
