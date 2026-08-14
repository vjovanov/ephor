//! Replies to a conversation: reading the target out of a thread's `reply`
//! descriptor, and sending one back through the provider that reported the
//! conversation.
//!
//! A channel that can carry a reply says so by putting a `reply` descriptor on
//! its thread, in the pattern a message uses for `react`
//! (§FS-007-matters.4). Whose descriptor it is, and how a reply is sent to it,
//! belongs to the source that wrote it (§REQ-001-boundary.5). A channel that
//! omits it is display-only: the surfaces offer no key, and a drafted answer
//! is what the reader copies (§FS-005-dispatch.13), which is a stated degrade
//! rather than a failure (§REQ-001-boundary.1).

use serde_json::Value;

use crate::error::{EphorError, Result};
use crate::feed::config::Defaults;
use crate::feed::providers::{self, forge_call, NativeWrite};

/// Where a posted reply goes. Parsed from a thread's `reply` descriptor.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplyTarget {
    /// A source ephor implements itself, which sends the reply directly.
    Native(NativeWrite),
    /// Any source reached through the forge interface: the descriptor is that
    /// implementation's own and goes back to it verbatim
    /// (§FS-001-forge-interface.1).
    Forge { source: String, target: Value },
}

/// Parse a thread's `reply` descriptor, if it has a usable one. `source` is
/// the item's source, which is who to hand a descriptor ephor does not
/// recognize back to.
pub fn parse_target(thread: &Value, source: &str) -> Option<ReplyTarget> {
    let reply = thread.get("reply").filter(|reply| !reply.is_null())?;
    if providers::claims_write(reply) {
        return providers::native_write(reply).map(ReplyTarget::Native);
    }
    Some(ReplyTarget::Forge {
        source: source.to_string(),
        target: reply.clone(),
    })
}

/// Send the reply. The text is the reader's — edited or as it was drafted —
/// and it goes out exactly as it stands (§FS-005-dispatch.13).
pub fn post(
    target: &ReplyTarget,
    text: &str,
    blocks: &[Value],
    project: &str,
    defaults: &Defaults,
) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        return Err(EphorError::Command("There is nothing to post".to_string()));
    }
    match target {
        ReplyTarget::Native(write) => providers::post_reply(write, text),
        ReplyTarget::Forge { source, target } => {
            let (forge, request) = forge_call(blocks, source, project, defaults)
                .map_err(|err| EphorError::Command(err.to_string()))?;
            forge
                .reply(&request, target, text)
                .map_err(|err| EphorError::Command(err.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A descriptor one of ephor's own providers wrote is carried out by that
    /// provider, and the engine holds it without knowing whose it is.
    #[test]
    fn parse_target_reads_a_native_descriptor() {
        let descriptor = providers::github::target_json(Some("github.example.com"), "PR_1");
        let write = providers::native_write(&descriptor).expect("a usable descriptor");
        let thread = json!({ "messages": [], "reply": descriptor });
        assert_eq!(
            parse_target(&thread, "github-prs"),
            Some(ReplyTarget::Native(write))
        );
    }

    /// A descriptor ephor does not recognize belongs to the forge that wrote
    /// it and goes back there verbatim (§FS-001-forge-interface.1).
    #[test]
    fn an_unrecognized_descriptor_goes_back_to_its_forge() {
        let thread = json!({ "reply": { "kind": "review-thread", "id": "t-9" } });
        assert_eq!(
            parse_target(&thread, "gdev"),
            Some(ReplyTarget::Forge {
                source: "gdev".to_string(),
                target: json!({ "kind": "review-thread", "id": "t-9" }),
            })
        );
    }

    /// A channel that declared nothing carries nothing: no target, and so no
    /// key on any surface (§FS-007-matters.4).
    #[test]
    fn a_channel_that_declares_no_reply_has_no_target() {
        assert_eq!(
            parse_target(&json!({ "messages": [{ "author": "ada" }] }), "gdev"),
            None
        );
        assert_eq!(parse_target(&json!({ "reply": null }), "gdev"), None);
    }

    /// Nothing is posted for an empty reply — a proposal edited down to
    /// whitespace is a proposal withdrawn, not a blank comment.
    #[test]
    fn an_empty_reply_is_refused_before_any_provider_is_asked() {
        let target = ReplyTarget::Forge {
            source: "gdev".to_string(),
            target: json!({ "id": "t-9" }),
        };
        let err = post(&target, "  \n\n", &[], "widget", &Defaults::default()).unwrap_err();
        assert!(err.to_string().contains("nothing to post"), "{err}");
    }
}
