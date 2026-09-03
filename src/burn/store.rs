//! The bucket store: five-minute totals, one file per day (§FS-013-burn.5).
//!
//! What a scan reads is folded into buckets and the transcripts are never read
//! again for it. Every window is then a sum over buckets, so changing the
//! window costs a few small file reads rather than a gigabyte — which is the
//! whole reason the store exists.
//!
//! It holds counters and the keys they are counted under, and no transcript
//! text. Day files past the retention are deleted on the way out of a scan, so
//! it bounds itself without anybody running anything.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::{Bucket, Key, Tokens};
use crate::error::{registry_error, Result};

/// How long a bucket is, in seconds (§FS-013-burn.5).
pub const SPAN: i64 = 300;

/// How many days of buckets are kept (§FS-013-burn.5).
pub const RETENTION: i64 = 30;

/// How stale the store may be before a command refreshes it inline
/// (§FS-013-burn.8).
pub const FRESH: i64 = 30;

/// Where the store lives: beside the feed cache, under ephor's own state
/// directory.
pub fn dir() -> PathBuf {
    crate::paths::state_dir().join("burn")
}

/// The start of the five-minute span a moment falls in.
pub fn floor(at: DateTime<Utc>) -> DateTime<Utc> {
    let seconds = at.timestamp();
    Utc.timestamp_opt(seconds - seconds.rem_euclid(SPAN), 0)
        .single()
        .unwrap_or(at)
}

/// One day's buckets, as they are written.
#[derive(Default, Serialize, Deserialize)]
struct Day {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    buckets: Vec<Bucket>,
}

fn version() -> u32 {
    1
}

fn day_path(dir: &Path, day: NaiveDate) -> PathBuf {
    dir.join(format!("{day}.json"))
}

/// Fold `buckets` into the store, adding to whatever is already there.
///
/// A scan that read the tail of a file twice would double what it read, so
/// the merge is by key: the same span and the same key is one row, and the
/// counters add.
pub fn merge(dir: &Path, buckets: Vec<Bucket>) -> Result<()> {
    if buckets.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|err| registry_error(format!("{err}")))?;
    let mut days: BTreeMap<NaiveDate, Vec<Bucket>> = BTreeMap::new();
    for bucket in buckets {
        days.entry(bucket.at.date_naive()).or_default().push(bucket);
    }
    for (day, arriving) in days {
        let path = day_path(dir, day);
        let mut folded: BTreeMap<(DateTime<Utc>, Key), (Tokens, Option<f64>)> = BTreeMap::new();
        for bucket in read_day(&path).into_iter().chain(arriving) {
            let entry = folded
                .entry((bucket.at, bucket.key))
                .or_insert((Tokens::default(), None));
            entry.0.add(&bucket.tokens);
            // Unknown plus known is known; unknown plus unknown stays unknown
            // and never becomes a zero (§FS-013-burn.7).
            if let Some(cost) = bucket.cost_usd {
                entry.1 = Some(entry.1.unwrap_or(0.0) + cost);
            }
        }
        let document = Day {
            version: version(),
            buckets: folded
                .into_iter()
                .map(|((at, key), (tokens, cost_usd))| Bucket {
                    at,
                    key,
                    tokens,
                    cost_usd,
                })
                .collect(),
        };
        let text =
            serde_json::to_string(&document).map_err(|err| registry_error(format!("{err}")))?;
        let temporary = path.with_extension("json.new");
        fs::write(&temporary, text).map_err(|err| registry_error(format!("{err}")))?;
        fs::rename(&temporary, &path).map_err(|err| registry_error(format!("{err}")))?;
    }
    Ok(())
}

fn read_day(path: &Path) -> Vec<Bucket> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Day>(&text).ok())
        .map(|day| day.buckets)
        .unwrap_or_default()
}

/// Every bucket from `since` onwards, oldest first.
///
/// Only the day files that can hold one are opened: a one-hour window over a
/// month of history reads one file, or two across midnight.
pub fn since(dir: &Path, from: DateTime<Utc>, now: DateTime<Utc>) -> Vec<Bucket> {
    let mut found = Vec::new();
    let mut day = from.date_naive();
    let last = now.date_naive();
    while day <= last {
        found.extend(
            read_day(&day_path(dir, day))
                .into_iter()
                .filter(|bucket| bucket.at >= from && bucket.at <= now),
        );
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
    found.sort_by(|left, right| left.at.cmp(&right.at));
    found
}

/// Delete day files past the retention (§FS-013-burn.5).
pub fn sweep(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let oldest = (Utc::now() - Duration::days(RETENTION)).date_naive();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(day) = stem.parse::<NaiveDate>() else {
            // `cursors.json` and anything else that is not a day file.
            continue;
        };
        if day < oldest {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Whether the store is old enough that a command should scan before reading
/// it (§FS-013-burn.8). A store that is not there yet is stale by definition:
/// the first reading builds one rather than refusing.
pub fn stale(dir: &Path) -> bool {
    let Ok(meta) = fs::metadata(super::cursors::path(dir)) else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    modified
        .elapsed()
        .map(|since| since.as_secs() as i64 >= FRESH)
        .unwrap_or(true)
}

/// The day a bucket file is named for — the store's own naming, exposed so a
/// test can put a file where a reading will find it.
pub fn day_of(at: DateTime<Utc>) -> String {
    format!("{:04}-{:02}-{:02}", at.year(), at.month(), at.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(at: &str, project: &str, tokens: u64, cost: Option<f64>) -> Bucket {
        Bucket {
            at: at.parse::<DateTime<Utc>>().expect("a time"),
            key: Key {
                project: project.to_string(),
                source: "claude".to_string(),
                provider: "anthropic".to_string(),
                model: "m".to_string(),
                session: "s".to_string(),
                subagent: false,
            },
            tokens: Tokens {
                output: tokens,
                ..Tokens::default()
            },
            cost_usd: cost,
        }
    }

    /// Two scans of the same span add up rather than replacing each other,
    /// and a window reads back exactly what was written (§FS-013-burn.5).
    #[test]
    fn buckets_merge_by_key_and_read_back_by_window() {
        let home = tempfile::tempdir().expect("a temporary world");
        let dir = home.path().join("burn");
        merge(&dir, vec![bucket("2026-09-03T10:00:00Z", "app", 10, None)]).expect("it writes");
        merge(
            &dir,
            vec![
                bucket("2026-09-03T10:00:00Z", "app", 5, None),
                bucket("2026-09-03T10:05:00Z", "lib", 7, Some(0.5)),
            ],
        )
        .expect("it writes again");
        let now = "2026-09-03T10:10:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        let found = since(&dir, now - Duration::hours(1), now);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].tokens.output, 15, "the two scans did not add");
        assert_eq!(found[0].cost_usd, None);
        assert_eq!(found[1].key.project, "lib");
        assert_eq!(found[1].cost_usd, Some(0.5));
        // A window that starts after them finds nothing rather than the lot.
        assert!(since(&dir, now, now).is_empty());
    }

    /// Retention deletes day files and leaves everything else, `cursors.json`
    /// above all — sweeping that would make the next scan re-read every
    /// transcript on the machine (§FS-013-burn.5).
    #[test]
    fn the_sweep_takes_old_days_and_nothing_else() {
        let home = tempfile::tempdir().expect("a temporary world");
        let dir = home.path().join("burn");
        fs::create_dir_all(&dir).expect("the store");
        let old = Utc::now() - Duration::days(RETENTION + 2);
        merge(
            &dir,
            vec![Bucket {
                at: floor(old),
                ..bucket("2026-09-03T10:00:00Z", "app", 1, None)
            }],
        )
        .expect("it writes");
        merge(
            &dir,
            vec![Bucket {
                at: floor(Utc::now()),
                ..bucket("2026-09-03T10:00:00Z", "app", 1, None)
            }],
        )
        .expect("it writes");
        fs::write(super::super::cursors::path(&dir), "{}").expect("the cursors");
        sweep(&dir);
        assert!(super::super::cursors::path(&dir).exists());
        assert!(!dir.join(format!("{}.json", day_of(old))).exists());
        assert!(dir.join(format!("{}.json", day_of(Utc::now()))).exists());
    }

    /// A store nobody has built yet is stale, so the first reading builds one
    /// rather than refusing (§FS-013-burn.8).
    #[test]
    fn a_store_that_is_not_there_is_stale() {
        let home = tempfile::tempdir().expect("a temporary world");
        assert!(stale(&home.path().join("burn")));
    }
}
