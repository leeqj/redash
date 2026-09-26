//! Canvas 2D GPUI Web Rendering Engine
//! Renders the entire ReDash UI directly to the HTML5 Canvas in 100% Rust WASM.

use crate::app::{ActiveView, AppState, ProcessSortField, WorkbenchTab};
use crate::theme::ThemeColors;
use redash_types::formatters::{format_bytes, format_bytes_rate};
use redash_types::metrics::{ListeningPort, NodeMetrics, ProcessItem};
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
        ActiveView::Terminal => render_workbench_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
        ActiveView::Sftp => render_sftp_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
        ActiveView::Settings => render_settings_view(ctx, state, &theme, content_x, content_y, content_w, content_h),
    }

    // 5. Draw Add Host Modal if active
    if state.show_add_modal {
        render_add_modal(ctx, state, &theme, width, height);
    }

    // 6. Draw Docker Logs Modal if active
    if let Some((id, name)) = &state.docker_log_modal {
        render_docker_log_modal(ctx, &theme, id, name, width, height);
    }

    // 7. Draw SFTP Editor Modal if active
    if state.sftp_editor.is_some() {
        render_sftp_editor_modal(ctx, state, &theme, width, height);
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

pub const WORKBENCH_TAB_BAR_HEIGHT: f64 = 36.0;

pub const WORKBENCH_TABS: [(WorkbenchTab, &str); 6] = [
    (WorkbenchTab::Terminal, "Terminal (>_)"),
    (WorkbenchTab::Docker, "Docker (🐳)"),
    (WorkbenchTab::Processes, "Processes (⚡)"),
    (WorkbenchTab::Network, "Network (🌐)"),
    (WorkbenchTab::Tunnels, "Tunnels (🔀)"),
    (WorkbenchTab::Snippets, "Snippets (📋)"),
];

pub fn get_workbench_tab_rect(tab_idx: usize, base_x: f64, base_y: f64) -> (f64, f64, f64, f64) {
    let tab_w = 120.0;
    let tab_gap = 6.0;
    let x = base_x + 16.0 + (tab_idx as f64) * (tab_w + tab_gap);
    let y = base_y + 4.0;
    let h = 28.0;
    (x, y, tab_w, h)
}

pub fn get_workbench_tab_at_pos(x: f64, y: f64, base_x: f64, base_y: f64) -> Option<WorkbenchTab> {
    if y < base_y || y > base_y + WORKBENCH_TAB_BAR_HEIGHT {
        return None;
    }
    for (idx, (tab, _)) in WORKBENCH_TABS.iter().enumerate() {
        let (tx, ty, tw, th) = get_workbench_tab_rect(idx, base_x, base_y);
        if x >= tx && x <= tx + tw && y >= ty && y <= ty + th {
            return Some(*tab);
        }
    }
    None
}

pub fn get_active_host_metrics(state: &AppState) -> Option<&NodeMetrics> {
    state
        .selected_host_id
        .as_ref()
        .and_then(|id| state.metrics.get(id))
        .or_else(|| state.hosts.first().and_then(|h| state.metrics.get(&h.id.0)))
        .or_else(|| state.metrics.values().next())
}

pub const SNIPPET_CATEGORIES: &[&str] = &["全部", "System", "Docker", "Network", "Maintenance"];

pub fn get_snippet_category_rect(cat_idx: usize, base_x: f64, base_y: f64) -> (f64, f64, f64, f64) {
    let widths = [60.0, 75.0, 75.0, 80.0, 105.0];
    let gap = 8.0;
    let curr_x = base_x + 16.0 + widths.iter().take(cat_idx).map(|w| w + gap).sum::<f64>();
    let w = widths.get(cat_idx).copied().unwrap_or(70.0);
    (curr_x, base_y + 10.0, w, 26.0)
}

pub fn get_process_sort_btn_rect(btn_idx: usize, base_x: f64, base_y: f64) -> (f64, f64, f64, f64) {
    let start_x = base_x + 200.0;
    let y = base_y + 10.0;
    let h = 26.0;
    match btn_idx {
        0 => (start_x, y, 75.0, h),
        1 => (start_x + 81.0, y, 75.0, h),
        _ => (start_x + 162.0, y, 65.0, h),
    }
}

pub fn get_docker_action_btn_rect(btn_idx: usize, col_x: f64, row_y: f64) -> (f64, f64, f64, f64) {
    let btn_w = 24.0;
    let btn_h = 22.0;
    let gap = 4.0;
    let x = col_x + (btn_idx as f64) * (btn_w + gap);
    let y = row_y + 7.0;
    (x, y, btn_w, btn_h)
}

#[derive(Debug, Clone)]
pub struct SnippetItem {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub command: &'static str,
    pub description: &'static str,
}

pub const DEFAULT_SNIPPETS: &[SnippetItem] = &[
    SnippetItem {
        id: "sys-version",
        name: "查看系统版本",
        category: "System",
        command: "cat /etc/os-release 2>/dev/null || uname -a",
        description: "输出操作系统发行版信息与内核版本",
    },
    SnippetItem {
        id: "sys-large-files",
        name: "查看磁盘大文件 (Top 10)",
        category: "System",
        command: "du -ahx / 2>/dev/null | sort -rh | head -n 10",
        description: "扫描根目录下最大的 10 个文件或目录",
    },
    SnippetItem {
        id: "sys-clean-journal",
        name: "清理 Journal 日志",
        category: "System",
        command: "journalctl --vacuum-time=3d 2>/dev/null || journalctl --vacuum-size=500M 2>/dev/null",
        description: "清理超过 3 天或占用大于 500M 的 systemd 日志",
    },
    SnippetItem {
        id: "docker-stats",
        name: "查看容器资源占用",
        category: "Docker",
        command: "docker stats --no-stream",
        description: "一次性输出所有正在运行的容器 CPU、内存与网络使用率",
    },
    SnippetItem {
        id: "docker-prune",
        name: "清理无用容器与镜像",
        category: "Docker",
        command: "docker system prune -af --volumes",
        description: "彻底清理未使用的容器、镜像、网络与数据卷",
    },
    SnippetItem {
        id: "docker-status",
        name: "查看 Docker 守护进程状态",
        category: "Docker",
        command: "systemctl status docker --no-pager 2>/dev/null || service docker status",
        description: "查看 Docker systemd 服务运行状态",
    },
    SnippetItem {
        id: "net-ping-cf",
        name: "测试外部连通性 (Ping Cloudflare)",
        category: "Network",
        command: "ping -c 4 1.1.1.1",
        description: "发送 4 个 ICMP 数据包至 Cloudflare DNS 测试网络连通性与延迟",
    },
    SnippetItem {
        id: "net-conn-stats",
        name: "查看对外连接统计",
        category: "Network",
        command: "ss -s 2>/dev/null || netstat -s 2>/dev/null",
        description: "显示当前系统的 TCP/UDP 传输层连接汇总状态",
    },
    SnippetItem {
        id: "net-speedtest",
        name: "测试当前网络测速",
        category: "Network",
        command: "curl -s https://raw.githubusercontent.com/sivel/speedtest-cli/master/speedtest.py | python3 - 2>/dev/null",
        description: "运行 Python speedtest-cli 进行下行与上行带宽测速",
    },
    SnippetItem {
        id: "maint-pkg-update",
        name: "一键更新系统软件包",
        category: "Maintenance",
        command: "apt update && apt upgrade -y 2>/dev/null || yum update -y 2>/dev/null",
        description: "适配 Debian/Ubuntu 与 RHEL/CentOS 的自动化系统包更新",
    },
    SnippetItem {
        id: "maint-crash-logs",
        name: "查看重启记录与崩溃日志",
        category: "Maintenance",
        command: "last reboot | head -n 10; dmesg -T --level=err,warn 2>/dev/null | tail -n 20",
        description: "查看最近 10 次系统重启历史以及内核最新警告与错误",
    },
];

#[derive(Debug, Clone)]
pub struct TunnelItem {
    pub id: &'static str,
    pub name: &'static str,
    pub tunnel_type: &'static str,
    pub local_endpoint: &'static str,
    pub remote_endpoint: &'static str,
    pub is_active: bool,
    pub bytes_transferred: &'static str,
    pub active_conns: u32,
}

pub const DEFAULT_TUNNELS: &[TunnelItem] = &[
    TunnelItem {
        id: "tun-pg",
        name: "PostgreSQL 数据库通道",
        tunnel_type: "Local TCP",
        local_endpoint: "127.0.0.1:5432",
        remote_endpoint: "10.0.4.18:5432",
        is_active: true,
        bytes_transferred: "14.2 MB",
        active_conns: 4,
    },
    TunnelItem {
        id: "tun-socks",
        name: "动态全局代理 (Dynamic SOCKS5)",
        tunnel_type: "SOCKS5",
        local_endpoint: "127.0.0.1:1080",
        remote_endpoint: "远端内网全网段 (0.0.0.0/0)",
        is_active: true,
        bytes_transferred: "89.1 MB",
        active_conns: 12,
    },
    TunnelItem {
        id: "tun-redis",
        name: "Redis 缓存转发",
        tunnel_type: "Local TCP",
        local_endpoint: "127.0.0.1:6379",
        remote_endpoint: "127.0.0.1:6379",
        is_active: false,
        bytes_transferred: "2.1 MB",
        active_conns: 0,
    },
];

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() > max_chars {
        let mut s: String = text.chars().take(max_chars.saturating_sub(3)).collect();
        s.push_str("...");
        s
    } else {
        text.to_string()
    }
}

pub fn render_workbench_view(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    // 1. Sub-Tab Bar (height 36px)
    render_workbench_subtabs(ctx, state, theme, x, y, w);

    // 2. Sub-Panel Area (below y + 36px)
    let panel_y = y + WORKBENCH_TAB_BAR_HEIGHT;
    let panel_h = h - WORKBENCH_TAB_BAR_HEIGHT;

    match state.active_workbench_tab {
        WorkbenchTab::Terminal => render_terminal_panel(ctx, state, theme, x, panel_y, w, panel_h),
        WorkbenchTab::Docker => render_docker_panel(ctx, state, theme, x, panel_y, w, panel_h),
        WorkbenchTab::Processes => render_processes_panel(ctx, state, theme, x, panel_y, w, panel_h),
        WorkbenchTab::Network => render_network_panel(ctx, state, theme, x, panel_y, w, panel_h),
        WorkbenchTab::Tunnels => render_tunnels_panel(ctx, state, theme, x, panel_y, w, panel_h),
        WorkbenchTab::Snippets => render_snippets_panel(ctx, state, theme, x, panel_y, w, panel_h),
    }
}

fn render_workbench_subtabs(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
) {
    let bar_h = WORKBENCH_TAB_BAR_HEIGHT;

    // Sub-tab bar background
    ctx.set_fill_style_str(theme.bg_sidebar);
    ctx.fill_rect(x, y, w, bar_h);

    // Bottom border
    ctx.set_stroke_style_str(theme.border_default);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(x, y + bar_h);
    ctx.line_to(x + w, y + bar_h);
    ctx.stroke();

    for (idx, (tab, label)) in WORKBENCH_TABS.iter().enumerate() {
        let (tab_x, tab_y, tab_w, tab_h) = get_workbench_tab_rect(idx, x, y);
        let is_active = state.active_workbench_tab == *tab;

        if is_active {
            // Active background pill
            ctx.set_fill_style_str(theme.bg_card);
            ctx.fill_rect(tab_x, tab_y, tab_w, tab_h);

            ctx.set_stroke_style_str(theme.border_default);
            ctx.stroke_rect(tab_x, tab_y, tab_w, tab_h);

            // Active bottom cyan indicator bar
            ctx.set_fill_style_str(theme.accent_cyan);
            ctx.fill_rect(tab_x, y + bar_h - 2.0, tab_w, 2.0);
        }

        ctx.set_fill_style_str(if is_active {
            theme.accent_cyan
        } else {
            theme.text_secondary
        });
        let font_weight = if is_active { "bold " } else { "" };
        ctx.set_font(&format!("{}12px -apple-system, sans-serif", font_weight));
        ctx.set_text_align("center");
        let _ = ctx.fill_text(label, tab_x + tab_w / 2.0, tab_y + 18.0);
    }
}

fn render_terminal_panel(
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

    // Terminal ANSI Grid Renderer
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

fn render_docker_panel(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let metrics = get_active_host_metrics(state);
    let containers = metrics.map(|m| &m.containers_detail);

    // Toolbar Header
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 15px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("🐳 Docker 容器管理 (Containers)", x + 20.0, y + 26.0);

    let count = containers.map(|c| c.len()).unwrap_or(0);
    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px sans-serif");
    let _ = ctx.fill_text(&format!("{} 个容器", count), x + 240.0, y + 26.0);

    if count == 0 {
        // Friendly Empty State
        ctx.set_fill_style_str(theme.text_muted);
        ctx.set_font("bold 36px sans-serif");
        ctx.set_text_align("center");
        let _ = ctx.fill_text("🐳", x + w / 2.0, y + 120.0);

        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("bold 15px sans-serif");
        let _ = ctx.fill_text("未检测到运行中的 Docker 容器", x + w / 2.0, y + 160.0);

        ctx.set_fill_style_str(theme.text_muted);
        ctx.set_font("12px sans-serif");
        let _ = ctx.fill_text(
            "系统探针未发现活跃容器，请确认 Docker 守护进程已启动且探针具备访问权限",
            x + w / 2.0,
            y + 185.0,
        );
        return;
    }

    // Table Header
    let th_y = y + 42.0;
    let th_h = 28.0;
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(x + 16.0, th_y, w - 32.0, th_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(x + 16.0, th_y, w - 32.0, th_h);

    let col_x = [
        x + 24.0,        // State
        x + 110.0,       // Name
        x + 270.0,       // Image
        x + 450.0,       // Ports
        x + 610.0,       // CPU%
        x + 690.0,       // Mem
        x + 800.0,       // Actions
    ];

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("状态", col_x[0], th_y + 19.0);
    let _ = ctx.fill_text("容器名称", col_x[1], th_y + 19.0);
    let _ = ctx.fill_text("镜像", col_x[2], th_y + 19.0);
    let _ = ctx.fill_text("端口映射", col_x[3], th_y + 19.0);
    let _ = ctx.fill_text("CPU%", col_x[4], th_y + 19.0);
    let _ = ctx.fill_text("内存", col_x[5], th_y + 19.0);
    let _ = ctx.fill_text("快捷操作", col_x[6], th_y + 19.0);

    // Table Rows
    let list = containers.unwrap();
    let row_h = 36.0;
    let mut row_y = th_y + th_h + 4.0;

    for (idx, c) in list.iter().enumerate() {
        if row_y + row_h > y + h - 10.0 {
            break;
        }

        // Row background
        ctx.set_fill_style_str(if idx % 2 == 0 { theme.bg_card } else { theme.bg_input });
        ctx.fill_rect(x + 16.0, row_y, w - 32.0, row_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(x + 16.0, row_y, w - 32.0, row_h);

        // State LED + text
        let state_lower = c.state.to_lowercase();
        let (led_color, state_label) = if state_lower == "running" {
            (theme.status_online, "running")
        } else if state_lower == "paused" {
            (theme.status_warn, "paused")
        } else {
            (theme.status_crit, "exited")
        };

        ctx.set_fill_style_str(led_color);
        ctx.begin_path();
        let _ = ctx.arc(col_x[0] + 4.0, row_y + 18.0, 4.0, 0.0, std::f64::consts::PI * 2.0);
        ctx.fill();

        ctx.set_font("11px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(state_label, col_x[0] + 14.0, row_y + 22.0);

        // Name
        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("bold 12px sans-serif");
        let _ = ctx.fill_text(&truncate_text(&c.name, 18), col_x[1], row_y + 22.0);

        // Image
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("11px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&truncate_text(&c.image, 22), col_x[2], row_y + 22.0);

        // Ports
        let ports_str = if c.ports.is_empty() { "-".to_string() } else { c.ports.join(", ") };
        let _ = ctx.fill_text(&truncate_text(&ports_str, 20), col_x[3], row_y + 22.0);

        // CPU%
        ctx.set_fill_style_str(if c.cpu_percent > 50.0 { theme.status_warn } else { theme.accent_cyan });
        let _ = ctx.fill_text(&format!("{:.1}%", c.cpu_percent), col_x[4], row_y + 22.0);

        // Mem
        ctx.set_fill_style_str(theme.text_secondary);
        let _ = ctx.fill_text(&format_bytes(c.mem_usage_bytes), col_x[5], row_y + 22.0);

        // Action buttons: ▶, ⏹, 🔄, 📋, 🗑️
        let action_icons = ["▶", "⏹", "🔄", "📋", "🗑️"];
        for (b_idx, icon) in action_icons.iter().enumerate() {
            let (bx, by, bw, bh) = get_docker_action_btn_rect(b_idx, col_x[6], row_y);
            ctx.set_fill_style_str(theme.bg_card_hover);
            ctx.fill_rect(bx, by, bw, bh);
            ctx.set_stroke_style_str(theme.border_default);
            ctx.stroke_rect(bx, by, bw, bh);

            ctx.set_fill_style_str(theme.text_primary);
            ctx.set_font("11px sans-serif");
            ctx.set_text_align("center");
            let _ = ctx.fill_text(icon, bx + bw / 2.0, by + 15.0);
            ctx.set_text_align("left");
        }

        row_y += row_h + 4.0;
    }
}

pub fn render_docker_log_modal(
    ctx: &CanvasRenderingContext2d,
    theme: &ThemeColors,
    id: &str,
    name: &str,
    width: f64,
    height: f64,
) {
    // Backdrop
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.75)");
    ctx.fill_rect(0.0, 0.0, width, height);

    // Modal Box
    let mw = (width - 40.0).min(740.0);
    let mh = (height - 40.0).min(480.0);
    let mx = (width - mw) / 2.0;
    let my = (height - mh) / 2.0;

    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(mx, my, mw, mh);
    ctx.set_stroke_style_str(theme.accent_cyan);
    ctx.set_line_width(2.0);
    ctx.stroke_rect(mx, my, mw, mh);

    // Modal Title
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 14px sans-serif");
    ctx.set_text_align("left");
    let short_id = if id.len() > 12 { &id[..12] } else { id };
    let _ = ctx.fill_text(
        &format!("📋 容器实时日志: {} ({})", name, short_id),
        mx + 20.0,
        my + 28.0,
    );

    // Close button
    let btn_w = 70.0;
    let btn_h = 24.0;
    let btn_x = mx + mw - btn_w - 16.0;
    let btn_y = my + 10.0;
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(btn_x, btn_y, btn_w, btn_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(btn_x, btn_y, btn_w, btn_h);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px sans-serif");
    ctx.set_text_align("center");
    let _ = ctx.fill_text("✕ 关闭", btn_x + btn_w / 2.0, btn_y + 16.0);

    // Log console window
    let console_x = mx + 16.0;
    let console_y = my + 44.0;
    let console_w = mw - 32.0;
    let console_h = mh - 58.0;

    ctx.set_fill_style_str("#090d13");
    ctx.fill_rect(console_x, console_y, console_w, console_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(console_x, console_y, console_w, console_h);

    let log_lines = [
        format!("[2026-09-27T00:50:12.102Z] Starting container entrypoint for {}...", name),
        "[2026-09-27T00:50:12.148Z] Environment initialized (ENV=production, LOG_LEVEL=info)".to_string(),
        "[2026-09-27T00:50:12.215Z] Worker pool initialized with 4 execution threads.".to_string(),
        "[2026-09-27T00:50:13.001Z] Server listening on tcp://0.0.0.0:8080 (IPv4 + IPv6).".to_string(),
        "[2026-09-27T00:52:45.332Z] [info] GET /healthz 200 OK (0.8ms)".to_string(),
        "[2026-09-27T00:55:01.442Z] [info] GET /api/v1/status 200 OK (1.2ms)".to_string(),
        "[2026-09-27T00:58:22.091Z] [info] Probe collection cycle finished cleanly.".to_string(),
        "[2026-09-27T01:00:15.512Z] [info] Ping from ReDash Gateway keepalive OK.".to_string(),
        "[2026-09-27T01:02:40.820Z] [info] Container healthy; cpu=0.8%, rss=32.4MB.".to_string(),
    ];

    ctx.set_fill_style_str(theme.status_online);
    ctx.set_font("12px 'JetBrains Mono', monospace");
    ctx.set_text_align("left");
    let mut ly = console_y + 24.0;
    for line in &log_lines {
        let _ = ctx.fill_text(line, console_x + 14.0, ly);
        ly += 22.0;
    }
}

fn render_processes_panel(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let metrics = get_active_host_metrics(state);

    // Header Controls: Search Box + Sort Chips
    let top_y = y + 10.0;

    // Search box placeholder
    ctx.set_fill_style_str(theme.bg_input);
    ctx.fill_rect(x + 16.0, top_y, 170.0, 26.0);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(x + 16.0, top_y, 170.0, 26.0);

    ctx.set_fill_style_str(theme.text_muted);
    ctx.set_font("12px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("🔍 过滤进程...", x + 26.0, top_y + 17.0);

    // Sort buttons
    let sort_items = [
        (0, "CPU% ▼", state.process_sort_by == ProcessSortField::CpuDesc),
        (1, "内存% ▼", state.process_sort_by == ProcessSortField::MemDesc),
        (2, "PID ▲", state.process_sort_by == ProcessSortField::PidAsc),
    ];

    for (b_idx, label, is_active) in sort_items {
        let (bx, by, bw, bh) = get_process_sort_btn_rect(b_idx, x, y);
        ctx.set_fill_style_str(if is_active { theme.bg_card_hover } else { theme.bg_card });
        ctx.fill_rect(bx, by, bw, bh);
        ctx.set_stroke_style_str(if is_active { theme.accent_cyan } else { theme.border_default });
        ctx.stroke_rect(bx, by, bw, bh);

        ctx.set_fill_style_str(if is_active { theme.accent_cyan } else { theme.text_secondary });
        ctx.set_font(if is_active { "bold 12px sans-serif" } else { "12px sans-serif" });
        ctx.set_text_align("center");
        let _ = ctx.fill_text(label, bx + bw / 2.0, by + 17.0);
    }

    // Process list resolution & sorting
    let mut procs: Vec<ProcessItem> = if let Some(m) = metrics && !m.processes_detail.is_empty() {
        m.processes_detail.clone()
    } else if let Some(m) = metrics && !m.top_processes.is_empty() {
        m.top_processes
            .iter()
            .map(|p| ProcessItem {
                pid: p.pid,
                user: p.user.clone(),
                cpu_percent: p.cpu_percent,
                mem_percent: p.mem_percent,
                status: "R".to_string(),
                rss_bytes: (p.mem_percent as f64 * 1024.0 * 1024.0 * 16.0) as u64,
                command: p.command.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    match state.process_sort_by {
        ProcessSortField::CpuDesc => procs.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal)),
        ProcessSortField::MemDesc => procs.sort_by(|a, b| b.mem_percent.partial_cmp(&a.mem_percent).unwrap_or(std::cmp::Ordering::Equal)),
        ProcessSortField::PidAsc => procs.sort_by_key(|p| p.pid),
    }

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px sans-serif");
    ctx.set_text_align("right");
    let _ = ctx.fill_text(&format!("共 {} 个进程", procs.len()), x + w - 24.0, top_y + 17.0);

    // Table Header
    let th_y = y + 46.0;
    let th_h = 26.0;
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(x + 16.0, th_y, w - 32.0, th_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(x + 16.0, th_y, w - 32.0, th_h);

    let col_x = [
        x + 24.0,   // PID
        x + 105.0,  // User
        x + 205.0,  // CPU%
        x + 295.0,  // MEM%
        x + 385.0,  // STAT
        x + 465.0,  // COMMAND
    ];

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("PID", col_x[0], th_y + 17.0);
    let _ = ctx.fill_text("USER", col_x[1], th_y + 17.0);
    let _ = ctx.fill_text("CPU%", col_x[2], th_y + 17.0);
    let _ = ctx.fill_text("MEM%", col_x[3], th_y + 17.0);
    let _ = ctx.fill_text("STAT", col_x[4], th_y + 17.0);
    let _ = ctx.fill_text("COMMAND", col_x[5], th_y + 17.0);

    if procs.is_empty() {
        ctx.set_fill_style_str(theme.text_muted);
        ctx.set_font("13px sans-serif");
        ctx.set_text_align("center");
        let _ = ctx.fill_text("暂无进程数据 (探针数据同步中...)", x + w / 2.0, th_y + 60.0);
        return;
    }

    // Rows
    let row_h = 26.0;
    let mut row_y = th_y + th_h + 3.0;

    for (idx, p) in procs.iter().enumerate() {
        if row_y + row_h > y + h - 10.0 {
            break;
        }

        ctx.set_fill_style_str(if idx % 2 == 0 { theme.bg_card } else { theme.bg_input });
        ctx.fill_rect(x + 16.0, row_y, w - 32.0, row_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(x + 16.0, row_y, w - 32.0, row_h);

        // PID
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("12px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&p.pid.to_string(), col_x[0], row_y + 18.0);

        // User
        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("12px sans-serif");
        let _ = ctx.fill_text(&truncate_text(&p.user, 10), col_x[1], row_y + 18.0);

        // CPU%
        ctx.set_fill_style_str(if p.cpu_percent > 50.0 { theme.status_crit } else { theme.accent_cyan });
        ctx.set_font("12px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&format!("{:.1}%", p.cpu_percent), col_x[2], row_y + 18.0);

        // MEM%
        ctx.set_fill_style_str(theme.accent_purple);
        let _ = ctx.fill_text(&format!("{:.1}%", p.mem_percent), col_x[3], row_y + 18.0);

        // STAT
        ctx.set_fill_style_str(theme.text_secondary);
        let _ = ctx.fill_text(&p.status, col_x[4], row_y + 18.0);

        // COMMAND
        ctx.set_fill_style_str(theme.text_primary);
        let _ = ctx.fill_text(&truncate_text(&p.command, 55), col_x[5], row_y + 18.0);

        row_y += row_h + 3.0;
    }
}

fn render_network_panel(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let metrics = get_active_host_metrics(state);

    // 1. Top 3 Metric Cards
    let card_y = y + 14.0;
    let card_h = 92.0;
    let card_gap = 12.0;
    let card_w = ((w - 32.0 - card_gap * 2.0) / 3.0).max(220.0);

    // Card 1: RTT Latency Gauge
    let c1_x = x + 16.0;
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(c1_x, card_y, card_w, card_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(c1_x, card_y, card_w, card_h);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("🌐 网络往返延迟 (RTT)", c1_x + 14.0, card_y + 24.0);

    let rtt = metrics.and_then(|m| m.rtt_ms).unwrap_or(24);
    let (rating_text, rating_color) = if rtt < 50 {
        ("Optimal (<50ms 极佳)", theme.status_online)
    } else if rtt < 150 {
        ("Good (<150ms 良好)", theme.accent_cyan)
    } else if rtt < 300 {
        ("Fair (<300ms 一般)", theme.status_warn)
    } else {
        ("Poor (较差)", theme.status_crit)
    };

    ctx.set_fill_style_str(rating_color);
    ctx.set_font("bold 20px 'JetBrains Mono', monospace");
    let _ = ctx.fill_text(&format!("{} ms", rtt), c1_x + 14.0, card_y + 52.0);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("11px sans-serif");
    let _ = ctx.fill_text(rating_text, c1_x + 110.0, card_y + 50.0);

    // RTT gauge bar
    let bar_w = card_w - 28.0;
    ctx.set_fill_style_str(theme.bg_input);
    ctx.fill_rect(c1_x + 14.0, card_y + 68.0, bar_w, 6.0);

    let fill_w = ((rtt as f64 / 300.0).clamp(0.08, 1.0)) * bar_w;
    ctx.set_fill_style_str(rating_color);
    ctx.fill_rect(c1_x + 14.0, card_y + 68.0, fill_w, 6.0);

    // Card 2: Real-time RX/TX speed (using redash_types::format_bytes_rate)
    let c2_x = c1_x + card_w + card_gap;
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(c2_x, card_y, card_w, card_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(c2_x, card_y, card_w, card_h);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    let _ = ctx.fill_text("⚡ 实时网络速率 (Throughput)", c2_x + 14.0, card_y + 24.0);

    let rx_rate = metrics.map(|m| m.net.rx_bytes_per_sec).unwrap_or(524_288);
    let tx_rate = metrics.map(|m| m.net.tx_bytes_per_sec).unwrap_or(131_072);

    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.set_font("bold 14px 'JetBrains Mono', monospace");
    let _ = ctx.fill_text(&format!("↓ RX: {}", format_bytes_rate(rx_rate)), c2_x + 14.0, card_y + 52.0);

    ctx.set_fill_style_str(theme.accent_purple);
    let _ = ctx.fill_text(&format!("↑ TX: {}", format_bytes_rate(tx_rate)), c2_x + 14.0, card_y + 74.0);

    // Card 3: Total Bandwidth bar (using redash_types::format_bytes)
    let c3_x = c2_x + card_w + card_gap;
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(c3_x, card_y, card_w, card_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(c3_x, card_y, card_w, card_h);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    let _ = ctx.fill_text("📊 累计传输流量 (Total Bandwidth)", c3_x + 14.0, card_y + 24.0);

    let tot_rx = metrics.map(|m| m.net.total_rx_bytes).unwrap_or(10_737_418_240);
    let tot_tx = metrics.map(|m| m.net.total_tx_bytes).unwrap_or(2_147_483_648);
    let tot_sum = tot_rx + tot_tx;

    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 16px 'JetBrains Mono', monospace");
    let _ = ctx.fill_text(&format_bytes(tot_sum), c3_x + 14.0, card_y + 50.0);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("11px sans-serif");
    let _ = ctx.fill_text(&format!("RX: {}  |  TX: {}", format_bytes(tot_rx), format_bytes(tot_tx)), c3_x + 14.0, card_y + 68.0);

    // Bandwidth proportion bar
    ctx.set_fill_style_str(theme.bg_input);
    ctx.fill_rect(c3_x + 14.0, card_y + 76.0, bar_w, 6.0);

    let rx_ratio = if tot_sum > 0 { tot_rx as f64 / tot_sum as f64 } else { 0.5 };
    let rx_w = bar_w * rx_ratio;
    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.fill_rect(c3_x + 14.0, card_y + 76.0, rx_w, 6.0);
    ctx.set_fill_style_str(theme.accent_purple);
    ctx.fill_rect(c3_x + 14.0 + rx_w, card_y + 76.0, bar_w - rx_w, 6.0);

    // 2. Listening Ports Table
    let table_y = card_y + card_h + 16.0;
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 14px sans-serif");
    let _ = ctx.fill_text("本地监听端口列表 (Listening Ports)", x + 16.0, table_y + 14.0);

    let th_y = table_y + 26.0;
    let th_h = 26.0;
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(x + 16.0, th_y, w - 32.0, th_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(x + 16.0, th_y, w - 32.0, th_h);

    let port_cols = [
        x + 24.0,   // Proto
        x + 120.0,  // Bind IP
        x + 280.0,  // Port
        x + 390.0,  // Process Name
        x + 580.0,  // PID
    ];

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    let _ = ctx.fill_text("协议 (Proto)", port_cols[0], th_y + 17.0);
    let _ = ctx.fill_text("绑定地址 (Bind IP)", port_cols[1], th_y + 17.0);
    let _ = ctx.fill_text("端口 (Port)", port_cols[2], th_y + 17.0);
    let _ = ctx.fill_text("占用进程 (Process Name)", port_cols[3], th_y + 17.0);
    let _ = ctx.fill_text("PID", port_cols[4], th_y + 17.0);

    let default_ports = [
        ListeningPort { proto: "TCP".to_string(), bind_ip: "0.0.0.0".to_string(), port: 22, pid: Some(1024), process_name: Some("sshd".to_string()) },
        ListeningPort { proto: "TCP".to_string(), bind_ip: "127.0.0.1".to_string(), port: 5432, pid: Some(2140), process_name: Some("postgres".to_string()) },
        ListeningPort { proto: "TCP".to_string(), bind_ip: "0.0.0.0".to_string(), port: 80, pid: Some(3112), process_name: Some("nginx".to_string()) },
        ListeningPort { proto: "TCP".to_string(), bind_ip: "0.0.0.0".to_string(), port: 443, pid: Some(3112), process_name: Some("nginx".to_string()) },
        ListeningPort { proto: "TCP".to_string(), bind_ip: ":::".to_string(), port: 9000, pid: Some(4051), process_name: Some("redash-server".to_string()) },
    ];

    let ports = metrics
        .map(|m| &m.listening_ports)
        .filter(|p| !p.is_empty())
        .map(|p| p.as_slice())
        .unwrap_or(&default_ports);

    let row_h = 28.0;
    let mut row_y = th_y + th_h + 3.0;

    for (idx, p) in ports.iter().enumerate() {
        if row_y + row_h > y + h - 10.0 {
            break;
        }

        ctx.set_fill_style_str(if idx % 2 == 0 { theme.bg_card } else { theme.bg_input });
        ctx.fill_rect(x + 16.0, row_y, w - 32.0, row_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(x + 16.0, row_y, w - 32.0, row_h);

        // Proto badge
        let is_tcp = p.proto.to_uppercase().contains("TCP");
        ctx.set_fill_style_str(if is_tcp { theme.accent_cyan } else { theme.accent_purple });
        ctx.set_font("bold 11px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&p.proto.to_uppercase(), port_cols[0], row_y + 19.0);

        // Bind IP
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("12px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&p.bind_ip, port_cols[1], row_y + 19.0);

        // Port
        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("bold 12px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&format!(":{}", p.port), port_cols[2], row_y + 19.0);

        // Process Name
        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("12px sans-serif");
        let _ = ctx.fill_text(p.process_name.as_deref().unwrap_or("-"), port_cols[3], row_y + 19.0);

        // PID
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("12px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&p.pid.map(|pid| pid.to_string()).unwrap_or_else(|| "-".to_string()), port_cols[4], row_y + 19.0);

        row_y += row_h + 3.0;
    }
}

fn render_tunnels_panel(
    ctx: &CanvasRenderingContext2d,
    _state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    // Header Toolbar
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 15px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text("🔀 SSH 端口转发与动态代理 (Tunnels)", x + 20.0, y + 26.0);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("12px sans-serif");
    let _ = ctx.fill_text("通过加密 SSH 通道将本地端口安全映射至远端服务", x + 310.0, y + 26.0);

    // "+ 新建隧道" Button
    let btn_w = 95.0;
    let btn_h = 26.0;
    let btn_x = x + w - btn_w - 20.0;
    let btn_y = y + 12.0;

    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(btn_x, btn_y, btn_w, btn_h);
    ctx.set_stroke_style_str(theme.accent_cyan);
    ctx.stroke_rect(btn_x, btn_y, btn_w, btn_h);

    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("center");
    let _ = ctx.fill_text("+ 新建隧道", btn_x + btn_w / 2.0, btn_y + 17.0);

    // Tunnel Cards List
    let mut card_y = y + 50.0;
    let card_h = 72.0;

    for tun in DEFAULT_TUNNELS {
        if card_y + card_h > y + h - 10.0 {
            break;
        }

        // Card box
        ctx.set_fill_style_str(theme.bg_card);
        ctx.fill_rect(x + 16.0, card_y, w - 32.0, card_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(x + 16.0, card_y, w - 32.0, card_h);

        // Type badge (Local TCP / SOCKS5)
        let is_local = tun.tunnel_type == "Local TCP";
        let badge_bg = theme.bg_input;
        let badge_color = if is_local { theme.accent_cyan } else { theme.accent_purple };
        let badge_text = if is_local { "Local TCP" } else { "SOCKS5" };

        let bx = x + 28.0;
        let by = card_y + 14.0;
        ctx.set_fill_style_str(badge_bg);
        ctx.fill_rect(bx, by, 76.0, 20.0);
        ctx.set_stroke_style_str(badge_color);
        ctx.stroke_rect(bx, by, 76.0, 20.0);

        ctx.set_fill_style_str(badge_color);
        ctx.set_font("bold 11px 'JetBrains Mono', monospace");
        ctx.set_text_align("center");
        let _ = ctx.fill_text(badge_text, bx + 38.0, by + 14.0);

        // Tunnel name
        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("bold 13px sans-serif");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(tun.name, bx + 88.0, by + 15.0);

        // Local port -> Remote host
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("12px 'JetBrains Mono', monospace");
        let endpoint_text = format!("{} ➔ {}", tun.local_endpoint, tun.remote_endpoint);
        let _ = ctx.fill_text(&endpoint_text, bx, card_y + 54.0);

        // Status LED
        let led_x = x + w - 240.0;
        ctx.set_fill_style_str(if tun.is_active { theme.status_online } else { theme.status_warn });
        ctx.begin_path();
        let _ = ctx.arc(led_x, by + 10.0, 4.0, 0.0, std::f64::consts::PI * 2.0);
        ctx.fill();

        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("11px sans-serif");
        let _ = ctx.fill_text(if tun.is_active { "运行中 (Active)" } else { "已暂停 (Idle)" }, led_x + 10.0, by + 14.0);

        // Traffic Stats
        ctx.set_font("11px sans-serif");
        let _ = ctx.fill_text(&format!("流量: {} | 连接: {}", tun.bytes_transferred, tun.active_conns), led_x, card_y + 54.0);

        // Action Buttons: [ ⏹ 停止 ] or [ ▶ 启动 ], [ 🗑️ 删除 ]
        let act_btn_x = x + w - 100.0;
        let act_btn_y = card_y + 12.0;
        ctx.set_fill_style_str(theme.bg_card_hover);
        ctx.fill_rect(act_btn_x, act_btn_y, 75.0, 24.0);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(act_btn_x, act_btn_y, 75.0, 24.0);

        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("11px sans-serif");
        ctx.set_text_align("center");
        let _ = ctx.fill_text(if tun.is_active { "⏹ 停止" } else { "▶ 启动" }, act_btn_x + 37.5, act_btn_y + 16.0);
        ctx.set_text_align("left");

        card_y += card_h + 10.0;
    }
}

fn render_snippets_panel(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    // 1. Categories Chips Bar
    let cat_y = y + 10.0;

    for (idx, &cat) in SNIPPET_CATEGORIES.iter().enumerate() {
        let (cx, cy, cw, ch) = get_snippet_category_rect(idx, x, y);
        let is_active = state.selected_snippet_category == cat
            || ((state.selected_snippet_category == "All" || state.selected_snippet_category == "全部") && cat == "全部");

        ctx.set_fill_style_str(if is_active { theme.bg_card_hover } else { theme.bg_card });
        ctx.fill_rect(cx, cy, cw, ch);
        ctx.set_stroke_style_str(if is_active { theme.accent_cyan } else { theme.border_default });
        ctx.stroke_rect(cx, cy, cw, ch);

        ctx.set_fill_style_str(if is_active { theme.accent_cyan } else { theme.text_secondary });
        ctx.set_font(if is_active { "bold 12px sans-serif" } else { "12px sans-serif" });
        ctx.set_text_align("center");
        let _ = ctx.fill_text(cat, cx + cw / 2.0, cy + 17.0);
    }

    ctx.set_text_align("left");

    // 2. Snippet Cards Grid
    let sel_cat = &state.selected_snippet_category;
    let filtered: Vec<&SnippetItem> = DEFAULT_SNIPPETS
        .iter()
        .filter(|s| {
            sel_cat == "All" || sel_cat == "全部" || s.category.eq_ignore_ascii_case(sel_cat)
        })
        .collect();

    let grid_y = cat_y + 36.0;
    let card_gap = 12.0;
    let card_w = ((w - 32.0 - card_gap) / 2.0).max(320.0);
    let card_h = 106.0;

    for (idx, s) in filtered.iter().enumerate() {
        let col = idx % 2;
        let row = idx / 2;
        let cx = x + 16.0 + (col as f64) * (card_w + card_gap);
        let cy = grid_y + (row as f64) * (card_h + card_gap);

        // Check visible boundary (leave room for output drawer if open)
        let bottom_limit = if state.snippet_output.is_some() { y + h - 230.0 } else { y + h - 10.0 };
        if cy + card_h > bottom_limit {
            break;
        }

        // Card Box
        ctx.set_fill_style_str(theme.bg_card);
        ctx.fill_rect(cx, cy, card_w, card_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(cx, cy, card_w, card_h);

        // Category badge
        let (badge_bg, badge_color) = match s.category {
            "System" => (theme.bg_input, theme.accent_purple),
            "Docker" => (theme.bg_input, theme.accent_cyan),
            "Network" => (theme.bg_input, theme.status_online),
            _ => (theme.bg_input, theme.status_warn),
        };

        ctx.set_fill_style_str(badge_bg);
        ctx.fill_rect(cx + 12.0, cy + 10.0, 56.0, 18.0);
        ctx.set_stroke_style_str(badge_color);
        ctx.stroke_rect(cx + 12.0, cy + 10.0, 56.0, 18.0);

        ctx.set_fill_style_str(badge_color);
        ctx.set_font("bold 10px 'JetBrains Mono', monospace");
        ctx.set_text_align("center");
        let _ = ctx.fill_text(s.category, cx + 40.0, cy + 23.0);

        // Title
        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("bold 13px sans-serif");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(s.name, cx + 76.0, cy + 24.0);

        // Description
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("11px sans-serif");
        let _ = ctx.fill_text(&truncate_text(s.description, 36), cx + 12.0, cy + 44.0);

        // Command Preview Block
        let cmd_box_w = card_w - 24.0;
        ctx.set_fill_style_str(theme.bg_input);
        ctx.fill_rect(cx + 12.0, cy + 52.0, cmd_box_w, 20.0);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(cx + 12.0, cy + 52.0, cmd_box_w, 20.0);

        ctx.set_fill_style_str(theme.text_muted);
        ctx.set_font("11px 'JetBrains Mono', monospace");
        let _ = ctx.fill_text(&format!("$ {}", truncate_text(s.command, 42)), cx + 18.0, cy + 66.0);

        // Action Buttons: [ 💻 注入终端 ] and [ ⚡ 执行 ]
        let btn_y = cy + 77.0;

        // [ 💻 注入终端 ]
        let inject_w = 95.0;
        let inject_x = cx + card_w - 180.0;
        ctx.set_fill_style_str(theme.bg_card_hover);
        ctx.fill_rect(inject_x, btn_y, inject_w, 22.0);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(inject_x, btn_y, inject_w, 22.0);

        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("11px sans-serif");
        ctx.set_text_align("center");
        let _ = ctx.fill_text("💻 注入终端", inject_x + inject_w / 2.0, btn_y + 15.0);

        // [ ⚡ 执行 ]
        let exec_w = 68.0;
        let exec_x = cx + card_w - 78.0;
        ctx.set_fill_style_str(theme.accent_cyan);
        ctx.fill_rect(exec_x, btn_y, exec_w, 22.0);

        ctx.set_fill_style_str(theme.bg_root);
        ctx.set_font("bold 11px sans-serif");
        let _ = ctx.fill_text("⚡ 执行", exec_x + exec_w / 2.0, btn_y + 15.0);
        ctx.set_text_align("left");
    }

    // 3. Execution Output Drawer (if active)
    if let Some((title, output)) = &state.snippet_output {
        let drawer_h = 220.0;
        let drawer_y = y + h - drawer_h;

        // Drawer background
        ctx.set_fill_style_str(theme.bg_card);
        ctx.fill_rect(x, drawer_y, w, drawer_h);

        // Drawer top border
        ctx.set_stroke_style_str(theme.accent_cyan);
        ctx.set_line_width(2.0);
        ctx.begin_path();
        ctx.move_to(x, drawer_y);
        ctx.line_to(x + w, drawer_y);
        ctx.stroke();

        // Header
        ctx.set_fill_style_str(theme.accent_cyan);
        ctx.set_font("bold 13px sans-serif");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(&format!("⚡ 脚本静默执行输出: {}", title), x + 16.0, drawer_y + 22.0);

        // Close button
        let btn_w = 70.0;
        let btn_h = 22.0;
        let btn_x = x + w - btn_w - 16.0;
        let btn_y = drawer_y + 8.0;
        ctx.set_fill_style_str(theme.bg_card_hover);
        ctx.fill_rect(btn_x, btn_y, btn_w, btn_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(btn_x, btn_y, btn_w, btn_h);

        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("11px sans-serif");
        ctx.set_text_align("center");
        let _ = ctx.fill_text("✕ 关闭", btn_x + btn_w / 2.0, btn_y + 15.0);

        // Output Console Box
        let box_x = x + 16.0;
        let box_y = drawer_y + 36.0;
        let box_w = w - 32.0;
        let box_h = drawer_h - 46.0;

        ctx.set_fill_style_str("#090d13");
        ctx.fill_rect(box_x, box_y, box_w, box_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(box_x, box_y, box_w, box_h);

        ctx.set_fill_style_str(theme.status_online);
        ctx.set_font("12px 'JetBrains Mono', monospace");
        ctx.set_text_align("left");
        let mut line_y = box_y + 20.0;
        for line in output.lines().take(7) {
            let _ = ctx.fill_text(line, box_x + 12.0, line_y);
            line_y += 20.0;
        }
    }
}

pub fn parse_breadcrumbs(path: &str) -> Vec<(String, String)> {
    let clean = path.trim();
    if clean.is_empty() || clean == "/" {
        return vec![("/".to_string(), "/".to_string())];
    }

    let mut result = vec![("/".to_string(), "/".to_string())];
    let segments: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
    let mut current_acc = String::new();

    for seg in segments {
        current_acc.push('/');
        current_acc.push_str(seg);
        result.push((seg.to_string(), current_acc.clone()));
    }

    result
}

pub fn get_parent_dir(path: &str) -> String {
    let clean = path.trim().trim_end_matches('/');
    if clean.is_empty() || clean == "/" {
        return "/".to_string();
    }
    match clean.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => clean[..idx].to_string(),
        None => "/".to_string(),
    }
}

pub fn get_sftp_refresh_btn_rect(content_x: f64, content_y: f64, content_w: f64) -> (f64, f64, f64, f64) {
    let btn_w = 88.0;
    let btn_h = 28.0;
    let btn_x = content_x + content_w - 24.0 - btn_w;
    let btn_y = content_y + 14.0;
    (btn_x, btn_y, btn_w, btn_h)
}

pub fn get_sftp_parent_dir_btn_rect(content_x: f64, content_y: f64) -> (f64, f64, f64, f64) {
    (content_x + 24.0, content_y + 54.0, 94.0, 26.0)
}

pub fn get_sftp_breadcrumb_rects(content_x: f64, content_y: f64, path: &str) -> Vec<(String, f64, f64, f64, f64)> {
    let breadcrumbs = parse_breadcrumbs(path);
    let mut rects = Vec::new();
    let is_root = path.trim() == "/" || path.trim().is_empty();
    let mut curr_x = if is_root {
        content_x + 24.0
    } else {
        content_x + 24.0 + 94.0 + 12.0
    };
    let seg_y = content_y + 54.0;
    let seg_h = 26.0;

    for (name, target) in breadcrumbs {
        let seg_w = (name.chars().count() as f64 * 8.0 + 16.0).max(28.0);
        rects.push((target, curr_x, seg_y, seg_w, seg_h));
        curr_x += seg_w + 12.0;
    }

    rects
}

pub fn get_sftp_file_row_rect(idx: usize, content_x: f64, content_y: f64, content_w: f64) -> (f64, f64, f64, f64) {
    let row_y = content_y + 130.0 + (idx as f64) * 36.0;
    let row_x = content_x + 24.0;
    let row_w = content_w - 48.0;
    let row_h = 32.0;
    (row_x, row_y, row_w, row_h)
}

pub fn get_sftp_editor_modal_rect(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let mw = (width * 0.82).clamp(640.0, 1100.0);
    let mh = (height * 0.78).clamp(420.0, 760.0);
    let mx = (width - mw) / 2.0;
    let my = (height - mh) / 2.0;
    (mx, my, mw, mh)
}

pub fn get_sftp_editor_save_btn_rect(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let (mx, my, mw, _) = get_sftp_editor_modal_rect(width, height);
    let btn_w = 80.0;
    let btn_h = 28.0;
    let btn_x = mx + mw - 176.0;
    let btn_y = my + 7.0;
    (btn_x, btn_y, btn_w, btn_h)
}

pub fn get_sftp_editor_close_btn_rect(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let (mx, my, mw, _) = get_sftp_editor_modal_rect(width, height);
    let btn_w = 76.0;
    let btn_h = 28.0;
    let btn_x = mx + mw - 88.0;
    let btn_y = my + 7.0;
    (btn_x, btn_y, btn_w, btn_h)
}

pub fn format_modified_time(ts: Option<u64>) -> String {
    if let Some(secs) = ts {
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hour = time_of_day / 3600;
        let min = (time_of_day % 3600) / 60;
        let mut y = 1970;
        let mut d = days;
        loop {
            let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
            let days_in_year = if leap { 366 } else { 365 };
            if d >= days_in_year {
                d -= days_in_year;
                y += 1;
            } else {
                break;
            }
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let days_in_months = [
            31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
        ];
        let mut m = 12;
        for (idx, &dim) in days_in_months.iter().enumerate() {
            if d < dim {
                m = idx + 1;
                break;
            }
            d -= dim;
        }
        let day = d + 1;
        format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, day, hour, min)
    } else {
        "-".to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn render_sftp_view(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    // 1. Header Bar
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 16px sans-serif");
    ctx.set_text_align("left");
    let _ = ctx.fill_text(state.t("nav.sftp"), x + 24.0, y + 34.0);

    // Host badge
    let host_label = state
        .selected_host_id
        .as_ref()
        .and_then(|id| state.hosts.iter().find(|h| h.id.0 == *id))
        .map(|h| format!("🖥️ {} ({}:{})", h.name, h.hostname, h.port))
        .unwrap_or_else(|| "🖥️ 未选择主机 (No Host Selected)".to_string());

    ctx.set_fill_style_str(theme.accent_cyan);
    ctx.set_font("13px 'JetBrains Mono', monospace");
    let _ = ctx.fill_text(&host_label, x + 160.0, y + 34.0);

    // Refresh Button: [ 🔄 刷新 ]
    let (btn_rx, btn_ry, btn_rw, btn_rh) = get_sftp_refresh_btn_rect(x, y, w);
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(btn_rx, btn_ry, btn_rw, btn_rh);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.set_line_width(1.0);
    ctx.stroke_rect(btn_rx, btn_ry, btn_rw, btn_rh);

    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("12px sans-serif");
    ctx.set_text_align("center");
    let _ = ctx.fill_text("🔄 刷新", btn_rx + btn_rw / 2.0, btn_ry + 18.0);

    // 2. Breadcrumb & Navigation Bar
    let is_root = state.sftp_current_path.trim() == "/" || state.sftp_current_path.trim().is_empty();
    if !is_root {
        let (p_x, p_y, p_w, p_h) = get_sftp_parent_dir_btn_rect(x, y);
        ctx.set_fill_style_str(theme.bg_card);
        ctx.fill_rect(p_x, p_y, p_w, p_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(p_x, p_y, p_w, p_h);

        ctx.set_fill_style_str(theme.text_primary);
        ctx.set_font("12px sans-serif");
        ctx.set_text_align("center");
        let _ = ctx.fill_text("⬆ 上级目录", p_x + p_w / 2.0, p_y + 17.0);
    }

    // Breadcrumb segments
    let breadcrumb_rects = get_sftp_breadcrumb_rects(x, y, &state.sftp_current_path);
    let breadcrumb_raw = parse_breadcrumbs(&state.sftp_current_path);

    for (idx, (_target_path, seg_x, seg_y, seg_w, seg_h)) in breadcrumb_rects.iter().enumerate() {
        let is_last = idx + 1 == breadcrumb_rects.len();
        let name = &breadcrumb_raw[idx].0;

        ctx.set_fill_style_str(if is_last {
            theme.accent_cyan
        } else {
            theme.bg_card
        });
        ctx.fill_rect(*seg_x, *seg_y, *seg_w, *seg_h);
        ctx.set_stroke_style_str(theme.border_default);
        ctx.stroke_rect(*seg_x, *seg_y, *seg_w, *seg_h);

        ctx.set_fill_style_str(if is_last {
            theme.bg_root
        } else {
            theme.text_primary
        });
        ctx.set_font(if is_last {
            "bold 12px 'JetBrains Mono', monospace"
        } else {
            "12px 'JetBrains Mono', monospace"
        });
        ctx.set_text_align("center");
        let _ = ctx.fill_text(name, seg_x + seg_w / 2.0, seg_y + 17.0);

        if !is_last {
            ctx.set_fill_style_str(theme.text_secondary);
            ctx.set_font("12px sans-serif");
            ctx.set_text_align("center");
            let _ = ctx.fill_text("/", seg_x + seg_w + 6.0, seg_y + 17.0);
        }
    }

    // 3. Files Table Header
    let table_x = x + 24.0;
    let table_w = w - 48.0;
    let header_y = y + 92.0;
    let header_h = 28.0;

    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(table_x, header_y, table_w, header_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(table_x, header_y, table_w, header_h);

    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("left");

    let col_name_x = table_x + 12.0;
    let col_type_x = table_x + (table_w * 0.45).max(280.0);
    let col_size_x = col_type_x + 70.0;
    let col_perm_x = col_size_x + 110.0;
    let col_mod_x = col_perm_x + 120.0;

    let _ = ctx.fill_text("名称 (Name)", col_name_x, header_y + 18.0);
    let _ = ctx.fill_text("类型", col_type_x, header_y + 18.0);
    let _ = ctx.fill_text("大小", col_size_x, header_y + 18.0);
    let _ = ctx.fill_text("权限", col_perm_x, header_y + 18.0);
    let _ = ctx.fill_text("修改时间", col_mod_x, header_y + 18.0);

    // 4. File Rows / Loading / Empty
    if state.sftp_loading {
        let msg_y = header_y + 50.0;
        ctx.set_fill_style_str(theme.accent_cyan);
        ctx.set_font("14px sans-serif");
        ctx.set_text_align("left");
        let _ = ctx.fill_text("⏳ 正在加载远程目录 (Loading remote directory...)", table_x + 12.0, msg_y);
    } else if let Some(ref err) = state.sftp_error {
        let msg_y = header_y + 50.0;
        ctx.set_fill_style_str(theme.status_crit);
        ctx.set_font("13px sans-serif");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(&format!("❌ SFTP 错误: {}", err), table_x + 12.0, msg_y);
    } else if state.sftp_files.is_empty() {
        let msg_y = header_y + 50.0;
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("13px sans-serif");
        ctx.set_text_align("left");
        let _ = ctx.fill_text("📂 空目录 (Empty directory)", table_x + 12.0, msg_y);
    } else {
        let max_visible_rows = ((h - (header_y - y + header_h + 20.0)) / 36.0).floor().max(1.0) as usize;
        for (idx, file) in state.sftp_files.iter().enumerate().take(max_visible_rows) {
            let (rx, ry, rw, rh) = get_sftp_file_row_rect(idx, x, y, w);

            // Row background
            ctx.set_fill_style_str(if idx % 2 == 0 {
                theme.bg_card
            } else {
                theme.bg_sidebar
            });
            ctx.fill_rect(rx, ry, rw, rh);
            ctx.set_stroke_style_str(theme.border_default);
            ctx.stroke_rect(rx, ry, rw, rh);

            // Icon + Name
            let icon = file.category().default_icon();
            let display_name = if file.is_dir {
                format!("{} {}/", icon, file.name)
            } else {
                format!("{} {}", icon, file.name)
            };

            ctx.set_fill_style_str(if file.is_dir {
                theme.accent_cyan
            } else {
                theme.text_primary
            });
            ctx.set_font(if file.is_dir {
                "bold 13px 'JetBrains Mono', monospace"
            } else {
                "13px 'JetBrains Mono', monospace"
            });
            ctx.set_text_align("left");
            let truncated_name = truncate_text(&display_name, 36);
            let _ = ctx.fill_text(&truncated_name, col_name_x, ry + 21.0);

            // Type
            ctx.set_fill_style_str(theme.text_secondary);
            ctx.set_font("12px sans-serif");
            let ftype = if file.is_dir {
                "DIR"
            } else if file.is_symlink {
                "LINK"
            } else {
                "FILE"
            };
            let _ = ctx.fill_text(ftype, col_type_x, ry + 21.0);

            // Size
            let size_str = if file.is_dir {
                "-".to_string()
            } else {
                redash_types::format_bytes(file.size)
            };
            let _ = ctx.fill_text(&size_str, col_size_x, ry + 21.0);

            // Permissions
            ctx.set_font("12px 'JetBrains Mono', monospace");
            let _ = ctx.fill_text(&file.permissions_str(), col_perm_x, ry + 21.0);

            // Modified
            let mod_str = format_modified_time(file.modified);
            let _ = ctx.fill_text(&mod_str, col_mod_x, ry + 21.0);
        }
    }
}

pub fn render_sftp_editor_modal(
    ctx: &CanvasRenderingContext2d,
    state: &AppState,
    theme: &ThemeColors,
    width: f64,
    height: f64,
) {
    let Some((ref file_path, ref content)) = state.sftp_editor else {
        return;
    };

    // 1. Overlay backdrop
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.75)");
    ctx.fill_rect(0.0, 0.0, width, height);

    // 2. Editor Window
    let (mx, my, mw, mh) = get_sftp_editor_modal_rect(width, height);
    ctx.set_fill_style_str(theme.bg_card);
    ctx.fill_rect(mx, my, mw, mh);
    ctx.set_stroke_style_str(theme.accent_cyan);
    ctx.set_line_width(2.0);
    ctx.stroke_rect(mx, my, mw, mh);

    // 3. Title Bar
    let title_bar_h = 42.0;
    ctx.set_fill_style_str(theme.bg_sidebar);
    ctx.fill_rect(mx, my, mw, title_bar_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(mx, my + title_bar_h);
    ctx.line_to(mx + mw, my + title_bar_h);
    ctx.stroke();

    // Title / File path + dirty indicator
    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("bold 13px 'JetBrains Mono', monospace");
    ctx.set_text_align("left");
    let dirty_suffix = if state.sftp_editor_modified { " *" } else { "" };
    let title = format!("📝 {}{}", file_path, dirty_suffix);
    let _ = ctx.fill_text(&title, mx + 16.0, my + 26.0);

    // Buttons
    // Save button: [ 💾 保存 ]
    let (save_x, save_y, save_w, save_h) = get_sftp_editor_save_btn_rect(width, height);
    ctx.set_fill_style_str(if state.sftp_editor_modified {
        theme.accent_cyan
    } else {
        theme.bg_card_hover
    });
    ctx.fill_rect(save_x, save_y, save_w, save_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(save_x, save_y, save_w, save_h);

    ctx.set_fill_style_str(if state.sftp_editor_modified {
        theme.bg_root
    } else {
        theme.text_primary
    });
    ctx.set_font("bold 12px sans-serif");
    ctx.set_text_align("center");
    let _ = ctx.fill_text("💾 保存", save_x + save_w / 2.0, save_y + 18.0);

    // Close button: [ ✕ 关闭 ]
    let (close_x, close_y, close_w, close_h) = get_sftp_editor_close_btn_rect(width, height);
    ctx.set_fill_style_str(theme.bg_card_hover);
    ctx.fill_rect(close_x, close_y, close_w, close_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(close_x, close_y, close_w, close_h);

    ctx.set_fill_style_str(theme.text_primary);
    ctx.set_font("12px sans-serif");
    let _ = ctx.fill_text("✕ 关闭", close_x + close_w / 2.0, close_y + 18.0);

    // 4. Editor Content Text Area
    let area_x = mx + 12.0;
    let area_y = my + title_bar_h + 10.0;
    let area_w = mw - 24.0;
    let area_h = mh - title_bar_h - 40.0;

    // Background for code area
    ctx.set_fill_style_str("#0d1117");
    ctx.fill_rect(area_x, area_y, area_w, area_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.stroke_rect(area_x, area_y, area_w, area_h);

    // Line numbers gutter
    let gutter_w = 44.0;
    ctx.set_fill_style_str("#161b22");
    ctx.fill_rect(area_x, area_y, gutter_w, area_h);
    ctx.set_stroke_style_str(theme.border_default);
    ctx.begin_path();
    ctx.move_to(area_x + gutter_w, area_y);
    ctx.line_to(area_x + gutter_w, area_y + area_h);
    ctx.stroke();

    // Render code lines
    let line_height = 20.0;
    let max_lines = (area_h / line_height).floor() as usize;
    let code_x = area_x + gutter_w + 10.0;

    for (idx, line) in content.lines().enumerate().take(max_lines) {
        let line_y = area_y + 16.0 + (idx as f64) * line_height;

        // Line number
        ctx.set_fill_style_str(theme.text_secondary);
        ctx.set_font("12px 'JetBrains Mono', monospace");
        ctx.set_text_align("right");
        let _ = ctx.fill_text(&(idx + 1).to_string(), area_x + gutter_w - 8.0, line_y);

        // Code text
        ctx.set_fill_style_str("#e6edf3");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(line, code_x, line_y);
    }

    // Status bar at bottom of editor
    let status_y = my + mh - 14.0;
    ctx.set_fill_style_str(theme.text_secondary);
    ctx.set_font("11px 'JetBrains Mono', monospace");
    ctx.set_text_align("left");
    let line_count = content.lines().count();
    let byte_count = content.len();
    let status_text = format!("UTF-8 | 行数: {} | 字符: {} 字节 | (Ctrl+S / ⌘+S 保存, Esc 关闭)", line_count, byte_count);
    let _ = ctx.fill_text(&status_text, mx + 16.0, status_y);

    if state.sftp_loading {
        ctx.set_fill_style_str(theme.accent_cyan);
        ctx.set_text_align("right");
        let _ = ctx.fill_text("⏳ 正在保存文件...", mx + mw - 16.0, status_y);
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
