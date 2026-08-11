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
    Message,
}

impl ItemKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "status" => Some(ItemKind::Status),
            "pr" => Some(ItemKind::Pr),
            "ci" => Some(ItemKind::Ci),
            "issue" => Some(ItemKind::Issue),
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
            ItemKind::Message => "msg",
        }
    }
}

/// The names forges give a state that means the work is over
/// (§FS-003-feed-categories.2). Matched as substrings because forges spell
/// them differently and compose them (`open:changes_requested`, `CLOSED`).
const TERMINAL_STATES: [&str; 5] = ["closed", "merged", "done", "resolved", "declined"];

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
        let state = self.state.as_deref().unwrap_or("").to_lowercase();
        TERMINAL_STATES.iter().any(|needle| state.contains(needle))
    }

    /// Whether a finished item is recent enough to still be shown, against a
    /// window in days. A window of zero shows nothing finished
    /// (§FS-003-feed-categories.3).
    pub fn within_recent_window(&self, now: DateTime<Utc>, recent_days: u64) -> bool {
        recent_days > 0 && (now - self.updated_at).num_days() < recent_days as i64
    }

    /// In the feed at all: unfinished work always, finished work only while it
    /// is recent (§FS-003-feed-categories.2).
    pub fn is_visible(&self, now: DateTime<Utc>, recent_days: u64) -> bool {
        !self.is_finished() || self.within_recent_window(now, recent_days)
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

    #[test]
    fn issue_is_a_kind_of_its_own() {
        assert_eq!(ItemKind::parse("issue"), Some(ItemKind::Issue));
        assert_eq!(ItemKind::Issue.label(), "issue");
    }
}
