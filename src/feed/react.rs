//! Reactions on thread messages: the shared emoji palette, normalization of
//! provider reaction data into the thread display shape, and posting a
//! reaction back through the provider that owns the message.
//!
//! A message that supports posting carries a `react` descriptor in its
//! thread JSON, e.g. `{"provider": "github", "host": null, "subject_id":
//! "<node id>"}`. A forge that does not declare the `reactions`
//! capability omits it; its reactions are display-only.

use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{EphorError, Result};
use crate::feed::provider::run_json;
use crate::feed::providers::gh_command;

/// The palette offered by the TUI picker: (emoji, GitHub content name).
/// These are exactly GitHub's eight reaction contents; other providers map
/// what they can.
pub const PALETTE: [(&str, &str); 8] = [
    ("👍", "THUMBS_UP"),
    ("👎", "THUMBS_DOWN"),
    ("😄", "LAUGH"),
    ("🎉", "HOORAY"),
    ("😕", "CONFUSED"),
    ("❤️", "HEART"),
    ("🚀", "ROCKET"),
    ("👀", "EYES"),
];

/// Emoji for a GitHub reaction content name; unknown contents render as
/// `:name:` so new reaction kinds degrade readably.
pub fn emoji_for_content(content: &str) -> String {
    PALETTE
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(content))
        .map(|(emoji, _)| (*emoji).to_string())
        .unwrap_or_else(|| format!(":{}:", content.to_lowercase()))
}

/// Group GraphQL reaction nodes (`[{content, user{login}}]`) into the thread
/// display shape `[{"emoji": "👍", "users": ["alice", ...]}]`.
pub fn github_reactions_json(nodes: Option<&Value>) -> Value {
    let mut grouped: Vec<(String, Vec<Value>)> = Vec::new();
    for node in nodes.and_then(Value::as_array).into_iter().flatten() {
        let Some(content) = node.get("content").and_then(Value::as_str) else {
            continue;
        };
        let emoji = emoji_for_content(content);
        let user = node
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("");
        match grouped.iter_mut().find(|(existing, _)| *existing == emoji) {
            Some((_, users)) => users.push(json!(user)),
            None => grouped.push((emoji, vec![json!(user)])),
        }
    }
    Value::Array(
        grouped
            .into_iter()
            .map(|(emoji, users)| json!({ "emoji": emoji, "users": users }))
            .collect(),
    )
}

/// The `react` descriptor for a GitHub comment node.
pub fn github_target_json(host: Option<&str>, subject_id: &str) -> Value {
    json!({ "provider": "github", "host": host, "subject_id": subject_id })
}

/// Where a posted reaction goes. Parsed from a message's `react` descriptor.
#[derive(Debug, Clone, PartialEq)]
pub enum ReactTarget {
    Github {
        host: Option<String>,
        subject_id: String,
    },
}

/// Parse a thread message's `react` descriptor, if it has a usable one.
pub fn parse_target(message: &Value) -> Option<ReactTarget> {
    let react = message.get("react")?;
    match react.get("provider").and_then(Value::as_str)? {
        "github" => Some(ReactTarget::Github {
            host: react.get("host").and_then(Value::as_str).map(String::from),
            subject_id: react.get("subject_id").and_then(Value::as_str)?.to_string(),
        }),
        _ => None,
    }
}

const ADD_REACTION: &str = "mutation($subject:ID!,$content:ReactionContent!){\
addReaction(input:{subjectId:$subject,content:$content}){reaction{content}}}";

/// Post a reaction. `content` is the palette content name (e.g. THUMBS_UP).
pub fn post(target: &ReactTarget, content: &str) -> Result<()> {
    match target {
        ReactTarget::Github { host, subject_id } => {
            let mut command = gh_command(host.as_deref());
            command
                .args(["api", "graphql", "-f", &format!("query={ADD_REACTION}")])
                .args(["-f", &format!("subject={subject_id}")])
                .args(["-f", &format!("content={content}")]);
            run_json(command, Duration::from_secs(15), false)
                .map_err(|err| EphorError::Command(format!("reaction failed: {err}")))?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_names_map_to_emoji() {
        assert_eq!(emoji_for_content("THUMBS_UP"), "👍");
        assert_eq!(emoji_for_content("rocket"), "🚀");
        assert_eq!(emoji_for_content("SOMETHING_NEW"), ":something_new:");
    }

    #[test]
    fn github_nodes_group_by_emoji() {
        let nodes = json!([
            { "content": "THUMBS_UP", "user": { "login": "alice" } },
            { "content": "THUMBS_UP", "user": { "login": "bob" } },
            { "content": "ROCKET", "user": { "login": "carol" } },
        ]);
        let grouped = github_reactions_json(Some(&nodes));
        assert_eq!(
            grouped,
            json!([
                { "emoji": "👍", "users": ["alice", "bob"] },
                { "emoji": "🚀", "users": ["carol"] },
            ])
        );
    }

    #[test]
    fn parse_target_reads_github_descriptor() {
        let message = json!({
            "author": "a",
            "react": { "provider": "github", "host": "github.example.com", "subject_id": "MDEy" },
        });
        assert_eq!(
            parse_target(&message),
            Some(ReactTarget::Github {
                host: Some("github.example.com".to_string()),
                subject_id: "MDEy".to_string(),
            })
        );
        assert_eq!(parse_target(&json!({ "author": "a" })), None);
        assert_eq!(
            parse_target(&json!({ "react": { "provider": "gitlab" } })),
            None
        );
    }
}
