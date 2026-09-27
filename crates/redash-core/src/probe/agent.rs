pub use redash_types::agent::{AgentDefinition, AgentStatus, DetectedAgent, KNOWN_AGENTS};

pub struct AgentDetector;

impl AgentDetector {
    /// Extracts ANSI OSC 0 and OSC 2 window title sequences from a raw terminal byte stream.
    pub fn extract_osc_titles(bytes: &[u8]) -> Vec<String> {
        let mut titles = Vec::new();
        let mut i = 0;
        let len = bytes.len();

        while i + 3 < len {
            // Check for OSC prefix: \x1b]0; or \x1b]2;
            if bytes[i] == 0x1b
                && bytes[i + 1] == b']'
                && (bytes[i + 2] == b'0' || bytes[i + 2] == b'2')
                && bytes[i + 3] == b';'
            {
                let start = i + 4;
                let mut end = start;
                while end < len {
                    // Terminator: BEL (\x07) or ST (\x1b\)
                    if bytes[end] == 0x07 {
                        if let Ok(title) = std::str::from_utf8(&bytes[start..end]) {
                            titles.push(title.trim().to_string());
                        }
                        i = end + 1;
                        break;
                    } else if bytes[end] == 0x1b && end + 1 < len && bytes[end + 1] == b'\\' {
                        if let Ok(title) = std::str::from_utf8(&bytes[start..end]) {
                            titles.push(title.trim().to_string());
                        }
                        i = end + 2;
                        break;
                    }
                    end += 1;
                }
                if end >= len {
                    break;
                }
            } else {
                i += 1;
            }
        }

        titles
    }

    /// Primary multi-tier detection combining process names, window titles, and buffer heuristics.
    pub fn detect(
        process_name: Option<&str>,
        window_title: Option<&str>,
        buffer_text: &str,
    ) -> Option<DetectedAgent> {
        let title_lower = window_title.map(|s| s.to_lowercase());
        let proc_lower = process_name.map(|s| s.to_lowercase());
        let buf_lower = buffer_text.to_lowercase();

        // 1. Try finding matched AgentDefinition
        let mut matched_agent: Option<&'static AgentDefinition> = None;

        // Tier 1: Check window title patterns
        if let Some(ref title) = title_lower {
            for agent in KNOWN_AGENTS {
                for pattern in agent.title_patterns {
                    if title.contains(pattern) {
                        matched_agent = Some(agent);
                        break;
                    }
                }
                if matched_agent.is_some() {
                    break;
                }
            }
        }

        // Tier 2: Check active process name
        if matched_agent.is_none()
            && let Some(ref proc) = proc_lower
        {
            for agent in KNOWN_AGENTS {
                for p in agent.process_names {
                    if proc == p || proc.ends_with(&format!("/{}", p)) {
                        matched_agent = Some(agent);
                        break;
                    }
                }
                if matched_agent.is_some() {
                    break;
                }
            }
        }

        // Tier 3: Check buffer prompt signatures
        if matched_agent.is_none() {
            for agent in KNOWN_AGENTS {
                for sig in agent.prompt_signatures {
                    if buf_lower.contains(sig) {
                        matched_agent = Some(agent);
                        break;
                    }
                }
                if matched_agent.is_some() {
                    break;
                }
            }
        }

        let agent = matched_agent?;

        // 2. Evaluate current lifecycle status
        let status = Self::evaluate_status(title_lower.as_deref(), &buf_lower);

        let detail = match status {
            AgentStatus::NeedsInput => "正在等待用户授权或确认操作".to_string(),
            AgentStatus::Thinking => "正在分析代码或执行工具调用".to_string(),
            AgentStatus::Done => "本轮任务执行完毕".to_string(),
            AgentStatus::Idle => "等待用户指令".to_string(),
        };

        let (cost_usd, tokens) = extract_metrics_from_buffer(buffer_text);

        Some(DetectedAgent {
            id: agent.id.to_string(),
            name: agent.name.to_string(),
            category: agent.category.to_string(),
            status,
            detail,
            cost_usd,
            tokens,
        })
    }

    /// Extracts cost and token metrics from buffer text.
    pub fn extract_metrics_from_buffer(text: &str) -> (Option<f64>, Option<u64>) {
        extract_metrics_from_buffer(text)
    }

    /// Evaluates current AgentStatus from window title and screen buffer.
    pub fn evaluate_status(title_lower: Option<&str>, buf_lower: &str) -> AgentStatus {
        // Prefer the most recent explicit lifecycle signal in the screen buffer.
        const NEEDS_INPUT_PATTERNS: &[&str] = &[
            "[y/n]",
            "[yes/no]",
            "(y/n)",
            "(yes/no)",
            "allow tool call",
            "confirm execution",
            "press enter to continue",
            "do you want to run",
            "allow command",
            "allow editing",
            "execute this command?",
            "run this command?",
            "确认执行",
            "是否继续",
            "按回车继续",
            "是否允许",
        ];

        // Thinking patterns
        const THINKING_PATTERNS: &[&str] = &[
            "thinking...",
            "generating...",
            "running tool:",
            "reading file:",
            "editing file:",
            "searching codebase:",
            "executing command:",
            "思考中...",
            "生成中...",
            "执行工具中...",
        ];

        // Cost/token summaries and individual tool completions can appear while
        // work is still running; they are not task-completion signals.
        const DONE_PATTERNS: &[&str] = &["task completed", "任务完成", "执行完成"];

        let recent = [
            (NEEDS_INPUT_PATTERNS, AgentStatus::NeedsInput),
            (THINKING_PATTERNS, AgentStatus::Thinking),
            (DONE_PATTERNS, AgentStatus::Done),
        ]
        .into_iter()
        .flat_map(|(patterns, status)| {
            patterns.iter().filter_map(move |pattern| {
                buf_lower.rfind(pattern).map(|position| (position, status))
            })
        })
        .max_by_key(|(position, _)| *position);
        if let Some((_, status)) = recent {
            return status;
        }
        if title_lower.is_some_and(|title| {
            title.contains("thinking") || title.contains("generating") || title.contains("running")
        }) {
            AgentStatus::Thinking
        } else {
            AgentStatus::Idle
        }
    }
}

fn is_word_boundary_before(text: &str, byte_idx: usize) -> bool {
    if byte_idx == 0 {
        return true;
    }
    if !text.is_char_boundary(byte_idx) {
        return false;
    }
    match text[..byte_idx].chars().next_back() {
        Some(c) => !c.is_ascii_alphanumeric() && c != '_',
        None => true,
    }
}

fn extract_cost(text_lower: &str) -> Option<f64> {
    let mut last_cost = None;
    let mut search_idx = 0;

    while let Some(pos) = text_lower[search_idx..].find("cost") {
        let abs_pos = search_idx + pos;
        let is_total_cost = text_lower[..abs_pos].ends_with("total ")
            && is_word_boundary_before(text_lower, abs_pos.saturating_sub("total ".len()));
        let is_standalone_cost = is_word_boundary_before(text_lower, abs_pos);

        if is_total_cost || is_standalone_cost {
            let after_kw = &text_lower[abs_pos + "cost".len()..];
            let trimmed = after_kw.trim_start();
            let after_delim = trimmed
                .strip_prefix(':')
                .or_else(|| trimmed.strip_prefix('='))
                .map(|s| s.trim_start());

            if let Some(after_delim) = after_delim {
                let after_currency = if let Some(stripped) = after_delim.strip_prefix('$') {
                    stripped.trim_start()
                } else {
                    after_delim
                };

                let num_str: String = after_currency
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();

                if !num_str.is_empty()
                    && num_str != "."
                    && let Ok(val) = num_str.parse::<f64>()
                {
                    last_cost = Some(val);
                }
            }
        }
        search_idx = abs_pos + "cost".len();
    }

    last_cost
}

fn parse_token_str(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(num_str) = s.strip_suffix(['k', 'K']) {
        let clean = num_str.trim().replace(',', "");
        let val: f64 = clean.parse().ok()?;
        return Some((val * 1_000.0).round() as u64);
    }
    if let Some(num_str) = s.strip_suffix(['m', 'M']) {
        let clean = num_str.trim().replace(',', "");
        let val: f64 = clean.parse().ok()?;
        return Some((val * 1_000_000.0).round() as u64);
    }
    let clean = s.replace(',', "");
    if let Ok(val) = clean.parse::<u64>() {
        Some(val)
    } else if let Ok(val) = clean.parse::<f64>() {
        Some(val.round() as u64)
    } else {
        None
    }
}

fn extract_tokens(text_lower: &str) -> Option<u64> {
    let mut last_tokens = None;
    let mut search_idx = 0;

    while let Some(pos) = text_lower[search_idx..].find("token") {
        let abs_pos = search_idx + pos;
        if is_word_boundary_before(text_lower, abs_pos) {
            let rest = &text_lower[abs_pos..];
            let after_kw = rest
                .strip_prefix("tokens")
                .or_else(|| rest.strip_prefix("token"));

            if let Some(after_kw) = after_kw {
                let trimmed = after_kw.trim_start();
                let after_delim = trimmed
                    .strip_prefix(':')
                    .or_else(|| trimmed.strip_prefix('='))
                    .map(|s| s.trim_start());

                if let Some(after_delim) = after_delim {
                    let mut words = after_delim.split_whitespace();
                    if let Some(word0) = words.next() {
                        let clean_word0 = word0.trim_matches(|c: char| {
                            c == ','
                                || c == ';'
                                || c == ')'
                                || c == ']'
                                || c == '}'
                                || c == '!'
                                || c == '?'
                                || c == ':'
                        });
                        if let Some(next_word) = words.next() {
                            let clean_next = next_word.trim_matches(|c: char| {
                                c == ','
                                    || c == ';'
                                    || c == ')'
                                    || c == ']'
                                    || c == '}'
                                    || c == '!'
                                    || c == '?'
                                    || c == ':'
                            });
                            if (clean_next == "k" || clean_next == "m")
                                && clean_word0
                                    .chars()
                                    .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
                            {
                                let combined = format!("{}{}", clean_word0, clean_next);
                                if let Some(val) = parse_token_str(&combined) {
                                    last_tokens = Some(val);
                                }
                            } else if let Some(val) = parse_token_str(clean_word0) {
                                last_tokens = Some(val);
                            }
                        } else if let Some(val) = parse_token_str(clean_word0) {
                            last_tokens = Some(val);
                        }
                    }
                }
            }
        }
        search_idx = abs_pos + "token".len();
    }

    last_tokens
}

/// Parses LLM metrics from raw terminal buffer text.
///
/// Matches:
/// - Cost: `Cost: $X.XX` or `total cost: $X.XX` (e.g. `0.045`)
/// - Tokens: `Tokens: XXX` or `tokens: 12.4k` (parsed to 12400) or `12450`
pub fn extract_metrics_from_buffer(text: &str) -> (Option<f64>, Option<u64>) {
    let lower = text.to_lowercase();
    let cost = extract_cost(&lower);
    let tokens = extract_tokens(&lower);
    (cost, tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_agent_status_overrides_old_prompt() {
        assert_eq!(
            AgentDetector::evaluate_status(None, "allow command [y/n]\ntask completed"),
            AgentStatus::Done
        );
        assert_eq!(
            AgentDetector::evaluate_status(None, "task completed\nrunning tool:"),
            AgentStatus::Thinking
        );
    }

    #[test]
    fn test_all_known_agents_count() {
        assert_eq!(KNOWN_AGENTS.len(), 26);
        for agent in KNOWN_AGENTS {
            assert!(!agent.name.is_empty());
            assert!(!agent.process_names.is_empty());
        }
    }

    #[test]
    fn test_extract_osc_titles() {
        let stream = b"some data\x1b]0;Claude Code - Thinking...\x07normal output\x1b]2;Aider - Working\x1b\\done";
        let titles = AgentDetector::extract_osc_titles(stream);
        assert_eq!(titles.len(), 2);
        assert_eq!(titles[0], "Claude Code - Thinking...");
        assert_eq!(titles[1], "Aider - Working");
    }

    #[test]
    fn test_detect_claude_needs_input() {
        let buffer = "Allow tool call? Bash(cargo test) [y/N]: ";
        let detected = AgentDetector::detect(Some("claude"), None, buffer);
        assert!(detected.is_some());
        let agent = detected.unwrap();
        assert_eq!(agent.id, "claude");
        assert_eq!(agent.name, "Claude Code");
        assert_eq!(agent.status, AgentStatus::NeedsInput);
    }

    #[test]
    fn test_detect_aider_thinking() {
        let buffer = "Thinking...\nSearching codebase: src/main.rs";
        let detected = AgentDetector::detect(Some("aider"), None, buffer);
        assert!(detected.is_some());
        let agent = detected.unwrap();
        assert_eq!(agent.id, "aider");
        assert_eq!(agent.status, AgentStatus::Thinking);
    }

    #[test]
    fn test_detect_antigravity_from_title() {
        let title = "Antigravity - Task in progress";
        let buffer = "Running tool: replace_file_content";
        let detected = AgentDetector::detect(None, Some(title), buffer);
        assert!(detected.is_some());
        let agent = detected.unwrap();
        assert_eq!(agent.id, "antigravity");
        assert_eq!(agent.name, "Antigravity CLI");
        assert_eq!(agent.status, AgentStatus::Thinking);
    }

    #[test]
    fn test_detect_kimi_chinese_prompt() {
        let buffer = "是否允许执行该命令: rm -rf target [y/N]? 确认执行: ";
        let detected = AgentDetector::detect(Some("kimi"), None, buffer);
        assert!(detected.is_some());
        let agent = detected.unwrap();
        assert_eq!(agent.id, "kimi");
        assert_eq!(agent.name, "Kimi CLI");
        assert_eq!(agent.status, AgentStatus::NeedsInput);
    }

    #[test]
    fn test_detect_done_state() {
        let buffer = "Task completed! Cost: $0.045, Tokens: 12450";
        let detected = AgentDetector::detect(Some("claude"), None, buffer);
        assert!(detected.is_some());
        let agent = detected.unwrap();
        assert_eq!(agent.status, AgentStatus::Done);
        assert!(agent.cost_usd.is_some_and(|c| (c - 0.045).abs() < 1e-6));
        assert_eq!(agent.tokens, Some(12450));
    }

    #[test]
    fn test_extract_metrics_from_buffer_cost_and_tokens() {
        let (cost, tokens) = extract_metrics_from_buffer("Cost: $0.045, Tokens: 12450");
        assert!(cost.is_some_and(|c| (c - 0.045).abs() < 1e-6));
        assert_eq!(tokens, Some(12450));

        let (cost, tokens) = extract_metrics_from_buffer("total cost: $0.045, tokens: 12.4k");
        assert!(cost.is_some_and(|c| (c - 0.045).abs() < 1e-6));
        assert_eq!(tokens, Some(12400));

        let (cost, tokens) = extract_metrics_from_buffer("total cost: 0.045, tokens: 12.4 k");
        assert!(cost.is_some_and(|c| (c - 0.045).abs() < 1e-6));
        assert_eq!(tokens, Some(12400));

        let (cost, tokens) = extract_metrics_from_buffer("Cost: $ 1.25, tokens: 12,450");
        assert!(cost.is_some_and(|c| (c - 1.25).abs() < 1e-6));
        assert_eq!(tokens, Some(12450));

        let (cost, tokens) = extract_metrics_from_buffer("tokens: 1.5M, cost: $0.15");
        assert!(cost.is_some_and(|c| (c - 0.15).abs() < 1e-6));
        assert_eq!(tokens, Some(1500000));

        let (cost, tokens) = extract_metrics_from_buffer("plain text with no metrics");
        assert_eq!(cost, None);
        assert_eq!(tokens, None);
    }

    #[test]
    fn test_detect_agent_metrics_extraction() {
        let buffer = "Task completed! Cost: $0.045, Tokens: 12450";
        let detected =
            AgentDetector::detect(Some("claude"), None, buffer).expect("Should detect claude");
        assert_eq!(detected.status, AgentStatus::Done);
        assert!(detected.cost_usd.is_some_and(|c| (c - 0.045).abs() < 1e-6));
        assert_eq!(detected.tokens, Some(12450));

        let buffer2 = "Thinking... total cost: $0.02, tokens: 12.4k";
        let detected2 =
            AgentDetector::detect(Some("aider"), None, buffer2).expect("Should detect aider");
        assert_eq!(detected2.status, AgentStatus::Thinking);
        assert!(detected2.cost_usd.is_some_and(|c| (c - 0.02).abs() < 1e-6));
        assert_eq!(detected2.tokens, Some(12400));

        assert_eq!(
            AgentDetector::evaluate_status(None, "cost: $0.02, tokens: 12.4k"),
            AgentStatus::Idle
        );
    }

    #[test]
    fn test_detected_agent_serde() {
        let agent = DetectedAgent {
            id: "claude".into(),
            name: "Claude Code".into(),
            category: "Top Frontier".into(),
            status: AgentStatus::Done,
            detail: "done".into(),
            cost_usd: Some(0.045),
            tokens: Some(12450),
        };
        let json = serde_json::to_string(&agent).expect("Serialize");
        let deserialized: DetectedAgent = serde_json::from_str(&json).expect("Deserialize");
        assert_eq!(deserialized, agent);

        // Test backward-compatibility when cost_usd and tokens are missing
        let legacy_json = r#"{
            "id": "claude",
            "name": "Claude Code",
            "category": "Top Frontier",
            "status": "Done",
            "detail": "done"
        }"#;
        let deserialized_legacy: DetectedAgent =
            serde_json::from_str(legacy_json).expect("Deserialize legacy");
        assert_eq!(deserialized_legacy.cost_usd, None);
        assert_eq!(deserialized_legacy.tokens, None);
    }

    #[test]
    fn test_extract_metrics_utf8_boundaries() {
        // Multi-byte CJK characters and emojis directly before keywords to test char boundary safety
        let buffer = "模型思考中 🚀🔥 当前花费 total cost: $0.123 消耗 tokens: 4.5k";
        let (cost, tokens) = extract_metrics_from_buffer(buffer);
        assert!(cost.is_some_and(|c| (c - 0.123).abs() < 1e-6));
        assert_eq!(tokens, Some(4500));

        let buffer2 = "测试cost=$99.99 tokens: 100";
        let (cost2, tokens2) = extract_metrics_from_buffer(buffer2);
        assert!(cost2.is_some_and(|c| (c - 99.99).abs() < 1e-6));
        assert_eq!(tokens2, Some(100));
    }
}
