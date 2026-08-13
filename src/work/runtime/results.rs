//! What a run left behind, read back out of the work root
//! (§AR-007-runtime.1).
//!
//! Two things are read: the verdict line a finished ticket wrote, and the
//! **proposed answer** a ticket about a conversation wrote instead of posting
//! one (§FS-005-dispatch.13). Both are files the runtime's own states produce,
//! found by where they sit and read for what they say — nothing here writes
//! work state, which stays the runtime's (§FS-005-dispatch.4).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{EphorError, Result};

/// Where the shipped states put what a run wrote, relative to the work root.
const ARTIFACTS: &str = "runtime/ephor";

/// The verdict a finished ticket left behind, as the state machine ephor ships
/// asks for it. Found by what it says rather than by where it sits: an agent
/// asked for a document writes a document, and its first line is a heading.
/// Absent while the work has not reached that state, which is not a failure.
pub fn verdict(root: &Path, plan_id: &str, ticket: &str) -> Option<String> {
    let path = root
        .join(ARTIFACTS)
        .join(format!("{plan_id}.{ticket}.verdict.md"));
    let text = fs::read_to_string(path).ok()?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("VERDICT:"))
        .map(|line| line.trim_start_matches("VERDICT:").trim())
        .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))?;
    Some(line.to_string())
}

/// A reply a run drafted and did not send (§FS-005-dispatch.13). It is a file
/// and stays one until a person posts it: the proposal is materials, never an
/// act (§REQ-001-boundary.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The reply as it would be posted, exactly as the run wrote it.
    pub text: String,
    /// Where it sits — what the reader copies from where nothing can post it,
    /// and what they edit before posting where something can.
    pub path: PathBuf,
}

/// Where a plan's proposed answer belongs. One per matter rather than one per
/// ticket: what a reader posts is the answer to the conversation, and a second
/// pass over the same conversation supersedes the first rather than adding to
/// it.
pub fn reply_path(root: &Path, plan_id: &str) -> PathBuf {
    root.join(ARTIFACTS).join(format!("{plan_id}.reply.md"))
}

/// Where a posted proposal is moved to. Posting is the one deliberate move
/// (§FS-005-dispatch.13), and a proposal that stayed offered after it was sent
/// would invite sending it twice.
fn posted_path(root: &Path, plan_id: &str) -> PathBuf {
    root.join(ARTIFACTS)
        .join(format!("{plan_id}.reply.posted.md"))
}

/// The proposed answer a run left for this plan, or None where it left none —
/// which is every ticket that was not about a conversation, and every answer
/// ticket that has not finished.
pub fn proposal(root: &Path, plan_id: &str) -> Option<Proposal> {
    let path = reply_path(root, plan_id);
    let text = fs::read_to_string(&path).ok()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(Proposal {
        text: text.to_string(),
        path,
    })
}

/// Record that a proposal was posted, by moving it aside. The file is kept
/// rather than deleted: it is what was said in the reader's name.
pub fn mark_posted(root: &Path, plan_id: &str) -> Result<()> {
    let from = reply_path(root, plan_id);
    if !from.is_file() {
        return Ok(());
    }
    fs::rename(&from, posted_path(root, plan_id))
        .map_err(|err| EphorError::Command(format!("Cannot move {}: {err}", from.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifacts(root: &Path) -> PathBuf {
        let dir = root.join(ARTIFACTS);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_verdict_is_read_back_without_its_label() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = artifacts(tmp.path());
        // As an agent asked for a document actually writes one: a heading
        // first, and the verdict in the body.
        fs::write(
            dir.join("widget-42.fix-gate-1.verdict.md"),
            "# widget-42.fix-gate-1 — review verdict\n\n\
             VERDICT: blocked — the failing job needs a credential\n\n## What was done\n",
        )
        .unwrap();
        assert_eq!(
            verdict(tmp.path(), "widget-42", "fix-gate-1").as_deref(),
            Some("blocked — the failing job needs a credential")
        );
        assert!(verdict(tmp.path(), "widget-42", "nothing-1").is_none());
    }

    /// The proposal is read whole: it is a reply, and a reply summarized is a
    /// different reply (§FS-005-dispatch.13).
    #[test]
    fn a_proposed_reply_is_read_whole_and_says_where_it_sits() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = artifacts(tmp.path());
        assert_eq!(proposal(tmp.path(), "widget-42"), None);

        fs::write(
            dir.join("widget-42.reply.md"),
            "\nYes — the retry window is per attempt.\n\nThe test covers it.\n\n",
        )
        .unwrap();
        let found = proposal(tmp.path(), "widget-42").expect("a reply was drafted");
        assert_eq!(
            found.text,
            "Yes — the retry window is per attempt.\n\nThe test covers it."
        );
        assert_eq!(found.path, reply_path(tmp.path(), "widget-42"));

        // A file the run created and wrote nothing into is not a proposal.
        fs::write(dir.join("widget-43.reply.md"), "   \n\n").unwrap();
        assert_eq!(proposal(tmp.path(), "widget-43"), None);
    }

    /// Posted once: what was sent is kept, and what is left is not offered
    /// again (§FS-005-dispatch.13).
    #[test]
    fn posting_a_proposal_moves_it_aside_and_keeps_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = artifacts(tmp.path());
        fs::write(dir.join("widget-42.reply.md"), "posted words").unwrap();

        mark_posted(tmp.path(), "widget-42").unwrap();
        assert_eq!(proposal(tmp.path(), "widget-42"), None);
        assert_eq!(
            fs::read_to_string(posted_path(tmp.path(), "widget-42")).unwrap(),
            "posted words"
        );
        // Nothing to move is not an error: the reader may have posted from
        // another surface, or the run may never have written one.
        mark_posted(tmp.path(), "widget-42").unwrap();
    }
}
