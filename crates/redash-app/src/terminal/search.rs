#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalSearch {
    pub query: String,
    pub is_active: bool,
    pub case_sensitive: bool,
    pub matches: Vec<SearchMatch>,
    pub active_match_idx: usize,
}

#[allow(dead_code)]
impl TerminalSearch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.is_active = true;
    }

    pub fn close(&mut self) {
        self.is_active = false;
        self.matches.clear();
        self.active_match_idx = 0;
    }

    pub fn toggle(&mut self) {
        if self.is_active {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn set_query(&mut self, query: String, lines: &[String]) {
        self.query = query;
        self.execute_search(lines);
    }

    pub fn toggle_case_sensitive(&mut self, lines: &[String]) {
        self.case_sensitive = !self.case_sensitive;
        self.execute_search(lines);
    }

    pub fn execute_search(&mut self, lines: &[String]) {
        self.matches.clear();
        self.active_match_idx = 0;

        let query = self.query.trim();
        if query.is_empty() {
            return;
        }

        let query_processed = if self.case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        for (line_idx, line_str) in lines.iter().enumerate() {
            let mut haystack = String::new();
            let mut cells = Vec::new();
            let mut column = 0;
            for character in line_str.chars() {
                let width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
                let folded = if self.case_sensitive {
                    character.to_string()
                } else {
                    character.to_lowercase().collect()
                };
                cells.extend(std::iter::repeat_n((column, column + width), folded.len()));
                haystack.push_str(&folded);
                column += width;
            }

            let mut start = 0;
            while let Some(pos) = haystack[start..].find(&query_processed) {
                let actual_start = start + pos;
                let actual_end = actual_start + query_processed.len();
                self.matches.push(SearchMatch {
                    line: line_idx,
                    col_start: cells[actual_start].0,
                    col_end: cells[actual_end - 1].1,
                });
                start = actual_start + 1.max(query_processed.len());
                if start >= haystack.len() {
                    break;
                }
            }
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.active_match_idx = (self.active_match_idx + 1) % self.matches.len();
        Some(self.matches[self.active_match_idx])
    }

    pub fn prev(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        if self.active_match_idx == 0 {
            self.active_match_idx = self.matches.len() - 1;
        } else {
            self.active_match_idx -= 1;
        }
        Some(self.matches[self.active_match_idx])
    }

    pub fn current(&self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            None
        } else {
            Some(self.matches[self.active_match_idx])
        }
    }

    pub fn match_summary(&self) -> String {
        if self.query.trim().is_empty() {
            "".to_string()
        } else if self.matches.is_empty() {
            "无匹配".to_string()
        } else {
            format!("{}/{}", self.active_match_idx + 1, self.matches.len())
        }
    }

    /// Checks if a given cell at (line, col) is part of any search match.
    /// Returns (is_match, is_active_match).
    pub fn is_match_at(&self, line: usize, col: usize) -> (bool, bool) {
        if !self.is_active || self.matches.is_empty() {
            return (false, false);
        }

        let mut is_match = false;
        let mut is_active = false;

        let active_match = self.current();

        for m in &self.matches {
            if m.line == line && col >= m.col_start && col < m.col_end {
                is_match = true;
                if let Some(am) = active_match
                    && am == *m
                {
                    is_active = true;
                    break;
                }
            }
        }

        (is_match, is_active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_search_uses_terminal_columns() {
        let mut search = TerminalSearch::new();
        search.set_query("世界".into(), &["Hi 世界!".into()]);
        assert_eq!(
            search.matches,
            vec![SearchMatch {
                line: 0,
                col_start: 3,
                col_end: 7
            }]
        );
        search.set_query("X".into(), &["İX".into()]);
        assert_eq!(search.matches[0].col_start, 1);
    }

    #[test]
    fn test_search_matches_and_navigation() {
        let lines = vec![
            "Error: failed to connect to database".to_string(),
            "Retrying database connection in 5s...".to_string(),
            "Database connection established successfully".to_string(),
        ];

        let mut search = TerminalSearch::new();
        search.open();
        search.set_query("database".to_string(), &lines);

        assert_eq!(search.matches.len(), 3);
        assert_eq!(search.match_summary(), "1/3");

        let next = search.next();
        assert_eq!(next.unwrap().line, 1);
        assert_eq!(search.match_summary(), "2/3");

        let next = search.next();
        assert_eq!(next.unwrap().line, 2);
        assert_eq!(search.match_summary(), "3/3");

        // Wraparound
        let next = search.next();
        assert_eq!(next.unwrap().line, 0);
        assert_eq!(search.match_summary(), "1/3");

        // Previous
        let prev = search.prev();
        assert_eq!(prev.unwrap().line, 2);
        assert_eq!(search.match_summary(), "3/3");
    }

    #[test]
    fn test_case_sensitivity() {
        let lines = vec!["Hello hello HELLO".to_string()];
        let mut search = TerminalSearch::new();
        search.open();

        search.set_query("hello".to_string(), &lines);
        assert_eq!(search.matches.len(), 3);

        search.toggle_case_sensitive(&lines);
        assert_eq!(search.matches.len(), 1);
    }

    #[test]
    fn test_cell_match_check() {
        let lines = vec!["target".to_string()];
        let mut search = TerminalSearch::new();
        search.open();
        search.set_query("get".to_string(), &lines);

        // 'get' is at cols 3, 4, 5
        assert_eq!(search.is_match_at(0, 2), (false, false));
        assert_eq!(search.is_match_at(0, 3), (true, true));
        assert_eq!(search.is_match_at(0, 4), (true, true));
        assert_eq!(search.is_match_at(0, 5), (true, true));
        assert_eq!(search.is_match_at(0, 6), (false, false));
    }
}
