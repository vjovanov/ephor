use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Status,
    Pr,
    Ci,
    Message,
}

impl ItemKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "status" => Some(ItemKind::Status),
            "pr" => Some(ItemKind::Pr),
            "ci" => Some(ItemKind::Ci),
            "message" => Some(ItemKind::Message),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Status => "status",
            ItemKind::Pr => "pr",
            ItemKind::Ci => "ci",
            ItemKind::Message => "msg",
        }
    }
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
