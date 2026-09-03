//! What a command was given, and the name to refuse it under
//! (§FS-011-command-line.9).
//!
//! Every value `ephor checkout` and `ephor rebase` take arrives in one of two
//! spellings — the flag a reader types, and the name a program state sets in
//! the environment for that flag — because the key a reader presses and the
//! command line are one operation (§FS-005-dispatch.12). One home for both, so
//! a value is honoured or refused the same way whichever way it came in, and
//! the refusal names the spelling it actually arrived in.
//!
//! Absent, and empty once trimmed, are *nothing given*: the command falls
//! through to whatever it does without the value, which is what a state
//! machine handing a program `BRANCH: "{meta.branch}"` about a matter with no
//! branch depends on. Anything else is a value, and a value the command cannot
//! act on is refused rather than dropped into the path the command would have
//! taken without it — a caller who passed `--branch` and is told that nothing
//! said which branch to check out has been answered about a command they did
//! not run, and cannot tell the two apart from the output.

use crate::error::{EphorError, Result};

/// A value as it arrived, and the input it came in on: the flag where a reader
/// typed one, the environment name where a program state set it.
struct Said {
    value: String,
    input: String,
}

/// The value this input carries, or nothing given.
///
/// The flag wins where both are there, which is what makes the environment the
/// spelling of a flag rather than a second input: a state that sets `BRANCH`
/// and a reader who types `--branch` in the same run asked for one thing.
fn said(flag: &Option<String>, name: &str) -> Option<Said> {
    let (value, input) = match flag {
        Some(value) => (value.clone(), format!("--{}", name.to_ascii_lowercase())),
        None => (std::env::var(name).ok()?, name.to_string()),
    };
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(Said { value, input })
}

impl Said {
    /// Refused, in the shape a configuration refusal takes on this command line
    /// (§FS-011-command-line.9): the input and what it held, exiting 2 and
    /// landing on standard output as an outcome under `--json`, which is what
    /// [`EphorError::Registry`] already means here.
    fn refuse(&self, why: &str) -> EphorError {
        EphorError::Registry(format!(
            "{} was given '{}', which {why}.",
            self.input, self.value
        ))
    }
}

/// A value this command can carry as it stands: anything but a placeholder
/// whose runtime never filled it.
///
/// A `{` left in a value is a `{meta.*}` nothing answered, and the name it
/// stands for is not a project, an item, a path or a hand — the same reading
/// [`crate::forest::is_branch_name`] makes, taken here without its branch half,
/// which has no business being asked of a project id.
pub fn value(flag: &Option<String>, name: &str) -> Result<Option<String>> {
    let Some(said) = said(flag, name) else {
        return Ok(None);
    };
    if said.value.contains('{') {
        return Err(said.refuse("is a placeholder nothing filled, rather than a value"));
    }
    Ok(Some(said.value))
}

/// A value that has to be a branch name, held to what git will take as one
/// (§FS-004-quick-actions.7.3).
///
/// Asked here rather than left to the working tree that would be added under
/// it: git's own refusal arrives from inside the making, by which time the
/// directories leading down to the workspace are on disk, so a name that
/// reached above the project has already left them there.
pub fn branch(flag: &Option<String>, name: &str) -> Result<Option<String>> {
    let Some(said) = said(flag, name) else {
        return Ok(None);
    };
    if !crate::forest::is_branch_name(&said.value) {
        return Err(said.refuse("is a placeholder nothing filled, rather than a branch name"));
    }
    if let Some(why) = crate::branches::why_git_refuses(&said.value) {
        return Err(said.refuse(&format!("git will not take as a branch name: {why}")));
    }
    Ok(Some(said.value))
}
