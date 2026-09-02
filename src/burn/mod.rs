//! What this machine spends on agents, read out of the logs the agent
//! command-line tools already write (§FS-013-burn).
//!
//! This is the **machine lens** (§FS-013-burn.1): the ground truth for a
//! total, because every agent this machine runs — a person's own session and
//! one the runtime started alike — writes the same transcript. The work lens
//! is somebody else's: it is read out of the runtime's own accounting records
//! by the module that adapts that runtime, and the two are never added
//! together.
//!
//! Nothing here fetches. Every read is of a local file the tool was writing
//! anyway, which is why refreshing the store is not the fetch `refresh` owns
//! (§FS-013-burn.8).

pub mod attribution;
pub mod claude;
pub mod codex;
pub mod cursors;
pub mod query;
pub mod store;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// The four counters, kept apart all the way to the screen (§FS-013-burn.3).
///
/// A cache read and an input token are priced an order of magnitude apart, so
/// one "tokens" number would hide which of them moved.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub input: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub output: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cache_read: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cache_write: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn add(&mut self, other: &Tokens) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
    }

    /// This counter minus an earlier one, componentwise. What the log that
    /// restates a running total is diffed with (§FS-013-burn.3).
    pub fn since(&self, earlier: &Tokens) -> Tokens {
        Tokens {
            input: self.input.saturating_sub(earlier.input),
            output: self.output.saturating_sub(earlier.output),
            cache_read: self.cache_read.saturating_sub(earlier.cache_read),
            cache_write: self.cache_write.saturating_sub(earlier.cache_write),
        }
    }

    /// Whether this counter has gone backwards from `earlier` — a session
    /// whose running total was reset rather than one that spent nothing.
    pub fn behind(&self, earlier: &Tokens) -> bool {
        self.total() < earlier.total()
    }
}

/// What a bucket is counted under (§FS-013-burn.5).
///
/// Model and provider stay together: not every model an agent tool runs is
/// served by the vendor whose tool it is, and a bare model name would file two
/// different prices under one row.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Key {
    /// A registry project id, or [`attribution::OTHER`] (§FS-013-burn.4).
    pub project: String,
    /// Which agent tool wrote the record it came from.
    pub source: String,
    pub provider: String,
    pub model: String,
    /// The tool's own session id — what `--by session` groups on, and what a
    /// live row is about.
    pub session: String,
    /// The record came from a sub-agent's transcript rather than the
    /// session's own (§FS-013-burn.3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subagent: bool,
}

/// One record's spend, as an adapter read it, before it is attributed and
/// bucketed.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub at: DateTime<Utc>,
    /// The directory the agent ran in, where the record named one.
    pub cwd: Option<String>,
    pub source: &'static str,
    pub provider: String,
    pub model: String,
    pub session: String,
    pub subagent: bool,
    pub tokens: Tokens,
    /// Dollars, only where the log carried them already (§FS-013-burn.7).
    /// `None` is *unknown*, and never renders as zero.
    pub cost_usd: Option<f64>,
}

/// One five-minute bucket: what one key spent in one span (§FS-013-burn.5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    /// The start of the five-minute span.
    pub at: DateTime<Utc>,
    #[serde(flatten)]
    pub key: Key,
    #[serde(flatten)]
    pub tokens: Tokens,
    /// Unknown where nothing priced it — distinct from a priced zero
    /// (§FS-013-burn.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// Where the two logs the machine lens reads live (§FS-013-burn.2).
///
/// Both are the vendor tools' own directories under the person's home, and
/// both are given rather than found so that a test is a test of the reader
/// rather than of this laptop.
pub struct Sources {
    /// One directory per project, each holding one transcript per session.
    pub claude: PathBuf,
    /// Sessions filed under a date tree.
    pub codex: PathBuf,
}

impl Sources {
    /// Where the tools write on this machine.
    pub fn of_home() -> Sources {
        let home = crate::paths::home_dir();
        Sources {
            claude: home.join(".claude").join("projects"),
            codex: home.join(".codex").join("sessions"),
        }
    }
}

/// What one scan of the transcripts came to. The byte count is the point:
/// a scan that re-read everything would work and would cost a gigabyte a pass
/// (§FS-013-burn.5).
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Scan {
    pub files: usize,
    pub bytes: u64,
    pub samples: usize,
}

/// Read what the transcripts have gained, bucket it, and leave the store
/// current (§FS-013-burn.5).
///
/// Everything about this is incremental: an untouched file is not opened, a
/// grown one is read from its cursor, and what comes out is folded into the
/// day files rather than kept. Day files past the retention are swept on the
/// way out, so the store bounds itself without anybody running anything.
pub fn refresh(dir: &Path, sources: &Sources, roots: &attribution::Roots) -> Result<Scan> {
    let mut cursors = cursors::load(dir);
    let mut scan = Scan::default();
    let mut samples: Vec<Sample> = Vec::new();
    for (source, file) in transcripts(sources) {
        let named = file.display().to_string();
        let mut cursor = cursors.files.remove(&named).unwrap_or_default();
        let Some(read) = cursors::appended(&file, &cursor) else {
            cursors.files.insert(named, cursor);
            continue;
        };
        if read.restarted {
            cursor.carry = cursors::Carry::default();
        }
        scan.files += 1;
        scan.bytes += read.bytes;
        let found = match source {
            Source::Claude => claude::read(&file, &read.text, &mut cursor.carry),
            Source::Codex => codex::read(&read.text, &mut cursor.carry),
        };
        scan.samples += found.len();
        samples.extend(found);
        cursor.offset = read.offset;
        let (size, mtime) = cursors::stat(&file);
        cursor.size = size;
        cursor.mtime = mtime;
        cursors.files.insert(named, cursor);
    }
    store::merge(dir, buckets(samples, roots))?;
    cursors::store(dir, &cursors)?;
    store::sweep(dir);
    Ok(scan)
}

/// Which tool wrote a transcript. Kept as an enum rather than a string so
/// that adding a third log is a match arm the compiler asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
    Claude,
    Codex,
}

/// Every transcript under either source, in a stable order.
fn transcripts(sources: &Sources) -> Vec<(Source, PathBuf)> {
    let mut found = Vec::new();
    for (source, root) in [
        (Source::Claude, &sources.claude),
        (Source::Codex, &sources.codex),
    ] {
        let mut here = Vec::new();
        walk(root, &mut here, 0);
        here.sort();
        found.extend(here.into_iter().map(|path| (source, path)));
    }
    found
}

/// How deep either tool files a transcript. Bounded rather than unbounded:
/// this walks a directory of somebody's home, and a symlink loop there must
/// not become an unbounded walk in a reading.
const DEPTH: usize = 8;

fn walk(at: &Path, found: &mut Vec<PathBuf>, depth: usize) {
    if depth > DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&path, found, depth + 1),
            Ok(_) if path.extension().is_some_and(|ext| ext == "jsonl") => found.push(path),
            _ => {}
        }
    }
}

/// Samples folded into five-minute buckets, attributed on the way in
/// (§FS-013-burn.4, §FS-013-burn.5).
pub fn buckets(samples: Vec<Sample>, roots: &attribution::Roots) -> Vec<Bucket> {
    let mut folded: BTreeMap<(DateTime<Utc>, Key), (Tokens, Option<f64>)> = BTreeMap::new();
    for sample in samples {
        let key = Key {
            project: roots.project(sample.cwd.as_deref()),
            source: sample.source.to_string(),
            provider: sample.provider,
            model: sample.model,
            session: sample.session,
            subagent: sample.subagent,
        };
        let entry = folded
            .entry((store::floor(sample.at), key))
            .or_insert((Tokens::default(), None));
        entry.0.add(&sample.tokens);
        // A dollar figure only where one was carried: adding a known cost to
        // an unknown one must leave the known one, never turn the unknown into
        // a zero (§FS-013-burn.7).
        if let Some(cost) = sample.cost_usd {
            entry.1 = Some(entry.1.unwrap_or(0.0) + cost);
        }
    }
    folded
        .into_iter()
        .map(|((at, key), (tokens, cost_usd))| Bucket {
            at,
            key,
            tokens,
            cost_usd,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unknown and zero are different facts, and folding must not make them
    /// one (§FS-013-burn.7). A bucket nothing priced stays `None`; one that
    /// was priced at nothing stays `Some(0.0)`.
    #[test]
    fn folding_keeps_unpriced_apart_from_priced_at_nothing() {
        let roots = attribution::Roots::new(Vec::new());
        let sample = |cost| Sample {
            at: "2026-09-03T10:01:00Z".parse().expect("a time"),
            cwd: None,
            source: "claude",
            provider: "anthropic".to_string(),
            model: "m".to_string(),
            session: "s".to_string(),
            subagent: false,
            tokens: Tokens {
                output: 1,
                ..Tokens::default()
            },
            cost_usd: cost,
        };
        let unpriced = buckets(vec![sample(None), sample(None)], &roots);
        assert_eq!(unpriced.len(), 1);
        assert_eq!(unpriced[0].cost_usd, None);
        assert_eq!(unpriced[0].tokens.output, 2);

        let free = buckets(vec![sample(Some(0.0))], &roots);
        assert_eq!(free[0].cost_usd, Some(0.0));

        // One priced record among unpriced ones does not price the others,
        // and does not lose its own dollars either.
        let mixed = buckets(vec![sample(None), sample(Some(0.25))], &roots);
        assert_eq!(mixed[0].cost_usd, Some(0.25));
    }

    /// Everything in one five-minute span is one bucket, and the span is the
    /// floor rather than the first record's time (§FS-013-burn.5).
    #[test]
    fn one_span_is_one_bucket() {
        let roots = attribution::Roots::new(Vec::new());
        let at = |when: &str| Sample {
            at: when.parse().expect("a time"),
            cwd: None,
            source: "codex",
            provider: "openai".to_string(),
            model: "m".to_string(),
            session: "s".to_string(),
            subagent: false,
            tokens: Tokens {
                input: 10,
                ..Tokens::default()
            },
            cost_usd: None,
        };
        let folded = buckets(
            vec![
                at("2026-09-03T10:01:00Z"),
                at("2026-09-03T10:04:59Z"),
                at("2026-09-03T10:05:00Z"),
            ],
            &roots,
        );
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0].at.to_rfc3339(), "2026-09-03T10:00:00+00:00");
        assert_eq!(folded[0].tokens.input, 20);
        assert_eq!(folded[1].at.to_rfc3339(), "2026-09-03T10:05:00+00:00");
    }
}
