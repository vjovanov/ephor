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

/// Whether a state means the work is over (§FS-003-feed-categories.2). Free of
/// [`Item`] so the model can ask it of a matter without building a report to
/// ask it about — the two must answer the same way or a row lands in one
/// category and settles by another.
pub fn is_terminal(state: Option<&str>) -> bool {
    let state = state.unwrap_or("").to_lowercase();
    TERMINAL_STATES.iter().any(|needle| state.contains(needle))
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

    /// In the feed at all: unfinished work always, finished work only while it
    /// is recent (§FS-003-feed-categories.2).
    pub fn is_visible(&self, now: DateTime<Utc>, recent_days: u64) -> bool {
        !self.is_finished() || self.within_recent_window(now, recent_days)
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

    #[test]
    fn issue_is_a_kind_of_its_own() {
        assert_eq!(ItemKind::parse("issue"), Some(ItemKind::Issue));
        assert_eq!(ItemKind::Issue.label(), "issue");
    }
}
