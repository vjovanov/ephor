//! The nouns of the watch as data (§AR-006-matters).
//!
//! The unit is the **matter**: the subject under discussion or observation
//! (§FS-007-matters). One subject is one row however many sources reported it,
//! its conversation arrives as discussions of messages in channels, and
//! everything about it that is not conversation arrives as events. These types
//! are the core layer (§AR-001-layers.1): no source, seam, or surface adds
//! fields of its own — what a provider knows beyond the model rides in `raw`
//! and comes back out in `EPHOR_RAW` (§FS-005-dispatch.8).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::feed::model::{Item, ItemKind, ItemRole};

/// A matter's identity: the subject key its source stated
/// (§FS-007-matters.1). Never guessed from resemblance — two pull requests
/// may share a title, and a subject whose identity cannot be established is
/// left alone (§FS-003-feed-categories.5).
///
/// The grammar is `<scheme>:<the source's own id>`: `gh:acme/widget#42`,
/// `ticket:GR-73955`, a store's own id, or `topic:<digest>` for a
/// conversation that matched a project but no known subject.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SubjectKey(String);

/// The scheme a synthesized topic carries.
pub const TOPIC: &str = "topic";

impl SubjectKey {
    /// A key exactly as a source stated it.
    pub fn stated(key: impl Into<String>) -> SubjectKey {
        SubjectKey(key.into())
    }

    /// The subject a source names within its own namespace —
    /// `github-prs:acme/widget#42`.
    pub fn of_source(source: &str, id: &str) -> SubjectKey {
        if id.starts_with(&format!("{source}:")) {
            return SubjectKey(id.to_string());
        }
        SubjectKey(format!("{source}:{id}"))
    }

    /// A conversation that matched a project but no known subject
    /// (§FS-007-matters.1). The digest is of the words, so the same
    /// conversation seen twice is one topic rather than two.
    pub fn topic(text: &str) -> SubjectKey {
        SubjectKey(format!("{TOPIC}:{}", digest(text)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// What kind of name this is: everything before the first `:`.
    pub fn scheme(&self) -> &str {
        self.0
            .split_once(':')
            .map(|(scheme, _)| scheme)
            .unwrap_or("")
    }

    /// Whether this identity was synthesized rather than stated.
    pub fn is_topic(&self) -> bool {
        self.scheme() == TOPIC
    }
}

impl std::fmt::Display for SubjectKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Where a matter belongs. Ambiguity is not resolved by order: an item two
/// projects claim with equal strength goes to the unattributed bucket carrying
/// its candidates, because a guess that lands wrong amends someone's matter
/// silently (§FS-008-attribution.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// Placed on a project, and on a branch of it where one is known.
    On {
        project: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    /// Nothing could be placed, and these are the projects that claimed it.
    Unattributed {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        candidates: Vec<String>,
    },
}

impl Placement {
    pub fn on(project: impl Into<String>) -> Placement {
        Placement::On {
            project: project.into(),
            branch: None,
        }
    }

    /// The project this is placed on, or None while it is unattributed.
    pub fn project(&self) -> Option<&str> {
        match self {
            Placement::On { project, .. } => Some(project),
            Placement::Unattributed { .. } => None,
        }
    }

    pub fn branch(&self) -> Option<&str> {
        match self {
            Placement::On { branch, .. } => branch.as_deref(),
            Placement::Unattributed { .. } => None,
        }
    }
}

/// What a channel can carry (§FS-007-matters.4). What the reader may do about
/// a message is offered only where the channel declared it — an undeclared
/// capability narrows the offer by the degrade rule of §REQ-001-boundary.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    React,
    Tick,
    Reply,
}

/// The venue a discussion lives in: review threads, an issue's comments, a
/// mail thread, a chat thread (§FS-007-matters.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<Capability>,
}

impl Channel {
    pub fn new(id: impl Into<String>) -> Channel {
        Channel {
            id: id.into(),
            capabilities: Vec::new(),
        }
    }

    pub fn can(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
}

/// A task the venue tracks on a message (§FS-003-feed-categories.4). Task
/// state is the forge's own record of whether the thing was done, which is
/// exactly the question a thread asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageTask {
    pub resolved: bool,
    /// The source's own descriptor, handed back verbatim when the person ticks
    /// the box (§FS-004-quick-actions.5).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub target: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<DateTime<Utc>>,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<Reaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<MessageTask>,
    /// What the source needs handed back to act on this message — a reaction
    /// target, a resolve descriptor. Opaque above the source that wrote it.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub react: Value,
}

/// Ordered messages within one channel (§FS-007-matters.3). Whether a
/// discussion awaits the reader is decided per discussion, by the calculus of
/// §FS-003-feed-categories.4, identically in every channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discussion {
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_response: bool,
    /// Rendered over the thread where a matter has several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Discussion {
    pub fn last_activity(&self) -> Option<DateTime<Utc>> {
        self.messages
            .iter()
            .filter_map(|message| message.time)
            .max()
    }

    /// How many of this discussion's tasks are still open — part of the
    /// fingerprint, because a ticked box is a change the reader made.
    pub fn open_tasks(&self) -> usize {
        self.messages
            .iter()
            .filter(|message| message.task.as_ref().is_some_and(|task| !task.resolved))
            .count()
    }
}

/// Everything about a matter that is not conversation (§FS-007-matters.5):
/// the gate's counts changed, the state closed, a check finished, a ticket
/// resolved. Events fold into the matter's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub payload: Value,
}

/// The event kind a gate observation carries.
pub const GATE: &str = "gate";

/// A digest of what a reader would notice: the state, each discussion's
/// (last activity, message count, task states), and the event tail
/// (§AR-006-matters.2). Comparing fingerprints is how sync finds moved
/// matters, and the component that differs is how the row names its reason
/// for resurfacing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    /// Per discussion, keyed by channel: what the reader would notice about it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub discussions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub events: String,
}

/// Which part of a matter moved. The row says this rather than only that
/// something changed, because a row that reappears without a reason sends the
/// reader to re-read everything (§FS-007-matters.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    State,
    Discussions,
    Events,
}

impl Moved {
    pub fn name(self) -> &'static str {
        match self {
            Moved::State => "state",
            Moved::Discussions => "the conversation",
            Moved::Events => "events",
        }
    }
}

impl Fingerprint {
    /// What differs from an earlier print, in the order a reader cares:
    /// state first, then the conversation, then the rest.
    pub fn moved_since(&self, previous: &Fingerprint) -> Vec<Moved> {
        let mut moved = Vec::new();
        if self.state != previous.state {
            moved.push(Moved::State);
        }
        if self.discussions != previous.discussions {
            moved.push(Moved::Discussions);
        }
        if self.events != previous.events {
            moved.push(Moved::Events);
        }
        moved
    }
}

/// The subject under discussion or observation — the feed's row, and the unit
/// of attribution, state, fingerprinting, and dispatch (§FS-007-matters).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Matter {
    pub key: SubjectKey,
    pub kind: ItemKind,
    pub placement: Placement,
    /// The source that reported it. Where several did, the survivor's
    /// (§FS-003-feed-categories.5).
    pub source: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ItemRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub needs_response: bool,
    pub updated_at: DateTime<Utc>,
    /// Referenced subject keys — matters that are related rather than the same
    /// (§FS-007-matters.2). Merging what is one thing and linking what is
    /// related is the difference between a readable pile and a lossy one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SubjectKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discussions: Vec<Discussion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    #[serde(default)]
    pub fingerprint: Fingerprint,
    /// What the source knew beyond the model, carried whole and handed back in
    /// `EPHOR_RAW` (§AR-006-matters lead).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub raw: Value,
}

impl Matter {
    /// The matter one source's report is about. Providers still report the
    /// flat shape; this is where a report becomes the subject it is about.
    pub fn of_item(item: &Item) -> Matter {
        let mut matter = Matter {
            key: SubjectKey::stated(&item.id),
            kind: item.kind,
            placement: Placement::On {
                project: item.project.clone(),
                branch: item
                    .raw
                    .get("branch")
                    .and_then(Value::as_str)
                    .filter(|branch| !branch.is_empty())
                    .map(String::from),
            },
            source: item.source.clone(),
            title: item.title.clone(),
            role: item.role,
            url: item.url.clone(),
            state: item.state.clone(),
            needs_response: item.needs_response,
            updated_at: item.updated_at,
            links: Vec::new(),
            discussions: discussions_of(item),
            events: events_of(item),
            fingerprint: Fingerprint::default(),
            raw: item.raw.clone(),
        };
        matter.fingerprint = matter.print();
        matter
    }

    /// The flat report this matter reads as, for surfaces that have not yet
    /// been ported to the model. A rendering, never a second truth: nothing is
    /// stored in this shape (§AR-006-matters.3).
    pub fn as_item(&self) -> Item {
        Item {
            id: self.key.as_str().to_string(),
            project: self.placement.project().unwrap_or_default().to_string(),
            source: self.source.clone(),
            kind: self.kind,
            role: self.role,
            title: self.title.clone(),
            url: self.url.clone(),
            state: self.state.clone(),
            needs_response: self.needs_response,
            updated_at: self.updated_at,
            raw: self.raw.clone(),
        }
    }

    /// Recompute what a reader would notice about this matter
    /// (§AR-006-matters.2).
    pub fn print(&self) -> Fingerprint {
        Fingerprint {
            state: self.state.clone().unwrap_or_default(),
            discussions: self
                .discussions
                .iter()
                .map(|discussion| {
                    let last = discussion
                        .last_activity()
                        .map(|time| time.to_rfc3339())
                        .unwrap_or_default();
                    (
                        discussion.channel.id.clone(),
                        format!(
                            "{last}/{}/{}",
                            discussion.messages.len(),
                            discussion.open_tasks()
                        ),
                    )
                })
                .collect(),
            // The tail rather than the whole history: what moved is what
            // arrived, and an event that fell out of the tail is not news.
            // Nothing observed prints as nothing, rather than as the digest
            // of an empty string.
            events: if self.events.is_empty() {
                String::new()
            } else {
                digest(
                    &self
                        .events
                        .iter()
                        .rev()
                        .take(EVENT_TAIL)
                        .map(|event| format!("{}:{}", event.kind, event.payload))
                        .collect::<Vec<_>>()
                        .join("|"),
                )
            },
        }
    }

    /// Whether this matter awaits its reader: any of its discussions does, or
    /// the source said so (§FS-007-matters.3).
    pub fn awaits(&self) -> bool {
        self.needs_response
            || self
                .discussions
                .iter()
                .any(|discussion| discussion.needs_response)
    }
}

/// How many events the fingerprint remembers.
const EVENT_TAIL: usize = 8;

/// The discussions a provider's report carries. Providers still write them as
/// `raw.threads`; reading them here is what makes them the model's
/// (§FS-007-matters.3) — and dissolving the sources that mint their own rows
/// is the next step, not this one.
fn discussions_of(item: &Item) -> Vec<Discussion> {
    let Some(threads) = item.raw.get("threads").and_then(Value::as_array) else {
        return Vec::new();
    };
    threads
        .iter()
        .enumerate()
        .filter_map(|(index, thread)| {
            let messages: Vec<Message> = thread
                .get("messages")
                .and_then(Value::as_array)
                .map(|messages| messages.iter().map(message_of).collect())
                .unwrap_or_default();
            if messages.is_empty() {
                return None;
            }
            Some(Discussion {
                channel: Channel::new(format!("{}#{index}", item.source)),
                needs_response: false,
                label: thread
                    .get("label")
                    .and_then(Value::as_str)
                    .map(String::from),
                messages,
            })
        })
        .collect()
}

fn message_of(message: &Value) -> Message {
    let string = |key: &str| {
        message
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Message {
        author: string("author"),
        time: message
            .get("when")
            .and_then(Value::as_str)
            .and_then(|when| DateTime::parse_from_rfc3339(when).ok())
            .map(|time| time.with_timezone(&Utc)),
        text: string("text"),
        mine: message
            .get("mine")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        reactions: message
            .get("reactions")
            .and_then(Value::as_array)
            .map(|reactions| {
                reactions
                    .iter()
                    .map(|reaction| Reaction {
                        emoji: reaction
                            .get("emoji")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        users: reaction
                            .get("users")
                            .and_then(Value::as_array)
                            .map(|users| {
                                users
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(String::from)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        mine: reaction
                            .get("mine")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        task: message
            .get("task")
            .filter(|task| !task.is_null())
            .map(|task| MessageTask {
                resolved: crate::forge::task_resolved(task),
                target: task.clone(),
            }),
        react: message.get("react").cloned().unwrap_or(Value::Null),
    }
}

/// The events a report carries. A gate is an observation of the matter, not a
/// row of its own (§FS-007-matters.5).
fn events_of(item: &Item) -> Vec<Event> {
    let mut events = Vec::new();
    if let Some(gate) = crate::feed::gate::Gate::of(item) {
        events.push(Event {
            kind: GATE.to_string(),
            time: Some(item.updated_at),
            payload: gate.to_value(),
        });
    }
    events
}

/// A short, stable digest. Change detection only: it says *that* something
/// differs, and the model says what.
fn digest(text: &str) -> String {
    // FNV-1a, 64-bit. Small, dependency-free, and good enough to notice a
    // conversation that grew by one message.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(raw: Value) -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind: ItemKind::Pr,
            role: Some(ItemRole::Author),
            title: "Retry window".to_string(),
            url: Some("https://github.com/acme/widget/pull/42".to_string()),
            state: Some("open".to_string()),
            needs_response: true,
            updated_at: "2026-08-01T10:00:00Z".parse().unwrap(),
            raw,
        }
    }

    #[test]
    fn a_key_carries_the_scheme_its_source_stated() {
        let stated = SubjectKey::stated("gh:acme/widget#42");
        assert_eq!(stated.scheme(), "gh");
        assert!(!stated.is_topic());
        assert_eq!(
            SubjectKey::of_source("github-prs", "acme/widget#42").as_str(),
            "github-prs:acme/widget#42"
        );
        // A source that already namespaced its id is not namespaced twice.
        assert_eq!(
            SubjectKey::of_source("github-prs", "github-prs:acme/widget#42").as_str(),
            "github-prs:acme/widget#42"
        );
    }

    #[test]
    fn a_topic_is_the_same_topic_when_the_words_are() {
        let one = SubjectKey::topic("the release is stuck");
        assert!(one.is_topic());
        assert_eq!(one, SubjectKey::topic("the release is stuck"));
        assert_ne!(one, SubjectKey::topic("the release shipped"));
    }

    #[test]
    fn a_report_becomes_the_matter_it_is_about() {
        let matter = Matter::of_item(&item(json!({
            "branch": "you/ABC-42-retry-window",
            "threads": [{"messages": [
                {"author": "ada", "text": "why?", "when": "2026-08-01T09:00:00Z"},
                {"author": "you", "text": "because", "when": "2026-08-01T10:00:00Z", "mine": true}
            ]}],
            "gate": {"repos": [{"repo": "acme/widget", "passed": 1, "failed": 1, "running": 0}]}
        })));
        assert_eq!(matter.key.as_str(), "github-prs:acme/widget#42");
        assert_eq!(matter.placement.project(), Some("widget"));
        assert_eq!(matter.placement.branch(), Some("you/ABC-42-retry-window"));
        assert_eq!(matter.discussions.len(), 1);
        assert_eq!(matter.discussions[0].messages.len(), 2);
        assert!(matter.discussions[0].messages[1].mine);
        // The gate is an observation of the matter, not a row of its own.
        assert_eq!(matter.events.len(), 1);
        assert_eq!(matter.events[0].kind, GATE);
    }

    #[test]
    fn the_view_a_surface_reads_is_the_report_it_came_from() {
        let original = item(json!({"branch": "b", "threads": [{"messages": [
            {"author": "ada", "text": "why?"}
        ]}]}));
        let round_tripped = Matter::of_item(&original).as_item();
        // Everything the surfaces read survives the model, or the strangler
        // step would be a behaviour change (§FS-007-matters).
        assert_eq!(round_tripped.id, original.id);
        assert_eq!(round_tripped.project, original.project);
        assert_eq!(round_tripped.source, original.source);
        assert_eq!(round_tripped.kind, original.kind);
        assert_eq!(round_tripped.role, original.role);
        assert_eq!(round_tripped.title, original.title);
        assert_eq!(round_tripped.url, original.url);
        assert_eq!(round_tripped.state, original.state);
        assert_eq!(round_tripped.needs_response, original.needs_response);
        assert_eq!(round_tripped.updated_at, original.updated_at);
        assert_eq!(round_tripped.raw, original.raw);
    }

    #[test]
    fn a_task_state_is_read_where_the_venue_tracks_one() {
        let matter = Matter::of_item(&item(json!({"threads": [{"messages": [
            {"author": "bot", "text": "check this", "task": {"state": "open", "id": "t1"}},
            {"author": "bot", "text": "done", "task": {"state": "resolved", "id": "t2"}}
        ]}]})));
        let discussion = &matter.discussions[0];
        assert_eq!(discussion.open_tasks(), 1);
        // The source's own descriptor comes back verbatim for the tick.
        let task = discussion.messages[0].task.as_ref().unwrap();
        assert!(!task.resolved);
        assert_eq!(task.target.get("id").unwrap(), "t1");
    }

    #[test]
    fn a_fingerprint_names_what_moved_rather_than_that_something_did() {
        let before = Matter::of_item(&item(json!({"threads": [{"messages": [
            {"author": "ada", "text": "why?", "when": "2026-08-01T09:00:00Z"}
        ]}]})));

        // A reply arrives.
        let after = Matter::of_item(&item(json!({"threads": [{"messages": [
            {"author": "ada", "text": "why?", "when": "2026-08-01T09:00:00Z"},
            {"author": "bo", "text": "here", "when": "2026-08-01T11:00:00Z"}
        ]}]})));
        assert_eq!(
            after.fingerprint.moved_since(&before.fingerprint),
            vec![Moved::Discussions]
        );

        // The gate goes red.
        let gated = Matter::of_item(&item(json!({
            "threads": [{"messages": [{"author": "ada", "text": "why?", "when": "2026-08-01T09:00:00Z"}]}],
            "gate": {"repos": [{"repo": "acme/widget", "passed": 0, "failed": 3, "running": 0}]}
        })));
        assert_eq!(
            gated.fingerprint.moved_since(&before.fingerprint),
            vec![Moved::Events]
        );

        // Closing moves the state.
        let mut closed = before.clone();
        closed.state = Some("closed".to_string());
        closed.fingerprint = closed.print();
        assert_eq!(
            closed.fingerprint.moved_since(&before.fingerprint),
            vec![Moved::State]
        );

        // Nothing moved: nothing to say.
        assert!(before
            .fingerprint
            .moved_since(&before.fingerprint)
            .is_empty());
    }

    #[test]
    fn an_unattributed_matter_carries_the_projects_that_claimed_it() {
        let placement = Placement::Unattributed {
            candidates: vec!["widget".to_string(), "gadget".to_string()],
        };
        assert_eq!(placement.project(), None);
        assert_eq!(placement.branch(), None);
        // It survives the store, because the bucket is part of it
        // (§FS-008-attribution.4).
        let json = serde_json::to_string(&placement).unwrap();
        let back: Placement = serde_json::from_str(&json).unwrap();
        assert_eq!(back, placement);
    }

    #[test]
    fn a_matter_awaits_while_any_of_its_discussions_does() {
        let mut matter = Matter::of_item(&item(json!({"threads": [{"messages": [
            {"author": "ada", "text": "why?"}
        ]}]})));
        matter.needs_response = false;
        assert!(!matter.awaits());
        matter.discussions[0].needs_response = true;
        assert!(matter.awaits());
    }
}
