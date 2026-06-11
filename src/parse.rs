//! Defensive parsing of agent-session JSONL transcripts (Claude Code's
//! `~/.claude/projects/*/*.jsonl` format): every line is a JSON object;
//! unknown shapes are skipped rather than failed.

use serde_json::Value;

/// Who produced an event.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// One transcript event, flattened for display.
#[derive(Clone)]
pub struct Event {
    pub role: Role,
    /// First line of content, for the timeline row.
    pub preview: String,
    /// Full content (text blocks, tool payloads), for the detail pane.
    pub detail: String,
    /// Tool name when the event carries a tool call.
    pub tool: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// HH:MM:SS, when the line carried a timestamp.
    pub time: String,
}

pub fn parse_session(text: &str) -> Vec<Event> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|v| parse_event(&v))
        .collect()
}

fn parse_event(v: &Value) -> Option<Event> {
    let typ = v.get("type")?.as_str()?;
    if typ != "user" && typ != "assistant" {
        return None;
    }
    let msg = v.get("message")?;
    let time = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| t.get(11..19))
        .unwrap_or("")
        .to_owned();
    let usage = msg.get("usage");
    let grab = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };

    let mut texts: Vec<String> = Vec::new();
    let mut detail = String::new();
    let mut tool = None;
    let mut only_tool_results = true;

    match msg.get("content") {
        Some(Value::String(s)) => {
            only_tool_results = false;
            texts.push(s.clone());
            detail.push_str(s);
        }
        Some(Value::Array(blocks)) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        only_tool_results = false;
                        if let Some(t) = block.get("text").and_then(Value::as_str) {
                            texts.push(t.to_owned());
                            detail.push_str(t);
                            detail.push_str("\n\n");
                        }
                    }
                    Some("tool_use") => {
                        only_tool_results = false;
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_owned();
                        let input = block
                            .get("input")
                            .map(|i| serde_json::to_string_pretty(i).unwrap_or_default())
                            .unwrap_or_default();
                        detail.push_str(&format!("[tool_use {name}]\n{input}\n\n"));
                        tool.get_or_insert(name);
                    }
                    Some("tool_result") => {
                        let body = match block.get("content") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(parts)) => parts
                                .iter()
                                .filter_map(|p| p.get("text").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join("\n"),
                            _ => String::new(),
                        };
                        detail.push_str(&format!("[tool_result]\n{body}\n\n"));
                        if texts.is_empty() {
                            texts.push(body.lines().next().unwrap_or("").to_owned());
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => return None,
    }

    let role = if typ == "assistant" {
        Role::Assistant
    } else if only_tool_results {
        Role::Tool
    } else {
        Role::User
    };
    let mut preview = texts
        .first()
        .map(|t| t.lines().next().unwrap_or("").to_owned())
        .unwrap_or_default();
    if preview.is_empty()
        && let Some(t) = &tool
    {
        preview = format!("[{t}]");
    }
    if preview.is_empty() {
        preview = "(empty)".to_owned();
    }
    preview.truncate(160);

    Some(Event {
        role,
        preview,
        detail: detail.trim_end().to_owned(),
        tool,
        tokens_in: grab("input_tokens"),
        tokens_out: grab("output_tokens"),
        time,
    })
}
