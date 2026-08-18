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

use crate::feed::gate::{Failure, Gate, Scope};
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
    /// Embeds `review` in the pull requests it returns — the user's own
    /// verdict on a change they were asked to look at
    /// (§FS-001-forge-interface.1). Kept apart from `pull_requests` because it
    /// is a second question of the forge: the reviewer list says who was
    /// asked, this says who answered.
    pub review: bool,
    /// Answers [`Forge::failures`] — what went wrong under a red gate. Kept
    /// apart from `gate` because it is the expensive question: a gate is read
    /// for every pull request on every refresh, a failure list only when
    /// somebody asks (§FS-001-forge-interface.1).
    pub failures: bool,
    /// Answers [`Forge::restart`] — run a pull request's gate again
    /// (§FS-004-quick-actions.9). Kept apart from `gate` and from `failures`
    /// because it is the one capability here that *writes*, and what it spends
    /// is somebody else's machines: a forge that reports a gate it cannot
    /// re-run is an ordinary implementation, and ephor offers no key there
    /// rather than one that fails.
    pub restart: bool,
    /// Answers [`Forge::issues`].
    pub issues: bool,
    /// Answers [`Forge::notices`] — the forge's own list of what it decided to
    /// tell the user (§FS-001-forge-interface.1). The completeness capability:
    /// every other one returns what ephor knew to ask for, this one returns
    /// what the forge knew to say.
    pub notices: bool,
    /// Answers [`Forge::react`]; without it, messages are display-only.
    pub reactions: bool,
    /// Answers [`Forge::resolve_task`]; without it, the tasks a forge reports
    /// still render with their state, since an unticked box is worth seeing
    /// even where ephor cannot tick it (§FS-001-forge-interface.1).
    pub tasks: bool,
    /// Answers [`Forge::reply`]; without it, a conversation is read here and
    /// answered where it lives, and a drafted reply is material to copy rather
    /// than something to send (§FS-005-dispatch.13).
    pub replies: bool,
}

/// What a restart actually asked for (§FS-001-forge-interface.1).
///
/// A gate is minutes away from saying anything itself, so the answer has to
/// stand on its own until then: *done* alone cannot be told apart from a
/// restart that found nothing to run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Restarted {
    /// How many jobs the forge said it asked to run again. `Some(0)` is an
    /// answer — there was nothing not-green to restart — and not a failure.
    ///
    /// `None` is a different answer: the forge took the request and does not
    /// say how much it scheduled. That is an ordinary gate rather than a
    /// broken one — a whole-gate start is accepted and executed
    /// asynchronously, with the count knowable only from the gate itself
    /// minutes later — and it must not be reported as zero, which would read
    /// as a restart that found nothing to do.
    pub asked: Option<usize>,
    /// What it could not restart, one line each: an external status somebody
    /// else's system wrote, a run too old for the forge to re-run. Reported
    /// rather than swallowed, because a key that silently did three quarters
    /// of the job is worse than one that says which quarter it skipped.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
    /// The forge's own sentence about what it did, where it has one. Shown
    /// verbatim, for the same reason a gate's blockers are
    /// (§FS-001-forge-interface.1): it is the forge's vocabulary, and it is
    /// what a reader matches against the forge itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Restarted {
    /// The one line worth showing about a restart, in the reader's own terms.
    pub fn says(&self) -> String {
        match (self.asked, &self.note) {
            (Some(0), None) => "nothing here needed restarting".to_string(),
            (Some(count), None) => format!("asked {count} job(s) to run again"),
            (Some(count), Some(note)) => format!("asked {count} job(s) to run again — {note}"),
            // The forge does not count. Saying what it said beats inventing a
            // number, and beats a bare "done" that cannot be told from a
            // restart that did nothing.
            (None, Some(note)) => note.clone(),
            (None, None) => "the gate took the request; it does not say how much".to_string(),
        }
    }
}

/// Which side of a pull request or issue the user is on. Defaults to author:
/// an implementation that reports an item without saying is reporting the
/// user's own work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
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

/// Why a pull request is the user's (§FS-001-forge-interface.1). An
/// implementation reports every reason it found; which side of the review that
/// puts the user on, and whether any of them means an answer is owed, is
/// policy's (§FS-001-forge-interface.3).
///
/// The distinction that matters is between having *acted* — opened it, spoken
/// in it — and having been *asked*: a review request and an assignment leave no
/// trace in the conversation, so an implementation that reports only the first
/// kind reports the pull requests the user has already dealt with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// The user opened it.
    Authored,
    /// The user has spoken in it.
    InThread,
    /// The user is named in it.
    Mentioned,
    /// A review was asked of the user and they have not given it.
    ReviewRequested,
    /// It is assigned to the user.
    Assigned,
}

impl Reason {
    /// How the reason reads on a row, and what an item's state is composed
    /// from where the user is not the author.
    pub fn label(&self) -> &'static str {
        match self {
            Reason::Authored => "authored",
            Reason::InThread => "in-thread",
            Reason::Mentioned => "mentioned",
            Reason::ReviewRequested => "review-requested",
            Reason::Assigned => "assigned",
        }
    }
}

/// One message in a conversation. `react` carries whatever identity the
/// implementation needs to post a reaction back to this message; ephor treats
/// it as opaque and hands it back verbatim. `task` is the same bargain for a
/// message the forge tracks as a task, plus the one field ephor does read out
/// of it — `state` (§FS-001-forge-interface.1).
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
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub task: Value,
    /// Written by the user. Only the implementation knows how its forge
    /// identifies people — a login here, an email there — so it makes the call
    /// and policy stays identity-agnostic (§FS-001-forge-interface.3).
    #[serde(default)]
    pub mine: bool,
}

impl Message {
    /// A task still to be done — what makes a thread await the reader
    /// (§FS-003-feed-categories.4) and what the tick key acts on.
    pub fn open_task(&self) -> bool {
        !self.task.is_null() && !crate::matter::task_resolved(&self.task)
    }

    pub fn resolved_task(&self) -> bool {
        !self.task.is_null() && crate::matter::task_resolved(&self.task)
    }
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
    /// What the implementation needs handed back to send a reply here — the
    /// channel declaring it can carry one (§FS-007-matters.4). Opaque to
    /// ephor, like `react` on a message; absent means the conversation is
    /// display-only and a drafted answer is copy material
    /// (§FS-005-dispatch.13).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub reply: Value,
}

/// Review state as the forge reports it, lowercased and free-form: it is shown
/// to the user and matched against a small set of known values by policy, never
/// exhaustively matched, so a forge may report a state ephor has not seen.
pub type ReviewState = String;

/// The review the user themselves gave on a pull request
/// (§FS-001-forge-interface.1).
///
/// Closed where [`ReviewState`] is free-form, and deliberately: that one is the
/// forge's summary of everybody's review and is quoted, this one is the single
/// fact a reviewing row turns on, so each forge's spelling is mapped into these
/// three words rather than passed through. A verdict ephor has no word for — an
/// approval the forge dismissed, a review still in draft — is reported as no
/// review at all, which is what it means to the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Review {
    /// The user approved the change.
    Approved,
    /// The user asked for changes.
    ChangesRequested,
    /// The user reviewed it without deciding either way.
    Commented,
}

impl Review {
    /// How the verdict reads on a row, in the vocabulary the reasons use.
    pub fn label(&self) -> &'static str {
        match self {
            Review::Approved => "approved",
            Review::ChangesRequested => "changes-requested",
            Review::Commented => "commented",
        }
    }
}

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
    /// Which side of the review the user is on. An implementation that reports
    /// `reasons` need not set this — policy derives it — and one that reports
    /// neither is reporting the user's own work, as the default says.
    #[serde(default)]
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ReviewState>,
    /// Every reason this pull request is the user's. Empty from an
    /// implementation that has no notion of them, which is why `role` and
    /// `cited` remain the fallback rather than being replaced by it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<Reason>,
    /// The user is named in this pull request and owes an answer unless the
    /// conversation shows they gave one. Deciding that is policy's job.
    #[serde(default)]
    pub cited: bool,
    /// The user's own review, from an implementation declaring `review`
    /// (§FS-001-forge-interface.1). `None` is "they have not reviewed it",
    /// which is also all an implementation that does not declare the
    /// capability ever says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<Thread>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<Gate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Stable within the forge — a tracker key (`ABC-42`) or
    /// `<owner>/<repo>#<number>`. Becomes the item id.
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub updated_at: DateTime<Utc>,
    /// Whether the user opened this issue or merely takes part in it
    /// (§FS-001-forge-interface.1). An implementation with no notion of the
    /// difference omits it and everything it reports counts as the user's own.
    #[serde(default)]
    pub role: Role,
    /// Whether anyone has taken this issue, where the forge tracks that
    /// (§FS-001-forge-interface.1). `None` is an implementation with no notion
    /// of assignment, and it is kept apart from `Some(false)` on purpose:
    /// "nobody has picked this up" is a claim about the world, and one nobody
    /// made must never be counted as unclaimed work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

/// What kind of thing a notice is about. `Other` is not a failure to
/// recognise it — a forge notifies about releases, security advisories, and
/// invitations too, and those are worth telling the reader about precisely
/// because ephor has no capability that would ever have asked for them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectKind {
    PullRequest,
    Issue,
    #[default]
    Other,
}

/// One thing the forge decided to tell the user about
/// (§FS-001-forge-interface.1).
///
/// `reason` is the forge's own word for why, lowercased and free-form: it is
/// shown to the reader and matched against a small set of known values by
/// policy, never exhaustively matched, so a forge may give a reason ephor has
/// never seen and the notice still arrives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notice {
    /// Stable within the forge, and stable across refreshes: it becomes the
    /// item id, which is the unread-tracking key.
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The forge's word for why it is telling the user — `mention`,
    /// `team_mention`, `review_requested`, `assign`, `ci_activity`, …
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub subject: SubjectKind,
    /// The repository the subject lives in, where the forge has that notion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The subject's number within its repository — what makes this notice the
    /// same work as a pull request or issue another capability reported
    /// (§FS-003-feed-categories.5). Absent where the subject has no number, and
    /// then the notice stands on its own rather than being guessed at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    pub updated_at: DateTime<Utc>,
    /// The forge considers the user to have read it. Its own record, kept
    /// apart from ephor's unread tracking, which answers a different question:
    /// whether the reader has seen the row.
    #[serde(default)]
    pub read: bool,
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

    /// What this implementation answers. Fallible: for an out-of-process
    /// forge this is a process launch that can fail for every reason a fetch
    /// can, and reporting "declared nothing" in place of the real error turns
    /// an unreachable host into what reads like a malformed extension.
    fn capabilities(&self) -> Result<Capabilities, ProviderError>;

    /// Cheap check that the implementation can run at all — its CLI is on
    /// PATH, its credentials exist.
    fn available(&self) -> bool {
        true
    }

    /// What `available` looked for and did not find, named precisely enough
    /// to act on.
    fn unavailable_reason(&self) -> Option<String> {
        None
    }

    fn pull_requests(&self, _request: &Request) -> Result<Vec<PullRequest>, ProviderError> {
        Ok(Vec::new())
    }

    fn issues(&self, _request: &Request) -> Result<Vec<Issue>, ProviderError> {
        Ok(Vec::new())
    }

    /// Everything the forge says is directed at the user, whatever it is about
    /// (§FS-001-forge-interface.1). Answered exhaustively or not at all: a
    /// truncated notice list is the one answer this capability must never give,
    /// since its whole value is that the reader can believe it
    /// (§FS-001-forge-interface.6).
    fn notices(&self, _request: &Request) -> Result<Vec<Notice>, ProviderError> {
        Ok(Vec::new())
    }

    /// What failed under one pull request's red gate. Asked only when a reader
    /// asks, so it may be as slow as the forge needs it to be, and it may
    /// return nothing — a gate blocked on an approval has no failed job to
    /// show and that is an answer, not an error.
    fn failures(
        &self,
        _request: &Request,
        _repo: &str,
        _number: &str,
    ) -> Result<Vec<Failure>, ProviderError> {
        Err(ProviderError(format!(
            "{} does not report what failed",
            self.name()
        )))
    }

    /// Run a pull request's gate again, at the scope asked for
    /// (§FS-004-quick-actions.9). The one write here that spends somebody
    /// else's machines, so the scope is passed through rather than
    /// interpreted: an implementation that widened *what failed* into
    /// *everything* would be answering a question nobody put.
    fn restart(
        &self,
        _request: &Request,
        _repo: &str,
        _number: &str,
        _scope: Scope,
    ) -> Result<Restarted, ProviderError> {
        Err(ProviderError(format!(
            "{} does not restart a gate",
            self.name()
        )))
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

    /// Tick a task, given a message's `task` value verbatim
    /// (§FS-004-quick-actions.5). The descriptor is the implementation's own,
    /// so it carries whatever that forge needs to name the task — ephor read
    /// only `state` out of it and hands the rest back untouched.
    fn resolve_task(&self, _request: &Request, _target: &Value) -> Result<(), ProviderError> {
        Err(ProviderError(format!(
            "{} does not support ticking tasks",
            self.name()
        )))
    }

    /// Send a reply to a conversation, given the thread's `reply` value
    /// verbatim (§FS-007-matters.4). The one write that carries the reader's
    /// own words, and the last step of an answer a run drafted
    /// (§FS-005-dispatch.13).
    fn reply(&self, _request: &Request, _target: &Value, _text: &str) -> Result<(), ProviderError> {
        Err(ProviderError(format!(
            "{} does not support posting replies",
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
            "role": "reviewer",
            "state": "needs_work",
            "cited": true,
            "review": "changes-requested",
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
        assert_eq!(pr.role, Role::Reviewer);
        assert!(pr.cited);
        assert_eq!(pr.review, Some(Review::ChangesRequested));
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
        // No review reported is "they have not reviewed it", never an error.
        assert_eq!(pr.review, None);
    }

    #[test]
    fn capabilities_default_to_nothing_declared() {
        let none: Capabilities = serde_json::from_value(json!({})).unwrap();
        assert_eq!(none, Capabilities::default());
        let some: Capabilities =
            serde_json::from_value(json!({ "pull_requests": true, "gate": true, "review": true }))
                .unwrap();
        assert!(some.pull_requests && some.gate && some.review);
        assert!(!some.issues && !some.reactions);
    }
}
