use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnsiNamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnsiColor {
    #[default]
    Default,
    Named(AnsiNamedColor),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl AnsiColor {
    pub fn to_css_color(&self, is_foreground: bool) -> &'static str {
        match self {
            AnsiColor::Default => {
                if is_foreground {
                    "#c9d1d9"
                } else {
                    "transparent"
                }
            }
            AnsiColor::Named(named) => match named {
                AnsiNamedColor::Black => "#000000",
                AnsiNamedColor::Red => "#f85149",
                AnsiNamedColor::Green => "#3fb950",
                AnsiNamedColor::Yellow => "#d29922",
                AnsiNamedColor::Blue => "#58a6ff",
                AnsiNamedColor::Magenta => "#bc8cff",
                AnsiNamedColor::Cyan => "#38bdf8",
                AnsiNamedColor::White => "#ffffff",
                AnsiNamedColor::BrightBlack => "#484f58",
                AnsiNamedColor::BrightRed => "#ff7b72",
                AnsiNamedColor::BrightGreen => "#56d364",
                AnsiNamedColor::BrightYellow => "#e3b341",
                AnsiNamedColor::BrightBlue => "#79c0ff",
                AnsiNamedColor::BrightMagenta => "#d2a8ff",
                AnsiNamedColor::BrightCyan => "#00ffcc",
                AnsiNamedColor::BrightWhite => "#f0f6fc",
            },
            AnsiColor::Indexed(idx) => match idx {
                0 => "#000000",
                1 => "#f85149",
                2 => "#3fb950",
                3 => "#d29922",
                4 => "#58a6ff",
                5 => "#bc8cff",
                6 => "#38bdf8",
                7 => "#ffffff",
                8 => "#484f58",
                9 => "#ff7b72",
                10 => "#56d364",
                11 => "#e3b341",
                12 => "#79c0ff",
                13 => "#d2a8ff",
                14 => "#00ffcc",
                15 => "#f0f6fc",
                _ => {
                    if is_foreground {
                        "#c9d1d9"
                    } else {
                        "#161b22"
                    }
                }
            },
            AnsiColor::Rgb(..) => "#00ffcc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCell {
    pub c: char,
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub bold: bool,
    pub dim: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: AnsiColor::Default,
            bg: AnsiColor::Default,
            bold: false,
            dim: false,
            underline: false,
            inverse: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub current_fg: AnsiColor,
    pub current_bg: AnsiColor,
    pub current_bold: bool,
    pub current_dim: bool,
    pub current_underline: bool,
    pub current_inverse: bool,
    pub lines: Vec<Vec<TerminalCell>>,
    pub max_scrollback: usize,
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let mut grid = Self {
            cols,
            rows,
            cursor_col: 0,
            cursor_row: 0,
            current_fg: AnsiColor::Default,
            current_bg: AnsiColor::Default,
            current_bold: false,
            current_dim: false,
            current_underline: false,
            current_inverse: false,
            lines: Vec::new(),
            max_scrollback: 2000,
        };
        grid.lines.push(Vec::new());
        grid
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines.push(Vec::new());
        self.cursor_col = 0;
        self.cursor_row = 0;
    }

    pub fn write_stream(&mut self, text: &str) {
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    let mut seq = String::new();
                    for next_ch in chars.by_ref() {
                        seq.push(next_ch);
                        if next_ch.is_ascii_alphabetic() || next_ch == '@' || next_ch == '`' || seq.len() >= 64 {
                            break;
                        }
                    }
                    self.parse_csi_sequence(&seq);
                } else if chars.peek() == Some(&']') {
                    // OSC sequence: skip until BEL (\x07) or ST (\x1b\)
                    chars.next(); // consume ']'
                    for osc_ch in chars.by_ref() {
                        if osc_ch == '\x07' || osc_ch == '\x1b' {
                            break;
                        }
                    }
                }
            } else if ch == '\r' {
                self.cursor_col = 0;
            } else if ch == '\n' {
                self.cursor_col = 0;
                self.lines.push(Vec::new());
                if self.lines.len() > self.max_scrollback {
                    self.lines.remove(0);
                }
            } else if ch == '\x08' {
                // Backspace
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    if let Some(line) = self.lines.last_mut()
                        && self.cursor_col < line.len()
                    {
                        line.remove(self.cursor_col);
                    }
                }
            } else if ch == '\t' {
                let tab_stop = 4;
                let spaces = tab_stop - (self.cursor_col % tab_stop);
                for _ in 0..spaces {
                    self.push_char(' ');
                }
            } else {
                self.push_char(ch);
            }
        }
    }

    fn push_char(&mut self, ch: char) {
        let cell = TerminalCell {
            c: ch,
            fg: self.current_fg,
            bg: self.current_bg,
            bold: self.current_bold,
            dim: self.current_dim,
            underline: self.current_underline,
            inverse: self.current_inverse,
        };

        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        let Some(line) = self.lines.last_mut() else {
            return;
        };

        if self.cursor_col < line.len() {
            line[self.cursor_col] = cell;
        } else {
            while line.len() < self.cursor_col {
                line.push(TerminalCell::default());
            }
            line.push(cell);
        }
        self.cursor_col += 1;
    }

    fn parse_csi_sequence(&mut self, seq: &str) {
        if let Some(params) = seq.strip_suffix('m') {
            // SGR (Select Graphic Rendition)
            if params.is_empty() || params == "0" {
                self.current_fg = AnsiColor::Default;
                self.current_bg = AnsiColor::Default;
                self.current_bold = false;
                self.current_dim = false;
                self.current_underline = false;
                self.current_inverse = false;
                return;
            }

            let parts: Vec<&str> = params.split(';').collect();
            let mut i = 0;
            while i < parts.len() {
                match parts[i].parse::<u16>().unwrap_or(0) {
                    0 => {
                        self.current_fg = AnsiColor::Default;
                        self.current_bg = AnsiColor::Default;
                        self.current_bold = false;
                        self.current_dim = false;
                        self.current_underline = false;
                        self.current_inverse = false;
                    }
                    1 => self.current_bold = true,
                    2 => self.current_dim = true,
                    4 => self.current_underline = true,
                    7 => self.current_inverse = true,
                    22 => {
                        self.current_bold = false;
                        self.current_dim = false;
                    }
                    24 => self.current_underline = false,
                    27 => self.current_inverse = false,
                    30 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Black),
                    31 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Red),
                    32 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Green),
                    33 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Yellow),
                    34 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Blue),
                    35 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Magenta),
                    36 => self.current_fg = AnsiColor::Named(AnsiNamedColor::Cyan),
                    37 => self.current_fg = AnsiColor::Named(AnsiNamedColor::White),
                    39 => self.current_fg = AnsiColor::Default,
                    40 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Black),
                    41 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Red),
                    42 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Green),
                    43 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Yellow),
                    44 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Blue),
                    45 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Magenta),
                    46 => self.current_bg = AnsiColor::Named(AnsiNamedColor::Cyan),
                    47 => self.current_bg = AnsiColor::Named(AnsiNamedColor::White),
                    49 => self.current_bg = AnsiColor::Default,
                    90 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightBlack),
                    91 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightRed),
                    92 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightGreen),
                    93 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightYellow),
                    94 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightBlue),
                    95 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightMagenta),
                    96 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightCyan),
                    97 => self.current_fg = AnsiColor::Named(AnsiNamedColor::BrightWhite),
                    38 if i + 2 < parts.len() && parts[i + 1] == "5" => {
                        let color_idx = parts[i + 2].parse::<u8>().unwrap_or(0);
                        self.current_fg = AnsiColor::Indexed(color_idx);
                        i += 2;
                    }
                    48 if i + 2 < parts.len() && parts[i + 1] == "5" => {
                        let color_idx = parts[i + 2].parse::<u8>().unwrap_or(0);
                        self.current_bg = AnsiColor::Indexed(color_idx);
                        i += 2;
                    }
                    _ => {}
                }
                i += 1;
            }
        } else if seq.ends_with('K')
            && let Some(line) = self.lines.last_mut()
            && self.cursor_col < line.len()
        {
            line.truncate(self.cursor_col);
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_cells(&self, idx: usize) -> &[TerminalCell] {
        if idx < self.lines.len() {
            &self.lines[idx]
        } else {
            &[]
        }
    }
}
