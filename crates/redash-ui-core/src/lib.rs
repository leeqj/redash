pub mod i18n;
pub mod state;
pub mod terminal;
pub mod theme;

pub use i18n::*;
pub use state::*;
pub use terminal::*;
pub use theme::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_palette_resolution() {
        let dark = get_palette("DarkTech");
        assert_eq!(dark.name, "DarkTech");
        assert_eq!(dark.accent_cyan.to_hex_str(), "#00ffcc");

        let cyber = get_palette("CyberpunkNeon");
        assert_eq!(cyber.name, "CyberpunkNeon");

        let sol = get_palette("SolarizedDark");
        assert_eq!(sol.name, "SolarizedDark");
    }

    #[test]
    fn test_terminal_grid_ansi_parsing() {
        let mut grid = TerminalGrid::new(80, 24);
        // Write text with green color: \x1b[32mSUCCESS\x1b[0m
        grid.write_stream("\x1b[32mSUCCESS\x1b[0m\n");

        assert!(grid.line_count() >= 2);
        let first_line = grid.line_cells(0);
        assert_eq!(first_line.len(), 7);
        assert_eq!(first_line[0].c, 'S');
        assert_eq!(first_line[0].fg, AnsiColor::Named(AnsiNamedColor::Green));

        // Bold and 256 colors
        grid.write_stream("\x1b[1;38;5;14mCYAN BOLD\x1b[0m");
        let second_line = grid.line_cells(1);
        assert_eq!(second_line[0].c, 'C');
        assert!(second_line[0].bold);
        assert_eq!(second_line[0].fg, AnsiColor::Indexed(14));
    }
}
