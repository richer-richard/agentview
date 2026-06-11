//! The fixture parses into the expected flattened events.

#[path = "../src/parse.rs"]
mod parse;

use parse::{Role, parse_session};

#[test]
fn fixture_parses() {
    let events = parse_session(include_str!("fixture.jsonl"));
    assert_eq!(events.len(), 8);
    assert!(matches!(events[0].role, Role::User));
    assert_eq!(events[0].role.label(), "user");
    assert!(events[0].preview.starts_with("Add a dark mode toggle"));
    assert_eq!(events[1].tokens_in, 2412);
    assert!(matches!(events[1].role, Role::Assistant));
    assert_eq!(events[1].tool.as_deref(), Some("Read"));
    assert!(matches!(events[2].role, Role::Tool));
    assert_eq!(events[1].tokens_out, 188);
    assert!(events[3].detail.contains("tool_use Edit"));
    assert_eq!(events[0].time, "09:14:02");
}
