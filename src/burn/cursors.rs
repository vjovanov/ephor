//! Where the last scan stopped, per transcript file (§FS-013-burn.5).
//!
//! Both logs the machine lens reads are append-only JSONL, so a scan that
//! remembers a byte offset reads what was appended and nothing else. The
//! initial pass reads everything once; every pass after it reads the tail.
//!
//! A cursor carries more than an offset. Neither adapter can read a record in
//! isolation — one log restates a running total that has to be diffed against
//! the previous one, and both name the session's directory and model once at
//! the top and never again — so whatever the reader would have known from the
//! records it already consumed is carried here beside the offset
//! (§FS-013-burn.5).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::Tokens;
use crate::error::Result;

/// What a reader carries from one scan of one file to the next
/// (§FS-013-burn.5).
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Carry {
    /// The session the file is about, named once at its top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The directory it ran in — what attribution matches (§FS-013-burn.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// The model in force. A session that changes model mid-way changes this,
    /// and every delta after it is that model's (§FS-013-burn.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The session's last cumulative counters, for the log that restates its
    /// running total on every event rather than reporting a delta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totals: Option<Tokens>,
    /// The last cost total seen, per model, for the log that rolls its own
    /// dollars up per session (§FS-013-burn.7).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub costs: BTreeMap<String, f64>,
    /// Every model this session's own calls named. What a dollar rollup
    /// keyed by a longer spelling of one of them is joined back to
    /// (§FS-013-burn.3).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub models: BTreeSet<String>,
    /// The response whose counters were last charged, by the request id the
    /// records name it with. One response is written as one record per
    /// content block, each restating the same `usage`, so the records
    /// repeating this id are read for everything but their counters
    /// (§FS-013-burn.3). It is carried because a scan can stop between two
    /// blocks of one response, and the next pass would otherwise charge the
    /// rest of it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charged: Option<String>,
    /// The newest timestamp seen in the file, RFC 3339. A record that carries
    /// no time of its own is attributed to this one — it was written after
    /// everything before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// The file is a sub-agent's rather than a session's. Real spend, tagged
    /// so it can be told apart (§FS-013-burn.3).
    #[serde(default, skip_serializing_if = "is_false")]
    pub subagent: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One file's place in the scan.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Cursor {
    /// Bytes already read, always at a line boundary.
    pub offset: u64,
    /// What the file measured when it was last read. With the modification
    /// time, this is what lets an unchanged file go unopened.
    pub size: u64,
    /// Seconds since the epoch.
    #[serde(default)]
    pub mtime: u64,
    #[serde(default)]
    pub carry: Carry,
}

/// Every cursor, as one document beside the buckets.
#[derive(Default, Serialize, Deserialize)]
pub struct Cursors {
    #[serde(default = "version")]
    pub version: u32,
    /// Keyed by the transcript's path.
    #[serde(default)]
    pub files: BTreeMap<String, Cursor>,
}

fn version() -> u32 {
    1
}

/// The file the cursors live in, inside the burn store.
pub fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("cursors.json")
}

/// The cursors as they stand. A store that is not there yet is an empty one:
/// the first scan is a backfill, not a failure (§FS-013-burn.5).
pub fn load(dir: &Path) -> Cursors {
    fs::read_to_string(path(dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the cursors back. Through a temporary file in the same directory, so
/// a scan interrupted half way leaves the previous cursors rather than a
/// truncated document that would re-read every transcript on this machine.
pub fn store(dir: &Path, cursors: &Cursors) -> Result<()> {
    fs::create_dir_all(dir).map_err(|err| crate::error::registry_error(format!("{err}")))?;
    let text = serde_json::to_string(cursors)
        .map_err(|err| crate::error::registry_error(format!("{err}")))?;
    let temporary = path(dir).with_extension("json.new");
    fs::write(&temporary, text).map_err(|err| crate::error::registry_error(format!("{err}")))?;
    fs::rename(&temporary, path(dir)).map_err(|err| crate::error::registry_error(format!("{err}")))
}

/// What one file has gained since the cursor was written.
pub struct Appended {
    /// The appended text, cut at the last complete line.
    pub text: String,
    /// Where the next scan starts.
    pub offset: u64,
    /// How much was actually read off the disk — what makes "the second scan
    /// reads only what was appended" a fact a test can check.
    pub bytes: u64,
    /// The file is not the one the cursor was about: it is shorter than the
    /// offset already read, so it was replaced rather than appended to, and
    /// whatever the cursor carried is about a file that is gone.
    pub restarted: bool,
}

/// Read what `path` has gained, or `None` where it has gained nothing.
///
/// An unchanged size *and* modification time means untouched, and such a file
/// is never opened — which is the whole of what makes a repeat scan cheap. A
/// file shorter than what was already read is read from the start, because a
/// cursor into a file that no longer has those bytes points at nothing.
pub fn appended(file: &Path, cursor: &Cursor) -> Option<Appended> {
    let meta = fs::metadata(file).ok()?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
        .unwrap_or_default();
    if size == cursor.size && mtime == cursor.mtime {
        return None;
    }
    let restarted = size < cursor.offset;
    let start = if restarted { 0 } else { cursor.offset };
    if size <= start {
        // Nothing new, but the stat moved: record the new stat so the next
        // pass can skip the file again.
        return Some(Appended {
            text: String::new(),
            offset: start,
            bytes: 0,
            restarted,
        });
    }
    let mut handle = fs::File::open(file).ok()?;
    handle.seek(SeekFrom::Start(start)).ok()?;
    let mut raw = Vec::with_capacity((size - start) as usize);
    handle.read_to_end(&mut raw).ok()?;
    let bytes = raw.len() as u64;
    // Only whole lines: a record half written when the scan arrived is read
    // on the next pass, in one piece, rather than parsed as two broken ones.
    let complete = match raw.iter().rposition(|byte| *byte == b'\n') {
        Some(at) => at + 1,
        None => 0,
    };
    let text = String::from_utf8_lossy(&raw[..complete]).into_owned();
    Some(Appended {
        text,
        offset: start + complete as u64,
        bytes,
        restarted,
    })
}

/// The stat a cursor records for a file it has just read.
pub fn stat(file: &Path) -> (u64, u64) {
    let Ok(meta) = fs::metadata(file) else {
        return (0, 0);
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
        .unwrap_or_default();
    (meta.len(), mtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, text: &str) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("the transcript opens");
        file.write_all(text.as_bytes()).expect("it appends");
    }

    /// The point of the cursor: a second scan reads the appended bytes and
    /// not one byte more (§FS-013-burn.5). A reader that re-read the file
    /// would work and would cost a gigabyte a pass, which is the failure this
    /// pins — so the assertion is on how much was read, not on what was found.
    #[test]
    fn a_second_scan_reads_only_what_was_appended() {
        let home = tempfile::tempdir().expect("a temporary world");
        let file = home.path().join("session.jsonl");
        write(&file, "{\"a\":1}\n{\"a\":2}\n");
        let first = appended(&file, &Cursor::default()).expect("the first scan reads it");
        assert_eq!(first.bytes, 16);
        assert_eq!(first.offset, 16);
        assert!(!first.restarted);

        let mut cursor = Cursor {
            offset: first.offset,
            ..Cursor::default()
        };
        let (size, mtime) = stat(&file);
        cursor.size = size;
        cursor.mtime = mtime;
        // Untouched: not opened at all.
        assert!(appended(&file, &cursor).is_none());

        write(&file, "{\"a\":3}\n");
        let second = appended(&file, &cursor).expect("the second scan reads the tail");
        assert_eq!(second.bytes, 8, "read {:?}", second.text);
        assert_eq!(second.text, "{\"a\":3}\n");
    }

    /// A record still being written is not half a record: it waits for the
    /// pass that finds its newline.
    #[test]
    fn a_half_written_line_is_left_for_the_next_pass() {
        let home = tempfile::tempdir().expect("a temporary world");
        let file = home.path().join("session.jsonl");
        write(&file, "{\"a\":1}\n{\"a\":2");
        let read = appended(&file, &Cursor::default()).expect("it reads");
        assert_eq!(read.text, "{\"a\":1}\n");
        assert_eq!(read.offset, 8, "the cursor stops at the line boundary");
    }

    /// A file shorter than the offset is a different file wearing the same
    /// name, so it is read from the start and whatever the cursor carried is
    /// dropped by the caller.
    #[test]
    fn a_file_that_shrank_is_read_again_from_the_start() {
        let home = tempfile::tempdir().expect("a temporary world");
        let file = home.path().join("session.jsonl");
        write(&file, "{\"a\":1}\n");
        let cursor = Cursor {
            offset: 4096,
            size: 4096,
            mtime: 1,
            ..Cursor::default()
        };
        let read = appended(&file, &cursor).expect("it reads");
        assert!(read.restarted);
        assert_eq!(read.text, "{\"a\":1}\n");
    }
}
