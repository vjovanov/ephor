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
use crate::matter::Matter;
use crate::paths;

/// What model the stored matters are in. The cache is a cache
/// (§AR-006-matters.4): a model change rebuilds it rather than migrating it,
/// because everything in it can be fetched again. `seen` is the one part that
/// survives, and it lives in its own file for exactly that reason.
pub const MODEL: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectFeed {
    pub project: String,
    #[serde(default)]
    pub fetched_at: Option<DateTime<Utc>>,
    /// The model the matters below were stored in. An older one is dropped on
    /// load; a refresh fills it again.
    #[serde(default)]
    pub model: u32,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderSlot>,
}

/// Per-provider results. A failing provider keeps its last-good matters
/// with `stale: true` so one flaky source never blanks the feed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// the reader can fix — so the matters below are last-good data waiting
    /// out a network, not evidence that this source has nothing. Written only
    /// when true: a healthy slot should not carry a field about how it failed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unreachable: bool,
    /// What this source reported, as matters (§AR-006-matters.1).
    #[serde(default)]
    pub matters: Vec<Matter>,
    /// Incremental fetch cursor for message providers (slack/discord/email).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ProjectFeed {
    /// Everything every source reported, in provider-name order.
    pub fn matters(&self) -> impl Iterator<Item = &Matter> {
        self.providers.values().flat_map(|slot| slot.matters.iter())
    }

    /// The project's items as the reader sees them: every source's report,
    /// with a subject that several sources found merged into one row
    /// (§FS-003-feed-categories.5). `providers` is ordered by name, so the
    /// merge is the same on every read.
    ///
    /// The flat shape is a rendering of the matters, not a second copy of
    /// them (§AR-006-matters.3): the surfaces still read it while they are
    /// ported onto the model.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        crate::forge::policy::merge_reports(self.matters().map(Matter::as_item).collect())
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
    let feed: ProjectFeed = serde_json::from_str(&text).map_err(|err| {
        EphorError::Command(format!("Corrupt feed cache {}: {err}", path.display()))
    })?;
    if feed.model != MODEL {
        // A cache in an older model is not migrated: it is dropped, and the
        // next refresh fetches it again (§AR-006-matters.4). What the reader
        // marked read is in `seen.json` and is untouched by this.
        return Ok(Some(ProjectFeed {
            project: feed.project,
            ..ProjectFeed::default()
        }));
    }
    Ok(Some(feed))
}

pub fn store_feed(feed: &ProjectFeed) -> Result<()> {
    write_atomic(
        &feed_path(&feed.project),
        &serde_json::to_string_pretty(&ProjectFeed {
            project: feed.project.clone(),
            fetched_at: feed.fetched_at,
            model: MODEL,
            providers: feed.providers.clone(),
        })
        .unwrap(),
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

/// Whether a matter has moved since the reader last looked at it. The key is
/// the matter's own (§AR-006-matters.4), which is why marking read survives a
/// model rebuild: reading is per version, not per row
/// (§FS-003-feed-categories.2).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::{Matter, SubjectKey};

    fn matter(key: &str) -> Matter {
        Matter {
            key: SubjectKey::stated(key),
            kind: crate::feed::model::ItemKind::Pr,
            placement: crate::matter::Placement::on("widget"),
            source: "github-prs".to_string(),
            title: "Retry window".to_string(),
            role: None,
            url: None,
            state: None,
            needs_response: false,
            updated_at: "2026-08-01T10:00:00Z".parse().unwrap(),
            links: Vec::new(),
            discussions: Vec::new(),
            events: Vec::new(),
            fingerprint: Default::default(),
            raw: serde_json::Value::Null,
        }
    }

    fn feed_with(model: u32) -> String {
        let mut feed = ProjectFeed {
            project: "widget".to_string(),
            fetched_at: None,
            model,
            providers: BTreeMap::new(),
        };
        feed.providers.insert(
            "github-prs".to_string(),
            ProviderSlot {
                ok: true,
                matters: vec![matter("github-prs:acme/widget#42")],
                ..ProviderSlot::default()
            },
        );
        serde_json::to_string_pretty(&feed).unwrap()
    }

    /// The cache is a cache (§AR-006-matters.4): a store written in an older
    /// model is dropped rather than migrated, and the next refresh fills it.
    #[test]
    fn a_store_in_an_older_model_is_rebuilt_not_migrated() {
        let current: ProjectFeed = serde_json::from_str(&feed_with(MODEL)).unwrap();
        assert_eq!(current.matters().count(), 1);

        // What `load_feed` does with an older model, without touching the
        // filesystem: the project is kept, its matters are not.
        let older: ProjectFeed = serde_json::from_str(&feed_with(MODEL - 1)).unwrap();
        assert_ne!(older.model, MODEL);
        let rebuilt = ProjectFeed {
            project: older.project,
            ..ProjectFeed::default()
        };
        assert_eq!(rebuilt.project, "widget");
        assert_eq!(rebuilt.matters().count(), 0);
    }

    /// `seen` is the one part carried across a rebuild, keyed by matter key —
    /// which is why it lives in its own file and is never written by the
    /// feed store (§AR-006-matters.4).
    #[test]
    fn what_the_reader_marked_read_is_keyed_by_the_matter_and_outlives_the_feed() {
        let matter = matter("github-prs:acme/widget#42");
        let mut seen = Seen::new();
        assert!(is_unread(&seen, &matter.as_item()));

        seen.insert(matter.key.as_str().to_string(), matter.updated_at);
        assert!(!is_unread(&seen, &matter.as_item()));

        // The matter moves: it resurfaces, whatever the store did meanwhile.
        let mut moved = matter.clone();
        moved.updated_at = "2026-08-02T10:00:00Z".parse().unwrap();
        assert!(is_unread(&seen, &moved.as_item()));
    }

    /// A store always says which model it is in, so the next release can tell.
    #[test]
    fn the_store_stamps_the_model_it_wrote() {
        let written: serde_json::Value = serde_json::from_str(&feed_with(MODEL)).unwrap();
        assert_eq!(written["model"], MODEL);
        assert!(written["providers"]["github-prs"]["matters"][0]["key"]
            .as_str()
            .is_some());
    }
}
