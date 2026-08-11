//! §FS-001-forge-interface: the one interface every forge and issue tracker is
//! reached through, in either transport.
//!
//! The types below are the interface. An in-process implementation ([`Forge`])
//! constructs them directly; an out-of-process one ([`external`]) is an
//! executable that prints their JSON. Both forms come from these definitions,
//! so the wire format is exactly the serde form of the Rust types and the two
//! cannot drift (§FS-001-forge-interface.2).
//!
//! Nothing here decides what the data *means* — no answered-citation rule, no
//! `needs_response`, no branch matching. That is policy and lives in [`policy`]
//! above the interface (§FS-001-forge-interface.3), which is what keeps an
//! implementation small enough to be a shell script.

pub mod external;
pub mod policy;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::feed::gate::Gate;
use crate::feed::model::ItemRole;
use crate::feed::provider::{ProviderContext, ProviderError};

/// What an implementation answers. Anything it does not declare is simply
/// absent from the feed rather than an error, so a forge that has no gate or
/// no issue tracker is a first-class implementation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    /// Answers [`Forge::pull_requests`].
    pub pull_requests: bool,
    /// Embeds `threads` in the pull requests it returns.
    pub conversation: bool,
    /// Embeds `gate` in the pull requests it returns.
    pub gate: bool,
    /// Answers [`Forge::issues`].
    pub issues: bool,
    /// Answers [`Forge::react`]; without it, messages are display-only.
    pub reactions: bool,
}

/// Which side of a pull request the user is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Author,
    Reviewer,
}

impl From<Role> for ItemRole {
    fn from(role: Role) -> Self {
        match role {
            Role::Author => ItemRole::Author,
            Role::Reviewer => ItemRole::Reviewer,
        }
    }
}

/// One message in a conversation. `react` carries whatever identity the
/// implementation needs to post a reaction back to this message; ephor treats
/// it as opaque and hands it back verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub author: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<Reaction>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub react: Value,
    /// Written by the user. Only the implementation knows how its forge
    /// identifies people — a login here, an email there — so it makes the call
    /// and policy stays identity-agnostic (§FS-001-forge-interface.3).
    #[serde(default)]
    pub mine: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    #[serde(default)]
    pub users: Vec<String>,
    /// The user is among `users`; see [`Message::mine`].
    #[serde(default)]
    pub mine: bool,
}

/// One conversation. Forges that have a single conversation per pull request
/// return one thread; forges with resolvable review threads return several.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    #[serde(default)]
    pub messages: Vec<Message>,
}

/// Review state as the forge reports it, lowercased and free-form: it is shown
/// to the user and matched against a small set of known values by policy, never
/// exhaustively matched, so a forge may report a state ephor has not seen.
pub type ReviewState = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    /// Stable within the forge — `<repo>/<number>` or `<owner>/<repo>#<number>`.
    /// Becomes the item id, so it must not depend on ordering.
    pub id: String,
    pub repo: String,
    pub number: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Head branch, which links the item to a registry branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ReviewState>,
    /// The user is named in this pull request and owes an answer unless the
    /// conversation shows they gave one. Deciding that is policy's job.
    #[serde(default)]
    pub cited: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<Thread>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<Gate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

/// Everything an implementation is told about the request. Out of process this
/// is the JSON written to the implementation's stdin: its own configuration
/// block verbatim, plus the context ephor holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// The provider's configuration block, exactly as written in status.json.
    /// ephor does not interpret it.
    pub config: Value,
    pub project: String,
    /// Ticket keys harvested from the registry's active branches.
    #[serde(default)]
    pub tickets: Vec<String>,
    /// The user's login on this forge, when ephor knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub timeout_seconds: u64,
}

impl Request {
    pub fn new(config: Value, ctx: &ProviderContext) -> Self {
        Request {
            config,
            project: ctx.project_id.clone(),
            tickets: ctx.tickets.clone(),
            user: ctx.github_user.clone(),
            timeout_seconds: ctx.timeout.as_secs(),
        }
    }
}

/// A forge or issue tracker. Implement this to add one in process; to add one
/// out of process, write an executable answering the same shapes and let
/// [`external::ExternalForge`] adapt it.
pub trait Forge: Send + Sync {
    fn name(&self) -> String;

    fn capabilities(&self) -> Capabilities;

    /// Cheap check that the implementation can run at all — its CLI is on
    /// PATH, its credentials exist.
    fn available(&self) -> bool {
        true
    }

    fn pull_requests(&self, _request: &Request) -> Result<Vec<PullRequest>, ProviderError> {
        Ok(Vec::new())
    }

    fn issues(&self, _request: &Request) -> Result<Vec<Issue>, ProviderError> {
        Ok(Vec::new())
    }

    /// Post a reaction, given a message's `react` value verbatim.
    fn react(
        &self,
        _request: &Request,
        _target: &Value,
        _emoji: &str,
    ) -> Result<(), ProviderError> {
        Err(ProviderError(format!(
            "{} does not support posting reactions",
            self.name()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The wire format is the serde form of the Rust types — this is the
    /// guarantee §FS-001-forge-interface.2 rests on, so it is asserted rather
    /// than assumed. A shell script writing this JSON and a Rust
    /// implementation constructing the value are interchangeable.
    #[test]
    fn a_shell_implementations_json_parses_into_the_rust_types() {
        let wire = json!({
            "id": "app/101",
            "repo": "app",
            "number": "101",
            "title": "Widen the retry window",
            "url": "https://forge.example/pr/101",
            "branch": "you/ABC-42-retry",
            "updated_at": "2026-07-30T12:00:00Z",
            "role": "author",
            "state": "needs_work",
            "cited": true,
            "threads": [{ "messages": [{
                "author": "Other Dev",
                "text": "Please widen it further.",
                "when": "2026-07-30T11:00:00Z",
                "reactions": [{ "emoji": "👍", "users": ["dev"], "mine": true }],
                "react": { "kind": "comment", "id": "c-7" },
                "mine": false
            }] }],
            "gate": { "repos": [{ "repo": "app", "passed": 5, "failed": 1, "running": 0 }] }
        });

        let pr: PullRequest = serde_json::from_value(wire.clone()).expect("parses");
        assert_eq!(pr.id, "app/101");
        assert_eq!(pr.role, Role::Author);
        assert!(pr.cited);
        assert_eq!(pr.threads[0].messages[0].reactions[0].emoji, "👍");
        assert_eq!(pr.gate.as_ref().unwrap().passed(), 5);
        // Round-trips: what Rust emits is what a script may send back.
        assert_eq!(serde_json::to_value(&pr).unwrap(), wire);
    }

    #[test]
    fn optional_fields_may_be_omitted_entirely() {
        let minimal = json!({
            "id": "app/1", "repo": "app", "number": "1", "title": "t",
            "updated_at": "2026-07-30T12:00:00Z", "role": "reviewer"
        });
        let pr: PullRequest = serde_json::from_value(minimal).expect("parses");
        assert_eq!(pr.role, Role::Reviewer);
        assert!(pr.url.is_none() && pr.threads.is_empty() && pr.gate.is_none() && !pr.cited);
    }

    #[test]
    fn capabilities_default_to_nothing_declared() {
        let none: Capabilities = serde_json::from_value(json!({})).unwrap();
        assert_eq!(none, Capabilities::default());
        let some: Capabilities =
            serde_json::from_value(json!({ "pull_requests": true, "gate": true })).unwrap();
        assert!(some.pull_requests && some.gate && !some.issues && !some.reactions);
    }
}
