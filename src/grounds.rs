//! Where the toolchain keeps its own files under a root, and the name they
//! used to have (§FS-006-project-interface.12).
//!
//! `.agents/` is where an agent's *instructions* live, and a runtime that
//! sandboxes a checkout mounts it read-only — so the files the toolchain
//! maintains moved out from under it. Each has a home of its own now, and
//! `.agents/` is the deprecated former name for all of them.
//!
//! This module is the only thing that answers where a root keeps one of them.
//! Looking for one is a single probe in one fixed order, both names go on
//! working, and finding the old one produces one sentence and no other
//! consequence: nothing is refused and no exit code moves. Every surface with
//! something to say about the layout says [`Found::note`] rather than forming
//! a second opinion of its own, which is what lets `doctor` report a project
//! still on the deprecated name while judging nothing itself
//! (§FS-010-doctor.1).
//!
//! ephor reads both names and writes neither: watching a project costs the
//! project nothing, and where a file of the project's own lives is the
//! project's to change (§FS-006-project-interface.12).

use std::path::{Path, PathBuf};

/// The directory the toolchain's own files live in, under a root.
const HOME: &str = ".agent-grounds";

/// The name that directory used to have, and still answers to.
const DEPRECATED: &str = ".agents";

/// One of the toolchain's own files, as the two names it answers to under a
/// root (§FS-006-project-interface.12).
///
/// It is one type over all of them because the three do not share a home: the
/// settings overlay and the file-size budget sit under [`HOME`], the grounding
/// configuration sits at the root itself, and one probe generic over the file
/// it is asked about is what lets the same answer serve a work root and a
/// project checkout alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    home: PathBuf,
    deprecated: PathBuf,
}

/// A file whose home is under `.agent-grounds/`: the runtime settings overlay
/// a work root may carry, and the file-size budget.
pub fn under_the_home(relative: impl AsRef<Path>) -> Kept {
    let relative = relative.as_ref();
    Kept {
        home: Path::new(HOME).join(relative),
        deprecated: Path::new(DEPRECATED).join(relative),
    }
}

/// A file whose home is the root itself — the grounding configuration, which
/// lives there so that a glance says the repository is a grounded tree.
pub fn at_the_root(name: &str) -> Kept {
    Kept {
        home: PathBuf::from(name),
        deprecated: Path::new(DEPRECATED).join(name),
    }
}

/// Everything the toolchain keeps in a project's *checkout*: the grounding
/// configuration at the root, and the file-size budget under the home.
///
/// The runtime settings overlay is not here. It belongs to a work root rather
/// than to a checkout, and is asked for where that root is known
/// (§FS-005-dispatch.14) — a checkout is not the place to guess at one.
pub fn in_a_checkout() -> Vec<Kept> {
    vec![at_the_root("grund.toml"), under_the_home("fissile.toml")]
}

/// What a root turned out to keep, and what there is to say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The file that answered: the home's where it is there, the deprecated
    /// name's otherwise.
    pub path: PathBuf,
    /// The one sentence this layout earns — naming the deprecated file and
    /// where it belongs now, whether it was what answered or was passed over
    /// for the home. None where the home answered alone: a layout that is
    /// already current is not news.
    pub note: Option<String>,
}

impl Kept {
    /// Where this file is under `root` — its home first, the deprecated
    /// `.agents/` name second (§FS-006-project-interface.12). None where the
    /// root carries it under neither name: that root carries the file
    /// nowhere, which is an answer rather than a fault.
    pub fn find(&self, root: &Path) -> Option<Found> {
        let home = root.join(&self.home);
        let deprecated = root.join(&self.deprecated);
        // A root carrying both is answered by the home and the deprecated one
        // is ignored rather than merged: reading both would make the answer
        // depend on an order nobody wrote down.
        if home.is_file() {
            return Some(Found {
                note: deprecated.is_file().then(|| {
                    format!(
                        "{} answered, so {} was passed over — one file under two names is a \
                         tie, and the home wins",
                        home.display(),
                        deprecated.display()
                    )
                }),
                path: home,
            });
        }
        deprecated.is_file().then(|| Found {
            note: Some(format!(
                "{} is the deprecated name and still answers; the file belongs at {} now",
                deprecated.display(),
                home.display()
            )),
            path: deprecated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch root")
    }

    fn write(root: &Path, relative: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make the directory");
        std::fs::write(&path, "x").expect("write the file");
    }

    /// A root carrying nothing carries the file nowhere, which is an answer
    /// (§FS-006-project-interface.12).
    #[test]
    fn a_root_carrying_neither_name_answers_nothing() {
        let dir = root();
        assert_eq!(under_the_home("fissile.toml").find(dir.path()), None);
        assert_eq!(at_the_root("grund.toml").find(dir.path()), None);
    }

    /// The home answers, and a layout that is already current is not news.
    #[test]
    fn the_home_answers_and_says_nothing() {
        let dir = root();
        write(dir.path(), ".agent-grounds/fissile.toml");
        let found = under_the_home("fissile.toml")
            .find(dir.path())
            .expect("the home answered");
        assert_eq!(found.path, dir.path().join(".agent-grounds/fissile.toml"));
        assert_eq!(found.note, None);
    }

    /// The deprecated name goes on working, and the sentence names both the
    /// file that was read and where it belongs now.
    #[test]
    fn the_deprecated_name_answers_and_says_where_it_moved() {
        let dir = root();
        write(dir.path(), ".agents/grund.toml");
        let found = at_the_root("grund.toml")
            .find(dir.path())
            .expect("the deprecated name answered");
        assert_eq!(found.path, dir.path().join(".agents/grund.toml"));
        let note = found.note.expect("reading the old name is said");
        assert!(note.contains(".agents/grund.toml"), "{note}");
        assert!(note.contains("grund.toml now"), "{note}");
    }

    /// Two names for one file is a tie, and the home wins — the other is
    /// passed over rather than merged, and the reader is told it was.
    #[test]
    fn a_root_carrying_both_is_answered_by_the_home() {
        let dir = root();
        write(dir.path(), ".agent-grounds/fissile.toml");
        write(dir.path(), ".agents/fissile.toml");
        let found = under_the_home("fissile.toml")
            .find(dir.path())
            .expect("the home answered");
        assert_eq!(found.path, dir.path().join(".agent-grounds/fissile.toml"));
        let note = found.note.expect("the one passed over is said");
        assert!(note.contains(".agents/fissile.toml"), "{note}");
        assert!(note.contains("passed over"), "{note}");
    }
}
