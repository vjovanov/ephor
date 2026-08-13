//! Cached feed state under `~/.local/state/ephor/`: one JSON document per
//! project plus a global `seen.json` for unread tracking. Writes are atomic
//! (tmp file + rename) so an interrupted refresh never corrupts state.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{EphorError, Result};
use crate::feed::model::Item;
use crate::paths;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectFeed {
    pub project: String,
    #[serde(default)]
    pub fetched_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderSlot>,
}

/// Per-provider results. A failing provider keeps its last-good `items`
/// with `stale: true` so one flaky source never blanks the feed.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProviderSlot {
    #[serde(default)]
    pub fetched_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub stale: bool,
    /// The failure was "could not reach the destination" rather than anything
    /// the reader can fix — so the items below are last-good data waiting out
    /// a network, not evidence that this source has nothing. Written only when
    /// true: a healthy slot should not carry a field about how it failed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unreachable: bool,
    #[serde(default)]
    pub items: Vec<Item>,
    /// Incremental fetch cursor for message providers (slack/discord/email).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ProjectFeed {
    /// The project's items as the reader sees them: every source's report,
    /// with a subject that several sources found merged into one row
    /// (§FS-003-feed-categories.5). `providers` is ordered by name, so the
    /// merge is the same on every read.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        crate::forge::policy::merge_reports(
            self.providers
                .values()
                .flat_map(|slot| slot.items.iter().cloned())
                .collect(),
        )
        .into_iter()
    }

    pub fn is_stale(&self, item_source: &str) -> bool {
        self.providers
            .get(item_source)
            .map(|slot| slot.stale)
            .unwrap_or(false)
    }
}

pub fn feed_dir() -> PathBuf {
    paths::state_dir().join("feed")
}

pub fn feed_path(project_id: &str) -> PathBuf {
    feed_dir().join(format!("{project_id}.json"))
}

fn seen_path() -> PathBuf {
    paths::state_dir().join("seen.json")
}

pub fn load_feed(project_id: &str) -> Result<Option<ProjectFeed>> {
    let path = feed_path(project_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", path.display())))?;
    let feed = serde_json::from_str(&text).map_err(|err| {
        EphorError::Command(format!("Corrupt feed cache {}: {err}", path.display()))
    })?;
    Ok(Some(feed))
}

pub fn store_feed(feed: &ProjectFeed) -> Result<()> {
    write_atomic(
        &feed_path(&feed.project),
        &serde_json::to_string_pretty(feed).unwrap(),
    )
}

pub type Seen = BTreeMap<String, DateTime<Utc>>;

pub fn load_seen() -> Result<Seen> {
    let path = seen_path();
    if !path.exists() {
        return Ok(Seen::new());
    }
    let text = fs::read_to_string(&path)
        .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|err| EphorError::Command(format!("Corrupt seen file {}: {err}", path.display())))
}

pub fn store_seen(seen: &Seen) -> Result<()> {
    write_atomic(&seen_path(), &serde_json::to_string_pretty(seen).unwrap())
}

pub fn is_unread(seen: &Seen, item: &Item) -> bool {
    match seen.get(&item.id) {
        None => true,
        Some(read_at) => item.updated_at > *read_at,
    }
}

fn write_atomic(path: &PathBuf, content: &str) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        EphorError::Command(format!("No parent directory for {}", path.display()))
    })?;
    fs::create_dir_all(parent)
        .map_err(|err| EphorError::Command(format!("Cannot create {}: {err}", parent.display())))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", tmp.display())))?;
    fs::rename(&tmp, path)
        .map_err(|err| EphorError::Command(format!("Cannot rename {}: {err}", tmp.display())))?;
    Ok(())
}
