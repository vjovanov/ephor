//! Replies to a conversation: reading the target out of a thread's `reply`
//! descriptor, and sending one back through the provider that reported the
//! conversation.
//!
//! A channel that can carry a reply says so by putting a `reply` descriptor on
//! its thread, in the pattern a message uses for `react`
//! (§FS-007-matters.4) — e.g. `{"provider": "github", "host": null,
//! "subject_id": "<node id>"}`. A channel that omits it is display-only: the
//! surfaces offer no key, and a drafted answer is what the reader copies
//! (§FS-005-dispatch.13), which is a stated degrade rather than a failure
//! (§REQ-001-boundary.1).

use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{EphorError, Result};
use crate::feed::config::Defaults;
use crate::feed::provider::run_json;
use crate::feed::providers::{forge_call, gh_command};

/// Where a posted reply goes. Parsed from a thread's `reply` descriptor.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplyTarget {
    Github {
        host: Option<String>,
        /// The node the comment is added to — the pull request or issue, not
        /// one of its comments.
        subject_id: String,
    },
    /// Any source reached through the forge interface: the descriptor is that
    /// implementation's own and goes back to it verbatim
    /// (§FS-001-forge-interface.1).
    Forge { source: String, target: Value },
}

/// The `reply` descriptor for a GitHub subject that takes comments.
pub fn github_target_json(host: Option<&str>, subject_id: &str) -> Value {
    json!({ "provider": "github", "host": host, "subject_id": subject_id })
}

/// Parse a thread's `reply` descriptor, if it has a usable one. `source` is
/// the item's source, which is who to hand a descriptor ephor does not
/// recognize back to.
pub fn parse_target(thread: &Value, source: &str) -> Option<ReplyTarget> {
    let reply = thread.get("reply").filter(|reply| !reply.is_null())?;
    match reply.get("provider").and_then(Value::as_str) {
        Some("github") => Some(ReplyTarget::Github {
            host: reply.get("host").and_then(Value::as_str).map(String::from),
            subject_id: reply.get("subject_id").and_then(Value::as_str)?.to_string(),
        }),
        _ => Some(ReplyTarget::Forge {
            source: source.to_string(),
            target: reply.clone(),
        }),
    }
}

const ADD_COMMENT: &str = "mutation($subject:ID!,$body:String!){\
addComment(input:{subjectId:$subject,body:$body}){clientMutationId}}";

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
        ReplyTarget::Github { host, subject_id } => {
            let mut command = gh_command(host.as_deref());
            command
                .args(["api", "graphql", "-f", &format!("query={ADD_COMMENT}")])
                .args(["-f", &format!("subject={subject_id}")])
                .args(["-f", &format!("body={text}")]);
            run_json(command, Duration::from_secs(30), false)
                .map_err(|err| EphorError::Command(format!("reply failed: {err}")))?;
            Ok(())
        }
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

    #[test]
    fn parse_target_reads_a_github_descriptor() {
        let thread = json!({
            "messages": [],
            "reply": { "provider": "github", "host": "github.example.com", "subject_id": "PR_1" },
        });
        assert_eq!(
            parse_target(&thread, "github-prs"),
            Some(ReplyTarget::Github {
                host: Some("github.example.com".to_string()),
                subject_id: "PR_1".to_string(),
            })
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
