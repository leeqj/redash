use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

#[derive(Clone)]
pub struct TerminalEventListener(
    std::sync::Arc<std::sync::Mutex<Vec<alacritty_terminal::event::Event>>>,
);
impl alacritty_terminal::event::EventListener for TerminalEventListener {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        use alacritty_terminal::event::Event;
        if matches!(
            event,
            Event::PtyWrite(_) | Event::ColorRequest(..) | Event::TextAreaSizeRequest(..)
        ) {
            self.0.lock().unwrap().push(event);
        }
    }
}

#[derive(Clone, Copy)]
pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

pub const DEFAULT_TERM_FG: u32 = 0xf1f5f9;
pub const DEFAULT_TERM_BG: u32 = 0x0a0b10;

/// Maps Alacritty/ANSI named colors to 24-bit RGB values.
pub fn named_color_to_rgb(named: alacritty_terminal::vte::ansi::NamedColor, _is_bg: bool) -> u32 {
    use alacritty_terminal::vte::ansi::NamedColor;
    match named {
        // Standard 8 ANSI colors
        NamedColor::Black => 0x181825,
        NamedColor::Red => 0xf43f5e,
        NamedColor::Green => 0x10b981,
        NamedColor::Yellow => 0xf59e0b,
        NamedColor::Blue => 0x3b82f6,
        NamedColor::Magenta => 0xa855f7,
        NamedColor::Cyan => 0x06b6d4,
        NamedColor::White => 0xe2e8f0,

        // High-intensity / Bright 8 ANSI colors
        NamedColor::BrightBlack => 0x64748b,
        NamedColor::BrightRed => 0xfb7185,
        NamedColor::BrightGreen => 0x34d399,
        NamedColor::BrightYellow => 0xfbbf24,
        NamedColor::BrightBlue => 0x60a5fa,
        NamedColor::BrightMagenta => 0xc084fc,
        NamedColor::BrightCyan => 0x38bdf8,
        NamedColor::BrightWhite => 0xffffff,

        // Dim 8 colors
        NamedColor::DimBlack => 0x11111b,
        NamedColor::DimRed => 0x9f1239,
        NamedColor::DimGreen => 0x065f46,
        NamedColor::DimYellow => 0x92400e,
        NamedColor::DimBlue => 0x1e40af,
        NamedColor::DimMagenta => 0x6b21a8,
        NamedColor::DimCyan => 0x155e75,
        NamedColor::DimWhite => 0x94a3b8,

        // Semantic / UI colors
        NamedColor::Foreground => DEFAULT_TERM_FG,
        NamedColor::Background => DEFAULT_TERM_BG,
        NamedColor::Cursor => 0x38bdf8,
        NamedColor::BrightForeground => 0xffffff,
        NamedColor::DimForeground => 0x94a3b8,
    }
}

/// Converts indexed 256-color palette to 24-bit RGB values.
pub fn indexed_color_to_rgb(idx: u8) -> u32 {
    match idx {
        0 => 0x181825,
        1 => 0xf43f5e,
        2 => 0x10b981,
        3 => 0xf59e0b,
        4 => 0x3b82f6,
        5 => 0xa855f7,
        6 => 0x06b6d4,
        7 => 0xe2e8f0,
        8 => 0x64748b,
        9 => 0xfb7185,
        10 => 0x34d399,
        11 => 0xfbbf24,
        12 => 0x60a5fa,
        13 => 0xc084fc,
        14 => 0x38bdf8,
        15 => 0xffffff,
        // 16..231: 6x6x6 color cube
        16..=231 => {
            let offset = idx - 16;
            let r_idx = (offset / 36) as usize;
            let g_idx = ((offset % 36) / 6) as usize;
            let b_idx = (offset % 6) as usize;
            const STEPS: [u32; 6] = [0, 95, 135, 175, 215, 255];
            (STEPS[r_idx] << 16) | (STEPS[g_idx] << 8) | STEPS[b_idx]
        }
        // 232..255: grayscale ramp (24 shades)
        232..=255 => {
            let gray = 8 + (idx - 232) as u32 * 10;
            (gray << 16) | (gray << 8) | gray
        }
    }
}

/// Converts arbitrary Alacritty ANSI Color (Named, Indexed, Spec) to 24-bit RGB.
pub fn ansi_color_to_rgb(color: alacritty_terminal::vte::ansi::Color, is_bg: bool) -> u32 {
    use alacritty_terminal::vte::ansi::Color;
    match color {
        Color::Spec(rgb) => ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32),
        Color::Indexed(idx) => indexed_color_to_rgb(idx),
        Color::Named(named) => named_color_to_rgb(named, is_bg),
    }
}

use crate::terminal::search::TerminalSearch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: (usize, usize), // (col, row)
    pub end: (usize, usize),   // (col, row)
}

impl SelectionRange {
    pub fn new(start_col: usize, start_row: usize, end_col: usize, end_row: usize) -> Self {
        Self {
            start: (start_col, start_row),
            end: (end_col, end_row),
        }
    }

    pub fn normalized(&self) -> ((usize, usize), (usize, usize)) {
        let (c1, r1) = self.start;
        let (c2, r2) = self.end;
        if (r1, c1) <= (r2, c2) {
            ((c1, r1), (c2, r2))
        } else {
            ((c2, r2), (c1, r1))
        }
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        let ((sc, sr), (ec, er)) = self.normalized();
        if row < sr || row > er {
            return false;
        }
        if sr == er {
            col >= sc && col <= ec
        } else if row == sr {
            col >= sc
        } else if row == er {
            col <= ec
        } else {
            true
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Represents a contiguous run of terminal characters sharing the exact same styling.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub fg: u32,
    pub bg: Option<u32>,
    pub bold: bool,
    pub underline: bool,
    pub is_cursor: bool,
    pub is_selected: bool,
    pub is_search_match: bool,
    pub is_active_search_match: bool,
    pub cols: usize,
}

/// A line partitioned into contiguous stylized TextRuns.
#[derive(Debug, Clone, Default)]
pub struct TerminalLineRuns {
    pub runs: Vec<TextRun>,
}

pub struct TerminalEmulator {
    term: Term<TerminalEventListener>,
    events: TerminalEventListener,
    processor: Processor,
    pub cols: usize,
    pub rows: usize,
}

impl TerminalEmulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        let size = TermSize { cols, rows };
        let events = TerminalEventListener(Default::default());
        let term = Term::new(Config::default(), &size, events.clone());
        let processor = Processor::new();

        Self {
            events,
            term,
            processor,
            cols,
            rows,
        }
    }

    pub fn set_scrollback(&mut self, lines: usize) {
        self.term.set_options(Config {
            scrolling_history: lines.clamp(0, 100_000),
            ..Default::default()
        });
    }
    pub fn application_cursor(&self) -> bool {
        self.term
            .mode()
            .contains(alacritty_terminal::term::TermMode::APP_CURSOR)
    }
    pub fn prepare_paste(&self, text: &str) -> Vec<u8> {
        if self
            .term
            .mode()
            .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
        {
            // Escape cannot be allowed to terminate bracketed paste early.
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
        }
    }
    pub fn drain_responses(&self) -> Vec<Vec<u8>> {
        use alacritty_terminal::event::{Event, WindowSize};
        use alacritty_terminal::vte::ansi::Rgb;
        self.events
            .0
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|event| match event {
                Event::PtyWrite(value) => Some(value.into_bytes()),
                Event::TextAreaSizeRequest(format) => Some(
                    format(WindowSize {
                        num_cols: self.cols as u16,
                        num_lines: self.rows as u16,
                        cell_width: 8,
                        cell_height: 18,
                    })
                    .into_bytes(),
                ),
                Event::ColorRequest(index, format) => {
                    let color = match index {
                        0..=255 => indexed_color_to_rgb(index as u8),
                        257 => DEFAULT_TERM_BG,
                        _ => DEFAULT_TERM_FG,
                    };
                    Some(
                        format(Rgb {
                            r: (color >> 16) as u8,
                            g: (color >> 8) as u8,
                            b: color as u8,
                        })
                        .into_bytes(),
                    )
                }
                _ => None,
            })
            .collect()
    }

    pub fn process_input(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    #[allow(dead_code)]
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        let size = TermSize { cols, rows };
        self.term.resize(size);
    }

    pub fn cursor_position(&self) -> Option<(usize, usize)> {
        if !self
            .term
            .mode()
            .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR)
        {
            return None;
        }
        let grid = self.term.grid();
        let display_offset = grid.display_offset();
        let point = grid.cursor.point;
        let line_offset = point.line.0 + (display_offset as i32);
        let screen_lines = self.rows.min(grid.screen_lines());
        let columns = self.cols.min(grid.columns());
        if line_offset >= 0 && (line_offset as usize) < screen_lines && point.column.0 < columns {
            Some((line_offset as usize, point.column.0))
        } else {
            None
        }
    }

    /// Returns lines batched into contiguous stylized `TextRun`s, supporting Selection and Search highlighting.
    pub fn renderable_runs(
        &self,
        cursor_opt: Option<(usize, usize)>,
        selection: Option<&SelectionRange>,
        search: Option<&TerminalSearch>,
    ) -> Vec<TerminalLineRuns> {
        let grid = self.term.grid();
        let screen_lines = self.rows.min(grid.screen_lines());
        let total_cols = self.cols.min(grid.columns());
        let display_offset = grid.display_offset();
        let history_size = grid.history_size();
        let mut lines = Vec::with_capacity(self.rows);

        for line_idx in 0..screen_lines {
            let line_offset = (line_idx as i32) - (display_offset as i32);
            if line_offset < -(history_size as i32) || line_offset >= (grid.screen_lines() as i32) {
                lines.push(TerminalLineRuns::default());
                continue;
            }

            let row = &grid[Line(line_offset)];
            let is_cursor_row = cursor_opt.is_some_and(|(r, _)| r == line_idx);
            let cursor_col = cursor_opt.map(|(_, c)| c);

            // Determine the rightmost used column (characters, custom backgrounds, cursor, selection, search)
            let mut last_used_col = 0;
            for col_idx in 0..total_cols {
                let cell = &row[Column(col_idx)];
                if (cell.c != ' ' && cell.c != '\0') || cell.flags.contains(Flags::INVERSE) {
                    last_used_col = col_idx + 1;
                } else {
                    let bg = ansi_color_to_rgb(cell.bg, true);
                    if bg != DEFAULT_TERM_BG {
                        last_used_col = col_idx + 1;
                    }
                }
            }
            if is_cursor_row && let Some(cc) = cursor_col {
                last_used_col = last_used_col.max(cc + 1);
            }

            if let Some(sel) = selection {
                let ((_sc, sr), (ec, er)) = sel.normalized();
                if line_idx >= sr && line_idx <= er {
                    let col = if line_idx == er { ec + 1 } else { total_cols };
                    last_used_col = last_used_col.max(col);
                }
            }

            if let Some(s) = search {
                for m in &s.matches {
                    if m.line == line_idx {
                        last_used_col = last_used_col.max(m.col_end);
                    }
                }
            }

            let mut runs: Vec<TextRun> = Vec::new();
            let mut col_idx = 0;
            let end_col = last_used_col.min(total_cols);

            while col_idx < end_col {
                let cell = &row[Column(col_idx)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    col_idx += 1;
                    continue;
                }

                let is_wide = cell.flags.contains(Flags::WIDE_CHAR)
                    || (cell.c != ' '
                        && cell.c != '\0'
                        && unicode_width::UnicodeWidthChar::width(cell.c) == Some(2));
                let cell_cols = if is_wide { 2 } else { 1 };

                let is_cursor = is_cursor_row
                    && (cursor_col == Some(col_idx)
                        || (is_wide && cursor_col == Some(col_idx + 1)));
                let is_selected = selection.is_some_and(|s| s.contains(line_idx, col_idx));
                let (is_search_match, is_active_search_match) =
                    search.map_or((false, false), |s| s.is_match_at(line_idx, col_idx));

                let mut fg = ansi_color_to_rgb(cell.fg, false);
                let mut bg = ansi_color_to_rgb(cell.bg, true);
                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }

                // Apply overlays in order of priority: selection > active search match > search match
                if is_selected {
                    bg = 0x1d4ed8; // Tech blue selection background
                    fg = 0xffffff;
                } else if is_active_search_match {
                    bg = 0xf59e0b; // Bright amber active search highlight
                    fg = 0x0a0b10;
                } else if is_search_match {
                    bg = 0x78350f; // Muted amber search match
                    fg = 0xfef3c7;
                }

                let bg_opt = if bg == DEFAULT_TERM_BG
                    && !is_selected
                    && !is_search_match
                    && !is_active_search_match
                {
                    None
                } else {
                    Some(bg)
                };
                let bold = cell.flags.contains(Flags::BOLD);
                let underline = cell.flags.contains(Flags::UNDERLINE);

                let ch = if cell.c == '\0' || cell.c == ' ' {
                    '\u{00A0}'
                } else {
                    cell.c
                };

                // Merge into current run if styling matches
                let can_merge = if let Some(last) = runs.last_mut() {
                    !last.is_cursor
                        && !is_cursor
                        && last.is_selected == is_selected
                        && last.is_search_match == is_search_match
                        && last.is_active_search_match == is_active_search_match
                        && last.fg == fg
                        && last.bg == bg_opt
                        && last.bold == bold
                        && last.underline == underline
                } else {
                    false
                };

                if can_merge {
                    let last = runs.last_mut().unwrap();
                    last.text.push(ch);
                    last.cols += cell_cols;
                } else {
                    runs.push(TextRun {
                        text: ch.to_string(),
                        fg,
                        bg: bg_opt,
                        bold,
                        underline,
                        is_cursor,
                        is_selected,
                        is_search_match,
                        is_active_search_match,
                        cols: cell_cols,
                    });
                }

                col_idx += 1;
            }

            lines.push(TerminalLineRuns { runs });
        }

        for _ in screen_lines..self.rows {
            lines.push(TerminalLineRuns::default());
        }

        lines
    }

    /// Extracts text in the given selection range as a single string.
    pub fn get_selected_text(&self, selection: &SelectionRange) -> String {
        let grid = self.term.grid();
        let screen_lines = self.rows.min(grid.screen_lines());
        let total_cols = self.cols.min(grid.columns());
        let display_offset = grid.display_offset();
        let history_size = grid.history_size();
        let ((sc, sr), (ec, er)) = selection.normalized();

        let mut result = String::new();
        for row_idx in sr..=er.min(screen_lines.saturating_sub(1)) {
            let line_offset = (row_idx as i32) - (display_offset as i32);
            if line_offset < -(history_size as i32) || line_offset >= (grid.screen_lines() as i32) {
                continue;
            }
            let row = &grid[Line(line_offset)];
            let col_start = if row_idx == sr { sc } else { 0 };
            let col_end = if row_idx == er {
                ec.min(total_cols.saturating_sub(1))
            } else {
                total_cols.saturating_sub(1)
            };

            let mut line_str = String::new();
            for col_idx in col_start..=col_end {
                let cell = &row[Column(col_idx)];
                if !cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    if cell.c != '\0' {
                        line_str.push(cell.c);
                    } else {
                        line_str.push(' ');
                    }
                }
            }
            let is_wrapped = if total_cols > 0 {
                row[Column(total_cols - 1)].flags.contains(Flags::WRAPLINE)
            } else {
                false
            };

            if is_wrapped && row_idx < er {
                result.push_str(&line_str);
            } else {
                let trimmed = line_str.trim_end();
                result.push_str(trimmed);
                if row_idx < er {
                    result.push('\n');
                }
            }
        }
        result
    }

    /// Returns all visible screen lines as plain string vectors (for in-memory text search).
    pub fn screen_lines_as_strings(&self) -> Vec<String> {
        let grid = self.term.grid();
        let screen_lines = self.rows.min(grid.screen_lines());
        let total_cols = self.cols.min(grid.columns());
        let display_offset = grid.display_offset();
        let history_size = grid.history_size();
        let mut lines = Vec::with_capacity(screen_lines);

        for line_idx in 0..screen_lines {
            let line_offset = (line_idx as i32) - (display_offset as i32);
            if line_offset < -(history_size as i32) || line_offset >= (grid.screen_lines() as i32) {
                lines.push(String::new());
                continue;
            }
            let row = &grid[Line(line_offset)];
            let mut line_str = String::new();
            for col_idx in 0..total_cols {
                let cell = &row[Column(col_idx)];
                if !cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    if cell.c != '\0' {
                        line_str.push(cell.c);
                    } else {
                        line_str.push(' ');
                    }
                }
            }
            lines.push(line_str.trim_end().to_string());
        }
        lines
    }

    pub fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_queries_and_modes() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.process_input(b"abc\x1b[6n");
        assert_eq!(emu.drain_responses(), vec![b"\x1b[1;4R".to_vec()]);
        emu.process_input(b"\x1b[?2004h\x1b[?1h");
        assert_eq!(emu.prepare_paste("a\nb"), b"\x1b[200~a\nb\x1b[201~");
        assert!(emu.application_cursor());
        emu.process_input(b"\x1b[?25l");
        assert_eq!(emu.cursor_position(), None);
        emu.set_scrollback(2);
        emu.process_input(b"\r\n".repeat(40).as_slice());
        assert_eq!(emu.term.history_size(), 2);
    }

    #[test]
    fn test_named_and_indexed_colors() {
        use alacritty_terminal::vte::ansi::NamedColor;
        assert_eq!(named_color_to_rgb(NamedColor::Green, false), 0x10b981);
        assert_eq!(named_color_to_rgb(NamedColor::BrightCyan, false), 0x38bdf8);
        assert_eq!(indexed_color_to_rgb(1), 0xf43f5e);
        assert_eq!(indexed_color_to_rgb(2), 0x10b981);
        // Grayscale ramp check
        assert_eq!(indexed_color_to_rgb(232), 0x080808);
    }

    #[test]
    fn test_text_run_batching() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.process_input(b"Hello \x1b[32mWorld\x1b[0m\r\n");

        let runs = emu.renderable_runs(None, None, None);
        assert_eq!(runs.len(), 24);
        // Line 0 should have "Hello " followed by green "World"
        let line0 = &runs[0];
        assert_eq!(line0.runs.len(), 2);
        assert_eq!(line0.runs[0].text, "Hello\u{00A0}");
        assert_eq!(line0.runs[0].fg, DEFAULT_TERM_FG);
        assert_eq!(line0.runs[1].text, "World");
        assert_eq!(line0.runs[1].fg, 0x10b981); // Green
    }

    #[test]
    fn test_cjk_double_width() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.process_input("你好世界".as_bytes());

        let runs = emu.renderable_runs(None, None, None);
        let line0 = &runs[0];
        assert!(!line0.runs.is_empty());
        let total_cols: usize = line0.runs.iter().map(|r| r.cols).sum();
        assert_eq!(total_cols, 8); // 4 Chinese chars * 2 cols = 8
    }

    #[test]
    fn test_cursor_separation() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.process_input(b"abc");

        let cursor_pos = emu.cursor_position();
        let runs = emu.renderable_runs(cursor_pos, None, None);
        let line0 = &runs[0];
        // 'abc' then cursor at col 3
        let cursor_run = line0.runs.iter().find(|r| r.is_cursor);
        assert!(cursor_run.is_some());
    }

    #[test]
    fn test_selection_highlighting() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.process_input(b"Hello Rust World");

        let sel = SelectionRange::new(6, 0, 9, 0);
        let runs = emu.renderable_runs(None, Some(&sel), None);
        let line0 = &runs[0];
        let selected_run = line0.runs.iter().find(|r| r.is_selected);
        assert!(selected_run.is_some());
        let r = selected_run.unwrap();
        assert_eq!(r.text, "Rust");
        assert_eq!(r.bg, Some(0x1d4ed8)); // Selection blue
    }
}
