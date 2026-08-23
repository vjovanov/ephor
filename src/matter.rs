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

use crate::attribution::Evidence;
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
        /// How firmly the project claimed it (§FS-008-attribution.3). Absent
        /// where the matter was never placed by the engine at all — a source
        /// configured under one project reports about that project, and there
        /// is no evidence to weigh.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        how: Option<crate::attribution::Strength>,
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
            how: None,
        }
    }

    /// Placed by the attribution engine, carrying how firmly
    /// (§FS-008-attribution.3).
    pub fn claimed(project: impl Into<String>, how: crate::attribution::Strength) -> Placement {
        Placement::On {
            project: project.into(),
            branch: None,
            how: Some(how),
        }
    }

    /// Whether nothing firmer than the project's own name placed this. Such a
    /// matter may start a row of its own and may never be folded onto a
    /// subject some source actually named (§FS-008-attribution.3).
    pub fn by_resemblance(&self) -> bool {
        matches!(
            self,
            Placement::On {
                how: Some(crate::attribution::Strength::Resemblance),
                ..
            }
        )
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

/// A task is done or it is not, whatever the forge calls the states it uses
/// (§FS-001-forge-interface.1): anything but `resolved` is work left, so a
/// spelling ephor has not seen reads as unfinished rather than as finished.
pub fn task_resolved(task: &Value) -> bool {
    task.get("state")
        .and_then(Value::as_str)
        .is_some_and(|state| state.eq_ignore_ascii_case("resolved"))
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
    /// How the row says it, as a sentence fragment after "⟳ ".
    pub fn name(self) -> &'static str {
        match self {
            Moved::State => "the state moved",
            Moved::Discussions => "the conversation moved",
            Moved::Events => "the gate moved",
        }
    }
}

impl Fingerprint {
    /// Why a matter is back in front of the reader, in the words the row
    /// shows: "⟳ the conversation moved". A row that reappears without a
    /// reason sends the reader to re-read everything, which is the sweep this
    /// tool exists to retire (§FS-007-matters.5).
    pub fn resurfacing(&self, previous: &Fingerprint) -> Option<String> {
        let moved = self.moved_since(previous);
        if moved.is_empty() {
            return None;
        }
        Some(format!(
            "⟳ {}",
            moved
                .iter()
                .map(|part| part.name())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

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

/// What a report carries about where it belongs, extracted once at fetch
/// normalization (§AR-003-attribution.1). It is data on the matter, so a
/// misplacement can be debugged by looking rather than by rereading a source.
pub fn evidence_of(item: &Item) -> Evidence {
    let mut words = vec![item.title.clone()];
    let mut addresses = Vec::new();
    if let Some(threads) = item.raw.get("threads").and_then(Value::as_array) {
        for thread in threads {
            for message in thread
                .get("messages")
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
            {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    words.push(text.to_string());
                }
                if let Some(author) = message.get("author").and_then(Value::as_str) {
                    if !author.is_empty() && !addresses.contains(&author.to_string()) {
                        addresses.push(author.to_string());
                    }
                }
            }
        }
    }
    let spoken = words.join("\n");
    Evidence {
        venue: Some(SubjectKey::stated(&item.id)),
        repo: item.repo(),
        tickets: crate::ticket_ids::tickets_in(&spoken),
        repos: repos_in(&spoken)
            .into_iter()
            .chain(item.url.iter().flat_map(|url| repos_in(url)))
            .collect(),
        addresses,
        words: spoken,
    }
}

/// The `owner/name` repositories a piece of text names. Deliberately plain:
/// two slash-separated words that look like a repository, which is what a url
/// and a sentence both spell the same way.
fn repos_in(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for word in text.split(|character: char| character.is_whitespace()) {
        let cleaned = word.trim_matches(|character: char| !character.is_alphanumeric());
        // A url says it after the host; a sentence says it on its own. Which
        // host is not this layer's business — every forge spells a repository
        // the same way once the host is off the front (§REQ-001-boundary.5).
        let candidate = cleaned.split_once("://").map_or(cleaned, |(_, rest)| rest);
        let candidate = match candidate.split_once('/') {
            // A host is the leading segment with a dot in it, and it is only a
            // host when a whole repository still follows it.
            Some((host, rest)) if host.contains('.') && rest.contains('/') => rest,
            _ => candidate,
        };
        let parts: Vec<&str> = candidate.split('/').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            continue;
        }
        let repo = format!("{}/{}", parts[0], parts[1]);
        if repo.chars().all(|character| {
            character.is_alphanumeric()
                || character == '/'
                || character == '-'
                || character == '_'
                || character == '.'
        }) && !found.contains(&repo)
        {
            found.push(repo);
        }
    }
    found
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
                // Nothing was weighed: a report arrives under the project its
                // source was configured for, and the engine only speaks for
                // the shared sources it places (§DA-002-fetch-attribution-split).
                how: None,
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
        // A source that reported a finished subject as still awaiting an answer
        // is settled here rather than trusted: `forge::policy` settles what it
        // builds, and a source that answers the envelope directly does not go
        // through it (§FS-003-feed-categories.2).
        matter.settle();
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
    ///
    /// Finished work never does, whatever any of them said
    /// (§FS-003-feed-categories.2): a merged pull request asks nothing of
    /// anyone, however its conversation ended.
    pub fn awaits(&self) -> bool {
        !self.is_finished()
            && (self.needs_response
                || self
                    .discussions
                    .iter()
                    .any(|discussion| discussion.needs_response))
    }

    /// The work is over (§FS-003-feed-categories.2) — the same question the
    /// row's own renderer asks, asked of the model.
    pub fn is_finished(&self) -> bool {
        crate::feed::model::is_terminal(self.state.as_deref())
    }

    /// Finished work is news, not a task (§FS-003-feed-categories.2). Applied
    /// wherever a matter's state or its `needs_response` is set, because the
    /// two arrive from different reports and the row is the pair of them.
    ///
    /// The answer it was owed is kept as news rather than dropped: it is the
    /// loose end that decides whether the news is worth showing. This is where
    /// the fold records it, and the fold is where it usually arrives — a
    /// notice saying somebody is waiting is exactly the report that did not
    /// know the subject had already finished.
    fn settle(&mut self) {
        if self.is_finished() {
            if self.needs_response
                || self
                    .discussions
                    .iter()
                    .any(|discussion| discussion.needs_response)
            {
                crate::feed::model::note_unanswered(&mut self.raw);
            }
            self.needs_response = false;
            for discussion in &mut self.discussions {
                discussion.needs_response = false;
            }
        }
    }
}

/// How many events the fingerprint remembers.
const EVENT_TAIL: usize = 8;

impl Matter {
    /// What makes two reports the same work: the subject the source named,
    /// never the title (§FS-003-feed-categories.5). A report whose subject
    /// cannot be identified is its own subject, so it is left alone rather
    /// than guessed into somebody else's row.
    pub fn subject(&self) -> String {
        let item = self.as_item();
        // Resemblance may start a new row, it may not amend one
        // (§FS-008-attribution.3): a conversation placed by nothing firmer
        // than the project's name is its own subject, so it can never be
        // folded onto one a source actually stated.
        match (item.repo(), item.number()) {
            (Some(repo), Some(number)) if !self.placement.by_resemblance() => {
                format!("{:?}\u{0}{repo}#{number}", self.kind)
            }
            _ => format!("key\u{0}{}", self.key),
        }
    }

    /// How much a report says, for choosing between two of the same subject.
    /// Not a measure of which source is better — sources are not ranked — but
    /// of which row the reader can act on without leaving ephor.
    fn detail(&self) -> u8 {
        u8::from(!self.discussions.is_empty())
            + u8::from(!self.events.is_empty())
            + u8::from(self.placement.branch().is_some())
            + u8::from(self.role.is_some())
            + u8::from(self.state.is_some())
    }

    /// Fold another report of the same subject into this one. What the other
    /// one knew and this one did not comes with it — the conversation it alone
    /// read, the gate it alone fetched, the reason it alone was given — since
    /// that is usually the only thing explaining why the row is there at all
    /// (§FS-003-feed-categories.5).
    pub fn absorb(&mut self, other: Matter) {
        let awaited = self.needs_response || other.needs_response;
        let updated_at = self.updated_at.max(other.updated_at);
        let (mut winner, loser) = if other.detail() > self.detail() {
            (other, std::mem::replace(self, Matter::placeholder()))
        } else {
            (std::mem::replace(self, Matter::placeholder()), other)
        };

        for discussion in loser.discussions {
            if !winner
                .discussions
                .iter()
                .any(|kept| same_discussion(kept, &discussion))
            {
                winner.discussions.push(discussion);
            }
        }
        for event in loser.events {
            if !winner.events.contains(&event) {
                winner.events.push(event);
            }
        }
        for link in loser.links {
            if !winner.links.contains(&link) {
                winner.links.push(link);
            }
        }
        winner.raw = merge_raw(winner.raw, loser.raw);
        winner.needs_response = awaited;
        winner.updated_at = updated_at;
        // The thin report that knew somebody was waiting may be the one that
        // did not know the subject had finished — a notice's state is the
        // reason it was sent, never a terminal state, so nothing settled it
        // before it got here (§FS-003-feed-categories.2).
        winner.settle();
        winner.fingerprint = winner.print();
        *self = winner;
    }

    /// A matter with nothing in it, for the moment inside a fold where one is
    /// being replaced by another.
    fn placeholder() -> Matter {
        Matter {
            key: SubjectKey::stated(String::new()),
            kind: ItemKind::Status,
            placement: Placement::Unattributed {
                candidates: Vec::new(),
            },
            source: String::new(),
            title: String::new(),
            role: None,
            url: None,
            state: None,
            needs_response: false,
            updated_at: DateTime::<Utc>::MIN_UTC,
            links: Vec::new(),
            discussions: Vec::new(),
            events: Vec::new(),
            fingerprint: Fingerprint::default(),
            raw: Value::Null,
        }
    }
}

/// Two reports of the same conversation: who said what, when. Sources that
/// both watch a pull request report the same review threads, and a union that
/// could not tell them apart would show every message twice.
fn same_discussion(one: &Discussion, other: &Discussion) -> bool {
    one.messages.len() == other.messages.len()
        && one
            .messages
            .iter()
            .zip(&other.messages)
            .all(|(left, right)| {
                left.author == right.author && left.time == right.time && left.text == right.text
            })
}

/// The passthrough halves of two reports of one subject. The model's own
/// fields have already been folded; this keeps the source's extra knowledge —
/// including the `threads` and `gate` the not-yet-ported surfaces still read
/// out of it — and unions the reasons a forge gave.
fn merge_raw(winner: Value, loser: Value) -> Value {
    let (Some(mut kept), Some(other)) = (winner.as_object().cloned(), loser.as_object().cloned())
    else {
        return if winner.is_null() { loser } else { winner };
    };
    let reasons = |raw: &serde_json::Map<String, Value>| -> Vec<String> {
        raw.get("reasons")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut merged = reasons(&kept);
    for reason in reasons(&other) {
        if !merged.contains(&reason) {
            merged.push(reason);
        }
    }
    // The conversation and the gate are the two the reader would notice
    // missing, and the winner keeping only its own would drop exactly what
    // the merge exists to keep. They are unioned rather than replaced, in the
    // passthrough as well as in the model, for as long as the surfaces still
    // read them from here.
    let threads = union_threads(kept.get("threads"), other.get("threads"));
    for (key, value) in other {
        // What the winner has, the winner keeps; the rest is what only the
        // other report knew.
        kept.entry(key).or_insert(value);
    }
    if !threads.is_empty() {
        kept.insert("threads".to_string(), Value::Array(threads));
    }
    if !merged.is_empty() {
        kept.insert("reasons".to_string(), serde_json::json!(merged));
        // Merged with a fuller report, this is no longer only a notice.
        kept.remove("notice");
    }
    Value::Object(kept)
}

/// The subject keys a matter refers to: ticket keys it names, in its title
/// and in what was said on it. Referencing is not being: the pull request
/// implementing a ticket and the ticket itself stay two matters
/// (§FS-007-matters.2).
fn references(matter: &Matter) -> Vec<String> {
    let mut spoken = vec![matter.title.clone()];
    if let Some(branch) = matter.placement.branch() {
        spoken.push(branch.to_string());
    }
    for discussion in &matter.discussions {
        for message in &discussion.messages {
            spoken.push(message.text.clone());
        }
    }
    let mut found: Vec<String> = Vec::new();
    for text in spoken {
        for ticket in crate::ticket_ids::tickets_in(&text) {
            if !found.contains(&ticket) {
                found.push(ticket);
            }
        }
    }
    found
}

/// Whether this matter *is* the thing that ticket key names — a task store's
/// matter, or one whose key carries it.
fn names(matter: &Matter, ticket: &str) -> bool {
    matter.key.as_str().contains(ticket)
}

/// Link matters that reference each other (§FS-007-matters.2). Merging what is
/// one thing and linking what is related is the difference between a readable
/// pile and a lossy one: the link is recorded on both, so either row can be
/// read as the place the other belongs with.
pub fn link(mut matters: Vec<Matter>) -> Vec<Matter> {
    let referenced: Vec<(usize, Vec<String>)> = matters
        .iter()
        .enumerate()
        .map(|(index, matter)| (index, references(matter)))
        .collect();
    let keys: Vec<SubjectKey> = matters.iter().map(|matter| matter.key.clone()).collect();

    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (index, tickets) in &referenced {
        for ticket in tickets {
            for (other, _) in matters.iter().enumerate() {
                if other == *index || !names(&matters[other], ticket) {
                    continue;
                }
                if !pairs.contains(&(*index, other)) {
                    pairs.push((*index, other));
                }
            }
        }
    }
    for (one, other) in pairs {
        let key = keys[other].clone();
        if !matters[one].links.contains(&key) {
            matters[one].links.push(key);
        }
        let back = keys[one].clone();
        if !matters[other].links.contains(&back) {
            matters[other].links.push(back);
        }
    }
    matters
}

/// Both reports' conversations, each thread once. Sources that both watch a
/// pull request report the same review threads, and a union that could not
/// tell them apart would show the reader every message twice.
fn union_threads(winner: Option<&Value>, loser: Option<&Value>) -> Vec<Value> {
    let list = |value: Option<&Value>| -> Vec<Value> {
        value.and_then(Value::as_array).cloned().unwrap_or_default()
    };
    let key = |thread: &Value| -> String {
        thread
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .map(|message| {
                        format!(
                            "{}\u{1}{}\u{1}{}",
                            message.get("author").and_then(Value::as_str).unwrap_or(""),
                            message.get("when").and_then(Value::as_str).unwrap_or(""),
                            message.get("text").and_then(Value::as_str).unwrap_or(""),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\u{2}")
            })
            .unwrap_or_default()
    };
    let mut threads = list(winner);
    let mut seen: Vec<String> = threads.iter().map(key).collect();
    for thread in list(loser) {
        let digest = key(&thread);
        if !seen.contains(&digest) {
            seen.push(digest);
            threads.push(thread);
        }
    }
    threads
}

/// One subject, one row (§FS-003-feed-categories.5). Sources overlap on
/// purpose — that overlap is how the feed is exhaustive without being told
/// where to look — so the reader is shown the fullest report of each subject,
/// carrying over what the others knew and it did not.
///
/// Order is preserved from the input, and the input is in provider-name order,
/// so the same feed always merges the same way.
pub fn merge(reports: Vec<Matter>) -> Vec<Matter> {
    let mut order: Vec<String> = Vec::new();
    let mut merged: BTreeMap<String, Matter> = BTreeMap::new();
    for report in reports {
        let subject = report.subject();
        match merged.get_mut(&subject) {
            Some(kept) => kept.absorb(report),
            None => {
                order.push(subject.clone());
                merged.insert(subject, report);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|subject| merged.remove(&subject))
        .collect()
}

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
                channel: Channel {
                    id: format!("{}#{index}", item.source),
                    capabilities: capabilities_of(thread),
                },
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

/// What a channel says it can carry (§FS-007-matters.4). A source may declare
/// it outright with `can`, or by leaving the descriptor a write needs — the
/// pattern `react` already uses, where the thing handed back is also the
/// declaration. Nothing is inferred beyond that: a capability nobody declared
/// narrows the offer rather than being guessed at (§REQ-001-boundary.1).
fn capabilities_of(thread: &Value) -> Vec<Capability> {
    let mut capabilities: Vec<Capability> = thread
        .get("can")
        .and_then(Value::as_array)
        .map(|declared| {
            declared
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|name| match name {
                    "reply" => Some(Capability::Reply),
                    "react" => Some(Capability::React),
                    "tick" => Some(Capability::Tick),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if thread.get("reply").is_some_and(|reply| !reply.is_null())
        && !capabilities.contains(&Capability::Reply)
    {
        capabilities.push(Capability::Reply);
    }
    capabilities
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
                resolved: task_resolved(task),
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

    /// A channel says what it can carry, and nothing is inferred beyond what
    /// it said (§FS-007-matters.4): the declaration is either the word or the
    /// descriptor a write needs, and a channel that left both out is
    /// display-only (§REQ-001-boundary.1).
    #[test]
    fn a_channel_carries_only_the_capabilities_it_declared() {
        let matter = Matter::of_item(&item(json!({"threads": [
            {"messages": [{"author": "ada", "text": "on the diff"}]},
            {"messages": [{"author": "bo", "text": "in the conversation"}],
             "reply": {"provider": "github", "subject_id": "PR_1"}},
            {"messages": [{"author": "cy", "text": "by mail"}], "can": ["reply", "react"]}
        ]})));
        let can = |index: usize, capability| matter.discussions[index].channel.can(capability);
        assert!(!can(0, Capability::Reply));
        // The descriptor a reply needs is also the declaration that one can be
        // sent — the pattern `react` already uses on a message.
        assert!(can(1, Capability::Reply));
        // Or the channel says so outright, which is what the answer envelope
        // gives a project that speaks for itself.
        assert!(can(2, Capability::Reply) && can(2, Capability::React));
        assert!(!can(2, Capability::Tick));
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
        // And the row can say so rather than only reappearing.
        assert_eq!(
            after
                .fingerprint
                .resurfacing(&before.fingerprint)
                .as_deref(),
            Some("⟳ the conversation moved")
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

    fn report(source: &str, key: &str, raw: Value) -> Matter {
        let mut item = item(raw);
        item.id = key.to_string();
        item.source = source.to_string();
        Matter::of_item(&item)
    }

    /// One subject is one row however many sources reported it, and the
    /// fullest report is the one the reader gets (§FS-003-feed-categories.5).
    #[test]
    fn reports_of_one_subject_merge_and_the_thinner_one_hands_over_what_it_saw() {
        let rich = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget", "branch": "you/ABC-42", "reasons": ["authored"] }),
        );
        let mut thin = report(
            "github-threads",
            "github-threads:acme/widget#42",
            json!({ "repo": "acme/widget", "reasons": ["team_mention"], "threads": [
                {"messages": [{"author": "ada", "text": "why?", "when": "2026-08-01T09:00:00Z"}]}
            ]}),
        );
        thin.role = None;
        thin.state = None;

        let merged = merge(vec![rich, thin]);
        assert_eq!(merged.len(), 1);
        let kept = &merged[0];
        assert_eq!(kept.source, "github-prs", "the fuller report is the row");
        // The conversation only the thinner one read came with it…
        assert_eq!(kept.discussions.len(), 1);
        assert_eq!(kept.discussions[0].messages[0].author, "ada");
        // …and so did the reason only it was given.
        let reasons = kept.raw["reasons"].as_array().unwrap();
        assert_eq!(reasons.len(), 2, "{reasons:?}");
    }

    /// A merged row is settled by the state it ends up with, not by what each
    /// report believed on its own (§FS-003-feed-categories.2). The thin report
    /// that knew somebody was waiting is exactly the one that does not know
    /// the subject has finished: a notice's state is the reason it was sent,
    /// never a terminal state, so nothing settles it before the merge.
    #[test]
    fn a_finished_subject_never_awaits_however_the_thin_report_arrived() {
        let mut merged_pr = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget", "branch": "you/ABC-42" }),
        );
        merged_pr.state = Some("merged".to_string());
        merged_pr.needs_response = false;

        let mut notice = report(
            "github-notifications",
            "github-notifications:acme/widget#42",
            json!({ "repo": "acme/widget", "reasons": ["mentioned"], "notice": true }),
        );
        notice.state = Some("mentioned".to_string());
        notice.needs_response = true;
        notice.role = None;

        let rows = merge(vec![merged_pr, notice]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_finished());
        assert!(!rows[0].needs_response, "a merged change asks nothing");
        assert!(!rows[0].awaits());

        // …and yet the row stays: the notice is the report that says somebody
        // is waiting, and the fold keeps that as the loose end that earns the
        // merged change its place under Recent (§FS-003-feed-categories.2).
        assert_eq!(
            rows[0].as_item().loose_end(),
            Some(crate::feed::model::LooseEnd::Unanswered)
        );
        let just_after = rows[0].updated_at + chrono::Duration::hours(1);
        assert!(rows[0].as_item().is_visible(just_after, 7));
    }

    /// The merge nobody said anything about is over in every sense the reader
    /// cares about, and leaves the feed at once (§FS-003-feed-categories.2).
    #[test]
    fn a_merge_nobody_is_waiting_on_leaves_the_feed() {
        let mut merged_pr = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget", "branch": "you/ABC-42" }),
        );
        merged_pr.state = Some("merged".to_string());
        merged_pr.needs_response = false;

        let rows = merge(vec![merged_pr]);
        assert_eq!(rows.len(), 1);
        let just_after = rows[0].updated_at + chrono::Duration::hours(1);
        assert_eq!(rows[0].as_item().loose_end(), None);
        assert!(!rows[0].as_item().is_visible(just_after, 7));
    }

    /// A source that answers the envelope directly does not pass through
    /// `forge::policy`, so the model settles what it is handed
    /// (§FS-003-feed-categories.2).
    #[test]
    fn a_report_that_arrived_finished_and_awaiting_is_settled_on_the_way_in() {
        let mut item = item(json!({}));
        item.state = Some("resolved".to_string());
        item.needs_response = true;
        let matter = Matter::of_item(&item);
        assert!(matter.is_finished());
        assert!(!matter.needs_response);
    }

    /// Resemblance may start a new row, it may not amend one
    /// (§FS-008-attribution.3): a conversation placed by nothing firmer than
    /// the project's name cannot be folded onto a subject a source stated.
    #[test]
    fn a_matter_placed_by_resemblance_never_merges_onto_a_stated_subject() {
        let stated = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget" }),
        );
        let mut guessed = report(
            "email",
            "email:acme/widget#42",
            json!({ "repo": "acme/widget" }),
        );
        // Placed on the same project, but only because the words named it.
        guessed.placement = Placement::claimed("widget", crate::attribution::Strength::Resemblance);

        assert_eq!(merge(vec![stated, guessed]).len(), 2);

        // Placed by the venue, the same pair is one row: the merge rule is
        // about how firmly it was claimed, not about which source spoke.
        let stated = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget" }),
        );
        let mut named = report(
            "email",
            "email:acme/widget#42",
            json!({ "repo": "acme/widget" }),
        );
        named.placement = Placement::claimed("widget", crate::attribution::Strength::Venue);
        assert_eq!(merge(vec![stated, named]).len(), 1);
    }

    #[test]
    fn the_same_conversation_from_two_sources_is_one_conversation() {
        let thread = json!({"messages": [
            {"author": "ada", "text": "why?", "when": "2026-08-01T09:00:00Z"}
        ]});
        let one = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget", "threads": [thread.clone()] }),
        );
        let other = report(
            "github-threads",
            "github-threads:acme/widget#42",
            json!({ "repo": "acme/widget", "threads": [thread] }),
        );
        let merged = merge(vec![one, other]);
        assert_eq!(merged[0].discussions.len(), 1);
    }

    #[test]
    fn a_subject_that_cannot_be_identified_is_left_alone() {
        let one = report("custom-status", "custom-status:widget", json!({}));
        let other = report("custom-status", "custom-status:widget:1", json!({}));
        assert_eq!(merge(vec![one, other]).len(), 2);
    }

    /// Referencing is not being: the change implementing a ticket and the
    /// ticket itself stay two matters, linked (§FS-007-matters.2).
    #[test]
    fn matters_that_reference_each_other_are_linked_rather_than_merged() {
        let mut change = report(
            "github-prs",
            "github-prs:acme/widget#42",
            json!({ "repo": "acme/widget", "branch": "you/ABC-42-retry-window" }),
        );
        change.title = "Retry window".to_string();
        let mut ticket = report("tickets", "ticket:ABC-42", json!({}));
        ticket.title = "Widen the retry window".to_string();

        let linked = link(vec![change, ticket]);
        assert_eq!(linked.len(), 2, "related is not the same as identical");
        assert_eq!(linked[0].links, vec![SubjectKey::stated("ticket:ABC-42")]);
        // The link is on both, so either row reads as the place the other
        // belongs with.
        assert_eq!(
            linked[1].links,
            vec![SubjectKey::stated("github-prs:acme/widget#42")]
        );
    }

    #[test]
    fn a_ticket_named_in_the_conversation_links_too() {
        let mut change = report(
            "github-prs",
            "github-prs:acme/widget#7",
            json!({ "repo": "acme/widget", "threads": [{"messages": [
                {"author": "ada", "text": "this also fixes ABC-99, I think"}
            ]}]}),
        );
        change.title = "Unrelated title".to_string();
        let mut ticket = report("tickets", "ticket:ABC-99", json!({}));
        ticket.title = "Something else".to_string();

        let linked = link(vec![change, ticket]);
        assert_eq!(linked[0].links, vec![SubjectKey::stated("ticket:ABC-99")]);
    }

    /// Evidence is extracted once, at fetch normalization, and is data on the
    /// matter — so a misplacement is debugged by looking (§AR-003-attribution.1).
    #[test]
    fn what_a_report_carries_about_where_it_belongs_is_extracted_from_it() {
        let mut report = item(json!({
            "repo": "acme/widget",
            "threads": [{"messages": [
                {"author": "ada", "text": "this is the same as ABC-42, see other/plugin"}
            ]}]
        }));
        report.title = "Retry window".to_string();
        let evidence = evidence_of(&report);
        assert_eq!(evidence.repo.as_deref(), Some("acme/widget"));
        assert_eq!(evidence.tickets, vec!["ABC-42"]);
        assert!(evidence.repos.contains(&"other/plugin".to_string()));
        // The url names a repository too, which is how a notice with no
        // configured repository still says where it is.
        assert!(evidence.repos.contains(&"acme/widget".to_string()));
        assert_eq!(evidence.addresses, vec!["ada"]);
        assert!(evidence.words.contains("Retry window"));
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
