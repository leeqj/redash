use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Agent is idle or awaiting user query
    Idle,
    /// Agent is actively thinking, generating code, or running tools (cyan breathing LED)
    Thinking,
    /// Agent is waiting for user confirmation or permission approval (amber alert LED + notification)
    NeedsInput,
    /// Agent has completed its current task turn (solid emerald green LED)
    Done,
}

impl AgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Idle => "就绪",
            AgentStatus::Thinking => "思考中",
            AgentStatus::NeedsInput => "等待授权",
            AgentStatus::Done => "完成",
        }
    }

    pub fn color_rgb(&self) -> u32 {
        match self {
            AgentStatus::Idle => 0x64748b,       // Slate gray
            AgentStatus::Thinking => 0x38bdf8,   // Tech cyan
            AgentStatus::NeedsInput => 0xf59e0b, // Amber alert
            AgentStatus::Done => 0x10b981,       // Emerald green
        }
    }

    pub fn color_hex(&self) -> &'static str {
        match self {
            AgentStatus::Idle => "#64748b",
            AgentStatus::Thinking => "#38bdf8",
            AgentStatus::NeedsInput => "#f59e0b",
            AgentStatus::Done => "#10b981",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedAgent {
    pub id: String,
    pub name: String,
    pub category: String,
    pub status: AgentStatus,
    pub detail: String,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub process_names: &'static [&'static str],
    pub title_patterns: &'static [&'static str],
    pub prompt_signatures: &'static [&'static str],
}

pub static KNOWN_AGENTS: &[AgentDefinition] = &[
    AgentDefinition {
        id: "claude",
        name: "Claude Code",
        category: "Top Frontier",
        process_names: &["claude"],
        title_patterns: &["claude code", "claude"],
        prompt_signatures: &["claude-code", "claude>", "anthropics/claude"],
    },
    AgentDefinition {
        id: "codex",
        name: "Codex CLI",
        category: "Top Frontier",
        process_names: &["codex"],
        title_patterns: &["codex", "openai-codex"],
        prompt_signatures: &["codex>", "openai codex"],
    },
    AgentDefinition {
        id: "antigravity",
        name: "Antigravity CLI",
        category: "Top Frontier",
        process_names: &["agy", "antigravity"],
        title_patterns: &["antigravity", "agy"],
        prompt_signatures: &["antigravity", "agy>"],
    },
    AgentDefinition {
        id: "gemini",
        name: "Gemini CLI",
        category: "Top Frontier",
        process_names: &["gemini"],
        title_patterns: &["gemini-cli", "gemini"],
        prompt_signatures: &["@google/gemini-cli", "gemini>"],
    },
    AgentDefinition {
        id: "copilot",
        name: "Copilot CLI",
        category: "Top Frontier",
        process_names: &["copilot", "github-copilot-cli"],
        title_patterns: &["copilot", "github copilot"],
        prompt_signatures: &["copilot?", "copilot-agent"],
    },
    AgentDefinition {
        id: "cursor",
        name: "Cursor Agent",
        category: "Top Frontier",
        process_names: &["cursor-agent", "cursor"],
        title_patterns: &["cursor agent", "cursor-terminal"],
        prompt_signatures: &["cursor-agent>", "cursor>"],
    },
    AgentDefinition {
        id: "windsurf",
        name: "Windsurf Cascade",
        category: "Top Frontier",
        process_names: &["cascade", "windsurf"],
        title_patterns: &["cascade agent", "windsurf"],
        prompt_signatures: &["cascade>", "windsurf>"],
    },
    AgentDefinition {
        id: "kimi",
        name: "Kimi CLI",
        category: "Chinese Frontier",
        process_names: &["kimi", "kimi-cli"],
        title_patterns: &["kimi-cli", "kimi"],
        prompt_signatures: &["kimi>", "moonshot/kimi"],
    },
    AgentDefinition {
        id: "deepseek",
        name: "DeepSeek CLI",
        category: "Chinese Frontier",
        process_names: &["deepseek", "deepseek-cli"],
        title_patterns: &["deepseek-cli", "deepseek"],
        prompt_signatures: &["deepseek>", "deepseek-coder"],
    },
    AgentDefinition {
        id: "qwen",
        name: "Qwen Code",
        category: "Chinese Frontier",
        process_names: &["qwen", "qwen-code"],
        title_patterns: &["qwen-code", "qwen"],
        prompt_signatures: &["qwen>", "alibaba/qwen"],
    },
    AgentDefinition {
        id: "glm",
        name: "GLM Code",
        category: "Chinese Frontier",
        process_names: &["glm", "chatglm"],
        title_patterns: &["chatglm", "glm-cli"],
        prompt_signatures: &["glm>", "zhipu/glm"],
    },
    AgentDefinition {
        id: "doubao",
        name: "Doubao Coder",
        category: "Chinese Frontier",
        process_names: &["doubao", "doubao-coder"],
        title_patterns: &["doubao-coder", "doubao"],
        prompt_signatures: &["doubao>", "bytedance/doubao"],
    },
    AgentDefinition {
        id: "aider",
        name: "Aider",
        category: "Coding Assistants",
        process_names: &["aider"],
        title_patterns: &["aider"],
        prompt_signatures: &["aider>", "tokens remaining:"],
    },
    AgentDefinition {
        id: "openhands",
        name: "OpenHands",
        category: "Coding Assistants",
        process_names: &["openhands"],
        title_patterns: &["openhands"],
        prompt_signatures: &["openhands>", "all-hands-ai"],
    },
    AgentDefinition {
        id: "devin",
        name: "Devin CLI",
        category: "Coding Assistants",
        process_names: &["devin"],
        title_patterns: &["devin"],
        prompt_signatures: &["devin>", "cognition-labs"],
    },
    AgentDefinition {
        id: "mentat",
        name: "Mentat",
        category: "Coding Assistants",
        process_names: &["mentat"],
        title_patterns: &["mentat"],
        prompt_signatures: &["mentat>"],
    },
    AgentDefinition {
        id: "plandex",
        name: "Plandex",
        category: "Coding Assistants",
        process_names: &["plandex"],
        title_patterns: &["plandex"],
        prompt_signatures: &["plandex>"],
    },
    AgentDefinition {
        id: "goose",
        name: "Goose",
        category: "Coding Assistants",
        process_names: &["goose"],
        title_patterns: &["goose"],
        prompt_signatures: &["goose-agent", "goose>"],
    },
    AgentDefinition {
        id: "crush",
        name: "Crush",
        category: "Coding Assistants",
        process_names: &["crush"],
        title_patterns: &["crush"],
        prompt_signatures: &["crush-agent", "crush>"],
    },
    AgentDefinition {
        id: "cline",
        name: "Cline",
        category: "Coding Assistants",
        process_names: &["cline"],
        title_patterns: &["cline"],
        prompt_signatures: &["cline-cli", "cline>"],
    },
    AgentDefinition {
        id: "roo",
        name: "Roo Code",
        category: "Coding Assistants",
        process_names: &["roo"],
        title_patterns: &["roo", "roo-code"],
        prompt_signatures: &["roo>"],
    },
    AgentDefinition {
        id: "amp",
        name: "Amp",
        category: "Lightweight & Specialized",
        process_names: &["amp"],
        title_patterns: &["amp"],
        prompt_signatures: &["amp-cli", "amp>"],
    },
    AgentDefinition {
        id: "auggie",
        name: "Auggie",
        category: "Lightweight & Specialized",
        process_names: &["auggie"],
        title_patterns: &["auggie"],
        prompt_signatures: &["auggie-code", "auggie>"],
    },
    AgentDefinition {
        id: "hermes",
        name: "Hermes",
        category: "Lightweight & Specialized",
        process_names: &["hermes"],
        title_patterns: &["hermes"],
        prompt_signatures: &["hermes-agent", "hermes>"],
    },
    AgentDefinition {
        id: "vibe",
        name: "Vibe",
        category: "Lightweight & Specialized",
        process_names: &["vibe"],
        title_patterns: &["vibe"],
        prompt_signatures: &["vibe-cli", "vibe>"],
    },
    AgentDefinition {
        id: "qoder",
        name: "Qoder",
        category: "Lightweight & Specialized",
        process_names: &["qoder"],
        title_patterns: &["qoder"],
        prompt_signatures: &["qoder-cli", "qoder>"],
    },
];

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
pub fn extract_metrics_from_buffer(text: &str) -> (Option<f64>, Option<u64>) {
    let lower = text.to_lowercase();
    let cost = extract_cost(&lower);
    let tokens = extract_tokens(&lower);
    (cost, tokens)
}
