use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Status,
    Pr,
    Ci,
    Issue,
    /// The project's own task, read out of a store in its checkout
    /// (§FS-006-project-interface.7). Not an issue: an issue is what a forge
    /// files, and this is the project's own work
    /// (§FS-003-feed-categories.1).
    Task,
    Message,
}

impl ItemKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "status" => Some(ItemKind::Status),
            "pr" => Some(ItemKind::Pr),
            "ci" => Some(ItemKind::Ci),
            "issue" => Some(ItemKind::Issue),
            "task" => Some(ItemKind::Task),
            "message" => Some(ItemKind::Message),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Status => "status",
            ItemKind::Pr => "pr",
            ItemKind::Ci => "ci",
            ItemKind::Issue => "issue",
            ItemKind::Task => "task",
            ItemKind::Message => "msg",
        }
    }
}

/// The names forges give a state that means the work is over
/// (§FS-003-feed-categories.2). Matched as substrings because forges spell
/// them differently and compose them (`open:changes_requested`, `CLOSED`).
const TERMINAL_STATES: [&str; 5] = ["closed", "merged", "done", "resolved", "declined"];

/// Whether a state means the work is over (§FS-003-feed-categories.2). Free of
/// [`Item`] so the model can ask it of a matter without building a report to
/// ask it about — the two must answer the same way or a row lands in one
/// category and settles by another.
pub fn is_terminal(state: Option<&str>) -> bool {
    let state = state.unwrap_or("").to_lowercase();
    TERMINAL_STATES.iter().any(|needle| state.contains(needle))
}

/// Where a settled report keeps the answer it was owed
/// (§FS-003-feed-categories.2). Finishing clears `needs_response`, because a
/// finished item is news and not a task; this is the same fact kept as news,
/// which is what tells the finished work that still has a loose end from the
/// finished work that has none.
pub const UNANSWERED: &str = "unanswered";

/// Record on a report's passthrough that an answer was missing when the
/// subject finished. Written wherever settling clears the response it owed, so
/// that clearing it does not also forget it (§FS-003-feed-categories.2).
pub fn note_unanswered(raw: &mut Value) {
    match raw {
        Value::Object(map) => {
            map.insert(UNANSWERED.to_string(), Value::Bool(true));
        }
        // A report that carried nothing else still carries this.
        Value::Null => *raw = serde_json::json!({ UNANSWERED: true }),
        _ => {}
    }
}

/// Why a finished item is still in front of the reader
/// (§FS-003-feed-categories.2). Finished work with none of these is over in
/// every sense the reader cares about and leaves the feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LooseEnd {
    /// An answer is missing — whatever would have made the subject await one
    /// while it was still open (§FS-003-feed-categories.4).
    Unanswered,
    /// The gate went the other way, after the merge or before it.
    RedGate,
    /// The runtime still holds a ticket about this matter
    /// (§FS-005-dispatch.23). Not a fact of any report: the ledger's, so a
    /// surface that reads the ledger adds it and the model never guesses it.
    Working,
}

/// Whether an item is the user's own work or something they are reviewing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemRole {
    Author,
    Reviewer,
}

/// One entry in a project's information stream. `id` must be stable across
/// fetches (it is the unread-tracking key), so providers derive it from
/// natural identifiers (repo#number, ticket key, message timestamp) — never
/// from array positions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub project: String,
    pub source: String,
    pub kind: ItemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ItemRole>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub needs_response: bool,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub raw: Value,
}

impl Item {
    /// The work is over: the item belongs under Recent rather than in its own
    /// category (§FS-003-feed-categories.2).
    pub fn is_finished(&self) -> bool {
        is_terminal(self.state.as_deref())
    }

    /// Whether a finished item is recent enough to still be shown, against a
    /// window in days. A window of zero shows nothing finished
    /// (§FS-003-feed-categories.3).
    pub fn within_recent_window(&self, now: DateTime<Utc>, recent_days: u64) -> bool {
        recent_days > 0 && (now - self.updated_at).num_days() < recent_days as i64
    }

    /// What this finished item still leaves its reader to do
    /// (§FS-003-feed-categories.2). None while it is unfinished — such an item
    /// is in the feed because of its category, not because of this — and None
    /// on finished work that asks nothing, which is most of it.
    ///
    /// Two of the three the spec lists are facts of the report and are read
    /// here. The third, work still open on the matter, belongs to the ledger
    /// and is added by the surfaces that read one.
    pub fn loose_end(&self) -> Option<LooseEnd> {
        if !self.is_finished() {
            return None;
        }
        if self
            .raw
            .get(UNANSWERED)
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Some(LooseEnd::Unanswered);
        }
        crate::feed::gate::Gate::of(self)
            .is_some_and(|gate| gate.is_red())
            .then_some(LooseEnd::RedGate)
    }

    /// In the feed at all: unfinished work always; finished work only while it
    /// still has a loose end (§FS-003-feed-categories.2) whose last activity is
    /// inside the recency window (§FS-003-feed-categories.3).
    ///
    /// Work the runtime still holds open keeps a matter here too, and is not
    /// bounded by the window — but it is the ledger's fact rather than the
    /// report's, so it is added where a ledger is in hand rather than guessed
    /// at from a row.
    pub fn is_visible(&self, now: DateTime<Utc>, recent_days: u64) -> bool {
        if !self.is_finished() {
            return true;
        }
        self.within_recent_window(now, recent_days) && self.loose_end().is_some()
    }

    /// The repository, best effort: `raw.repo`, or the `owner/name` between
    /// the source prefix and `#` in the id (`github-prs:acme/widget#42`).
    pub fn repo(&self) -> Option<String> {
        if let Some(repo) = self.raw.get("repo").and_then(Value::as_str) {
            return Some(repo.to_string());
        }
        let tail = self.id.split_once(':')?.1;
        let (repo, _) = tail.rsplit_once('#')?;
        if repo.contains('/') {
            Some(repo.to_string())
        } else {
            None
        }
    }

    /// The pull request or issue number, best effort: the digits after the
    /// last `#` (`github-prs:acme/widget#42`) or the last `/`
    /// (`forge-prs:repo/123`) of the id.
    pub fn number(&self) -> Option<String> {
        for separator in ['#', '/'] {
            if let Some((_, tail)) = self.id.rsplit_once(separator) {
                if !tail.is_empty() && tail.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Some(tail.to_string());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(state: Option<&str>, updated_at: DateTime<Utc>) -> Item {
        Item {
            id: "x".to_string(),
            project: "p".to_string(),
            source: "s".to_string(),
            kind: ItemKind::Issue,
            role: None,
            title: "t".to_string(),
            url: None,
            state: state.map(String::from),
            needs_response: false,
            updated_at,
            raw: Value::Null,
        }
    }

    #[test]
    fn terminal_states_are_recognized_however_the_forge_spells_them() {
        for state in ["closed", "CLOSED", "open:merged", "Done", "declined"] {
            assert!(item(Some(state), Utc::now()).is_finished(), "{state}");
        }
        for state in ["open", "open:changes_requested", "in progress"] {
            assert!(!item(Some(state), Utc::now()).is_finished(), "{state}");
        }
        assert!(!item(None, Utc::now()).is_finished());
    }

    #[test]
    fn the_recency_window_is_a_span_of_days_and_zero_closes_it() {
        let now = Utc::now();
        let two_days_ago = item(Some("closed"), now - chrono::Duration::days(2));
        assert!(two_days_ago.within_recent_window(now, 7));
        assert!(!two_days_ago.within_recent_window(now, 1));
        assert!(!two_days_ago.within_recent_window(now, 0));
    }

    /// Finished work is in the feed only while it still leaves something to do
    /// (§FS-003-feed-categories.2). The merge that went as asked is over: it is
    /// not news anybody has to clear, and Recent is not a list of it.
    #[test]
    fn a_finished_item_with_nothing_left_to_do_leaves_the_feed_at_once() {
        let now = Utc::now();
        let merged = item(Some("merged"), now - chrono::Duration::hours(2));
        assert_eq!(merged.loose_end(), None);
        assert!(
            !merged.is_visible(now, 7),
            "inside the window and still over"
        );

        // Unfinished work is in the feed because of its category, and this
        // question is never asked of it.
        let open = item(Some("open"), now - chrono::Duration::days(400));
        assert_eq!(open.loose_end(), None);
        assert!(open.is_visible(now, 7));
    }

    /// The two loose ends a report knows: an answer that was missing when the
    /// subject finished, and a gate that went the other way
    /// (§FS-003-feed-categories.2).
    #[test]
    fn a_finished_item_stays_while_an_answer_is_missing_or_the_gate_is_red() {
        let now = Utc::now();

        let mut commented = item(Some("merged"), now - chrono::Duration::hours(2));
        note_unanswered(&mut commented.raw);
        assert_eq!(commented.loose_end(), Some(LooseEnd::Unanswered));
        assert!(commented.is_visible(now, 7));

        let mut red = item(Some("merged"), now - chrono::Duration::hours(2));
        red.raw = serde_json::json!({ "gate": { "repos": [
            { "repo": "acme/widget", "passed": 3, "failed": 1 }
        ] } });
        assert_eq!(red.loose_end(), Some(LooseEnd::RedGate));
        assert!(red.is_visible(now, 7));

        // A gate that went green is not a loose end, whatever else it says.
        let mut green = item(Some("merged"), now - chrono::Duration::hours(2));
        green.raw = serde_json::json!({ "gate": { "repos": [
            { "repo": "acme/widget", "passed": 4 }
        ] } });
        assert_eq!(green.loose_end(), None);
        assert!(!green.is_visible(now, 7));
    }

    /// The window still bounds a loose end the report knows: a conversation
    /// nobody answered a year ago is not this week's work
    /// (§FS-003-feed-categories.3).
    #[test]
    fn a_loose_end_outside_the_window_leaves_with_everything_else() {
        let now = Utc::now();
        let mut old = item(Some("closed"), now - chrono::Duration::days(30));
        note_unanswered(&mut old.raw);
        assert_eq!(old.loose_end(), Some(LooseEnd::Unanswered));
        assert!(!old.is_visible(now, 7));
        assert!(old.is_visible(now, 90));
    }

    /// The mark goes on a report that carried nothing else as readily as on one
    /// that carried a conversation: a notice is the thinnest report there is,
    /// and it is exactly the report that says somebody is waiting.
    #[test]
    fn the_unanswered_mark_lands_on_a_report_that_carried_nothing() {
        let mut raw = Value::Null;
        note_unanswered(&mut raw);
        assert_eq!(raw[UNANSWERED], serde_json::json!(true));

        let mut carrying = serde_json::json!({ "repo": "acme/widget" });
        note_unanswered(&mut carrying);
        assert_eq!(carrying["repo"], serde_json::json!("acme/widget"));
        assert_eq!(carrying[UNANSWERED], serde_json::json!(true));
    }

    #[test]
    fn issue_is_a_kind_of_its_own() {
        assert_eq!(ItemKind::parse("issue"), Some(ItemKind::Issue));
        assert_eq!(ItemKind::Issue.label(), "issue");
    }

    /// The project's own task is its own kind, and not an issue: an issue is
    /// what a forge files, and a task is the project's own work in its own
    /// checkout (§FS-003-feed-categories.1). The label is what a serialized
    /// matter carries and what a recipe's `kinds` matches, so it round-trips.
    #[test]
    fn task_is_a_kind_of_its_own() {
        assert_eq!(ItemKind::parse("task"), Some(ItemKind::Task));
        assert_eq!(ItemKind::Task.label(), "task");
        assert_ne!(ItemKind::Task, ItemKind::Issue);
        assert_eq!(
            serde_json::to_value(ItemKind::Task).unwrap(),
            serde_json::json!("task")
        );
    }
}
