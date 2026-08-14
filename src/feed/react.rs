//! Reactions on thread messages: the emoji palette every source's reactions
//! are shown in, and posting one back through the source that owns the
//! message.
//!
//! A message that supports posting carries a `react` descriptor in its thread
//! JSON. Whose descriptor it is, and how a reaction is posted to it, belongs
//! to the source that wrote it — either one of ephor's own providers, which
//! carries the write out itself, or the forge the descriptor came back from
//! (§REQ-001-boundary.5). A forge that does not declare the `reactions`
//! capability omits it; its reactions are display-only — which is also what
//! tells the reader's screen not to offer the key (§FS-004-quick-actions.2).

use serde_json::Value;

use crate::error::{EphorError, Result};
use crate::feed::config::Defaults;
use crate::feed::providers::{self, forge_call, NativeWrite};

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

/// Where a posted reaction goes. Parsed from a message's `react` descriptor.
#[derive(Debug, Clone, PartialEq)]
pub enum ReactTarget {
    /// A source ephor implements itself, which posts the reaction directly.
    Native(NativeWrite),
    /// Any source reached through the forge interface: the descriptor is that
    /// implementation's own and goes back to it verbatim
    /// (§FS-001-forge-interface.1). Without this arm the `react` subcommand
    /// every out-of-process forge may answer would be unreachable.
    Forge { source: String, target: Value },
}

/// Parse a thread message's `react` descriptor, if it has a usable one.
/// `source` is the item's source, which is who to hand a descriptor ephor does
/// not recognize back to.
pub fn parse_target(message: &Value, source: &str) -> Option<ReactTarget> {
    let react = message.get("react").filter(|react| !react.is_null())?;
    if providers::claims_write(react) {
        return providers::native_write(react).map(ReactTarget::Native);
    }
    Some(ReactTarget::Forge {
        source: source.to_string(),
        target: react.clone(),
    })
}

/// Post a reaction. `content` is the palette content name (e.g. THUMBS_UP),
/// `emoji` the same reaction as the interface spells it — a forge is asked in
/// the vocabulary of §FS-001-forge-interface, not in GitHub's.
pub fn post(
    target: &ReactTarget,
    content: &str,
    emoji: &str,
    blocks: &[Value],
    project: &str,
    defaults: &Defaults,
) -> Result<()> {
    match target {
        ReactTarget::Native(write) => providers::post_reaction(write, content),
        ReactTarget::Forge { source, target } => {
            let (forge, request) = forge_call(blocks, source, project, defaults)
                .map_err(|err| EphorError::Command(err.to_string()))?;
            forge
                .react(&request, target, emoji)
                .map_err(|err| EphorError::Command(err.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_names_map_to_emoji() {
        assert_eq!(emoji_for_content("THUMBS_UP"), "👍");
        assert_eq!(emoji_for_content("rocket"), "🚀");
        assert_eq!(emoji_for_content("SOMETHING_NEW"), ":something_new:");
    }

    /// A descriptor one of ephor's own providers wrote is carried out by that
    /// provider, and the engine holds it without knowing whose it is.
    #[test]
    fn parse_target_reads_a_native_descriptor() {
        let descriptor = providers::github::target_json(Some("github.example.com"), "MDEy");
        let write = providers::native_write(&descriptor).expect("a usable descriptor");
        let message = json!({ "author": "a", "react": descriptor });
        assert_eq!(
            parse_target(&message, "github-prs"),
            Some(ReactTarget::Native(write))
        );
        assert_eq!(parse_target(&json!({ "author": "a" }), "github-prs"), None);
        assert_eq!(parse_target(&json!({ "react": null }), "forge"), None);
    }

    /// A descriptor ephor does not recognize belongs to the forge that wrote
    /// it and goes back there verbatim — the arm that makes the `react`
    /// subcommand of an out-of-process implementation reachable at all.
    #[test]
    fn an_unrecognized_descriptor_goes_back_to_its_forge() {
        let message = json!({ "react": { "kind": "comment", "id": "c-7" } });
        assert_eq!(
            parse_target(&message, "forge"),
            Some(ReactTarget::Forge {
                source: "forge".to_string(),
                target: json!({ "kind": "comment", "id": "c-7" }),
            })
        );
    }

    /// A message with no descriptor offers no key at all
    /// (§FS-004-quick-actions.2) — the case every read-only forge is in.
    #[test]
    fn no_descriptor_is_no_target() {
        assert_eq!(
            parse_target(&json!({ "author": "Build Bot", "text": "…" }), "forge"),
            None
        );
    }
}
