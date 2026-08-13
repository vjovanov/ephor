//! The git ephor does itself: how far a checkout trails its main branch,
//! replaying it there (§FS-004-quick-actions.6), and making the workspace that
//! is not there yet (§FS-004-quick-actions.7).
//!
//! Nothing here knows what a forge is, or what the project is built with. A
//! rebase is a fetch, a replay, and an answer per repository — the same
//! operation on every project ephor watches. The reader's key and the state
//! machine's program state both run this one implementation, because two of
//! them would eventually disagree about what a clean rebase is
//! (§FS-005-dispatch.12).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::forest::{under, Forest};

/// What became of one repository.
#[derive(Debug, Clone, PartialEq)]
pub enum Replay {
    /// Already on top of the base — reported, not skipped.
    Current,
    /// Replayed onto the base; it had trailed by this many commits.
    Rebased(u64),
    /// Stopped in a conflict. The repository is left mid-rebase, which is the
    /// state resolving it needs (§FS-005-dispatch.12); these paths are unmerged.
    Conflicted(Vec<String>),
    /// Uncommitted work, so nothing was touched (§FS-004-quick-actions.6).
    Dirty(Vec<String>),
    /// git would not, and this is what it said.
    Refused(String),
}

impl Replay {
    /// Whether this repository is now on top of the base.
    pub fn is_clean(&self) -> bool {
        matches!(self, Replay::Current | Replay::Rebased(_))
    }
}

#[derive(Debug, Clone)]
pub struct RepoReplay {
    /// The repository's path relative to the checkout (`.` for the root).
    pub repo: String,
    /// The branch it was on, where git could say.
    pub branch: Option<String>,
    pub replay: Replay,
}

/// One rebase of one checkout: every repository under it, in order.
#[derive(Debug, Clone)]
pub struct Rebase {
    pub checkout: PathBuf,
    pub base: String,
    pub repos: Vec<RepoReplay>,
}

impl Rebase {
    pub fn conflicted(&self) -> Vec<&RepoReplay> {
        self.repos
            .iter()
            .filter(|repo| matches!(repo.replay, Replay::Conflicted(_)))
            .collect()
    }

    /// Repositories that were not replayed and are not conflicts: something a
    /// person has to clear before the rebase can be tried again.
    pub fn stuck(&self) -> Vec<&RepoReplay> {
        self.repos
            .iter()
            .filter(|repo| matches!(repo.replay, Replay::Dirty(_) | Replay::Refused(_)))
            .collect()
    }

    pub fn rebased(&self) -> usize {
        self.repos
            .iter()
            .filter(|repo| matches!(repo.replay, Replay::Rebased(_)))
            .count()
    }

    /// One line for a menu or a log: what happened, counted.
    pub fn summary(&self) -> String {
        if self.repos.is_empty() {
            return format!("no git repository under {}", self.checkout.display());
        }
        let conflicted = self.conflicted().len();
        let stuck = self.stuck().len();
        let rebased = self.rebased();
        let mut parts = Vec::new();
        if rebased > 0 {
            parts.push(format!("{rebased} rebased onto {}", self.base));
        }
        if conflicted > 0 {
            parts.push(format!("{conflicted} in conflict"));
        }
        if stuck > 0 {
            parts.push(format!("{stuck} left alone"));
        }
        if parts.is_empty() {
            return format!("already on {}", self.base);
        }
        parts.join(", ")
    }

    /// The whole outcome as markdown — what a state machine hands the agent
    /// that resolves it, and what the reader sees on their terminal.
    pub fn report(&self) -> String {
        let mut out = format!(
            "# rebase onto {} in {}\n\n",
            self.base,
            self.checkout.display()
        );
        if self.repos.is_empty() {
            out.push_str("No git repository under the checkout — nothing was done.\n");
            return out;
        }
        for repo in &self.repos {
            let branch = repo.branch.as_deref().unwrap_or("(unknown branch)");
            out.push_str(&format!("## {} — {branch}\n\n", repo.repo));
            match &repo.replay {
                Replay::Current => {
                    out.push_str(&format!("Already on top of `origin/{}`.\n\n", self.base));
                }
                Replay::Rebased(commits) => {
                    out.push_str(&format!(
                        "Replayed onto `origin/{}`; it had trailed by {commits} commit(s).\n\n",
                        self.base
                    ));
                }
                Replay::Conflicted(files) => {
                    out.push_str(
                        "**The rebase stopped in a conflict.** The repository is left \
                         mid-rebase, with these paths unmerged:\n\n",
                    );
                    for file in files {
                        out.push_str(&format!("- `{file}`\n"));
                    }
                    out.push_str(&format!(
                        "\nResolve them in `{}`, `git add` each one, then \
                         `git rebase --continue`.\n\n",
                        self.checkout.join(&repo.repo).display()
                    ));
                }
                Replay::Dirty(paths) => {
                    out.push_str(
                        "Uncommitted work — nothing was touched here. Commit it or stash it, \
                         then rebase again.\n\n```\n",
                    );
                    for path in paths {
                        out.push_str(&format!("{path}\n"));
                    }
                    out.push_str("```\n\n");
                }
                Replay::Refused(message) => {
                    out.push_str(&format!("git refused:\n\n```\n{message}\n```\n\n"));
                }
            }
        }
        out
    }
}

/// Whether the path is a git working tree. git answers `false` — successfully
/// — from inside a `.git` directory, so the answer is read rather than the
/// exit status: a repository's own metadata is not a second repository.
pub fn is_work_tree(path: &Path) -> bool {
    path.is_dir()
        && git(path, &["rev-parse", "--is-inside-work-tree"])
            .is_some_and(|answer| answer.trim() == "true")
}

/// Commits `repo`'s HEAD is behind the base, preferring the last-fetched
/// `origin/<base>`; None when not a git repository or no usable ref.
pub fn commits_behind(repo: &Path, base: &str) -> Option<u64> {
    if !is_work_tree(repo) {
        return None;
    }
    for reference in [format!("origin/{base}"), base.to_string()] {
        if let Some(count) = git(
            repo,
            &["rev-list", "--count", &format!("HEAD..{reference}")],
        ) {
            return count.trim().parse().ok();
        }
    }
    None
}

/// What this repository's origin calls its default branch, where it recorded
/// one. The fallback for a checkout no registry entry describes.
pub fn default_base(repo: &Path) -> Option<String> {
    let head = git(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )?;
    let name = head.trim().strip_prefix("origin/")?.to_string();
    (!name.is_empty()).then_some(name)
}

/// Whether this path is a repository in its own right, rather than a
/// directory inside one. `--is-inside-work-tree` answers `true` from every
/// subdirectory of a checkout, so probing on it alone would report `docs/` and
/// `src/` as repositories and then count the same repository's commits once
/// per subdirectory. A repository is its own toplevel.
fn is_repository_root(path: &Path) -> bool {
    if !is_work_tree(path) {
        return false;
    }
    let Some(toplevel) = git(path, &["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    let toplevel = PathBuf::from(toplevel.trim());
    // Compare through the filesystem: a temporary directory reached by a
    // symlink is one path to git and another to the caller.
    match (toplevel.canonicalize(), path.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => toplevel == path,
    }
}

/// Every repository at or directly under a checkout, in order: the checkout
/// itself first, then its children by name. This is the probe half of
/// §AR-004-forest.2 — what a forest is when nothing declared one — and the
/// only place on-disk discovery happens.
pub fn probe(checkout: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut push = |path: PathBuf| {
        if is_repository_root(&path) && !found.contains(&path) {
            found.push(path);
        }
    };
    push(checkout.to_path_buf());
    let mut children: Vec<PathBuf> = std::fs::read_dir(checkout)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        })
        .collect();
    children.sort();
    for child in children {
        push(child);
    }
    found
}

/// Replay every repository of the forest onto `base` — a fold with one answer
/// per repository (§AR-004-forest.1).
pub fn rebase(forest: &Forest, base: &str) -> Rebase {
    let outcomes = forest
        .repos
        .iter()
        .map(|repo| RepoReplay {
            repo: repo.name.clone(),
            branch: git(&repo.path, &["rev-parse", "--abbrev-ref", "HEAD"])
                .map(|name| name.trim().to_string()),
            replay: replay_one(&repo.path, base),
        })
        .collect();
    Rebase {
        checkout: forest.root.clone(),
        base: base.to_string(),
        repos: outcomes,
    }
}

fn replay_one(repo: &Path, base: &str) -> Replay {
    // A repository already stopped in a rebase is a conflict to finish, not a
    // rebase to start: starting a second one over it would lose the first.
    if let Some(files) = unmerged(repo) {
        if !files.is_empty() {
            return Replay::Conflicted(files);
        }
    }
    match git(repo, &["status", "--porcelain", "--untracked-files=no"]) {
        Some(status) if !status.trim().is_empty() => {
            return Replay::Dirty(
                status
                    .lines()
                    .map(|line| line.trim_end().to_string())
                    .collect(),
            )
        }
        None => return Replay::Refused("git status failed".to_string()),
        _ => {}
    }

    if let Err(message) = run(repo, &["fetch", "origin", "--prune"]) {
        return Replay::Refused(message);
    }
    let reference = format!("origin/{base}");
    if git(repo, &["rev-parse", "--verify", "--quiet", &reference]).is_none() {
        return Replay::Refused(format!(
            "no '{reference}' — the base branch is not on this repository's origin"
        ));
    }
    let behind = commits_behind(repo, base).unwrap_or(0);
    if behind == 0 {
        return Replay::Current;
    }
    match run(repo, &["rebase", &reference]) {
        Ok(_) => Replay::Rebased(behind),
        Err(message) => match unmerged(repo) {
            Some(files) if !files.is_empty() => Replay::Conflicted(files),
            // A rebase that failed without leaving a conflict left nothing to
            // resolve; whatever git said is the whole answer.
            _ => Replay::Refused(message),
        },
    }
}

/// What became of one repository of a workspace being checked out.
#[derive(Debug, Clone, PartialEq)]
pub enum Created {
    /// A working tree on the branch itself, which the repository already had.
    Tracking,
    /// The repository does not have that branch, so it was grown from the base
    /// named here — what a change touching one repository of a tree looks like
    /// (§FS-004-quick-actions.7).
    Branched(String),
    /// A working tree was already there; reported rather than skipped.
    Present,
    /// git would not, and this is what it said. A branch another working tree
    /// is holding is the common one, and git is right to refuse it.
    Refused(String),
}

impl Created {
    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            Created::Tracking | Created::Branched(_) | Created::Present
        )
    }
}

#[derive(Debug, Clone)]
pub struct RepoCreated {
    /// The repository's path relative to the checkout (`.` for the root).
    pub repo: String,
    pub created: Created,
}

/// One workspace being made: every repository that belongs under it, in order.
#[derive(Debug, Clone)]
pub struct Creation {
    pub target: PathBuf,
    pub branch: String,
    pub repos: Vec<RepoCreated>,
}

impl Creation {
    pub fn refused(&self) -> Vec<&RepoCreated> {
        self.repos
            .iter()
            .filter(|repo| matches!(repo.created, Created::Refused(_)))
            .collect()
    }

    /// Every repository has a working tree, so the workspace is usable.
    pub fn is_ready(&self) -> bool {
        !self.repos.is_empty() && self.repos.iter().all(|repo| repo.created.is_ready())
    }

    pub fn summary(&self) -> String {
        if self.repos.is_empty() {
            return format!("nothing to check out into {}", self.target.display());
        }
        let mut tracking = 0;
        let mut branched = 0;
        let mut present = 0;
        for repo in &self.repos {
            match repo.created {
                Created::Tracking => tracking += 1,
                Created::Branched(_) => branched += 1,
                Created::Present => present += 1,
                Created::Refused(_) => {}
            }
        }
        let mut parts = Vec::new();
        if tracking > 0 {
            parts.push(format!("{tracking} on {}", self.branch));
        }
        if branched > 0 {
            parts.push(format!("{branched} branched"));
        }
        if present > 0 {
            parts.push(format!("{present} already there"));
        }
        let refused = self.refused().len();
        if refused > 0 {
            parts.push(format!("{refused} refused"));
        }
        parts.join(", ")
    }

    /// The whole outcome as markdown, for the same two readers the rebase has.
    pub fn report(&self) -> String {
        let mut out = format!(
            "# check out {} into {}\n\n",
            self.branch,
            self.target.display()
        );
        if self.repos.is_empty() {
            out.push_str(
                "No repository to check out — the project says it has none, and the \
                 source checkout holds none.\n",
            );
            return out;
        }
        for repo in &self.repos {
            out.push_str(&format!("## {}\n\n", repo.repo));
            match &repo.created {
                Created::Tracking => out.push_str(&format!(
                    "A working tree on `{}`, tracking the branch the forge has.\n\n",
                    self.branch
                )),
                Created::Branched(base) => out.push_str(&format!(
                    "The repository has no `{}`, so it was started from `origin/{base}`.\n\n",
                    self.branch
                )),
                Created::Present => {
                    out.push_str("A working tree was already here; nothing was touched.\n\n")
                }
                Created::Refused(message) => {
                    out.push_str(&format!("git refused:\n\n```\n{message}\n```\n\n"))
                }
            }
        }
        out
    }
}

/// Make the branch workspace at `target`, one working tree per repository,
/// from an existing checkout of the same project (§FS-004-quick-actions.7).
///
/// `source` is a checkout that is already on disk — the main branch's, usually
/// — because a working tree is added *from* a repository and there is nothing
/// else to add it from. `base` is what a repository that does not have the
/// branch grows one from.
pub fn create(
    source: &Path,
    target: &Path,
    layout: &[String],
    branch: &str,
    base: &str,
) -> Creation {
    let repos = layout
        .iter()
        .map(|name: &String| RepoCreated {
            repo: name.clone(),
            created: create_one(&under(source, name), &under(target, name), branch, base),
        })
        .collect();

    Creation {
        target: target.to_path_buf(),
        branch: branch.to_string(),
        repos,
    }
}

fn create_one(source: &Path, target: &Path, branch: &str, base: &str) -> Created {
    if is_work_tree(target) {
        return Created::Present;
    }
    if target.exists() {
        return Created::Refused(format!(
            "{} is in the way and is not a git working tree",
            target.display()
        ));
    }
    if !is_work_tree(source) {
        return Created::Refused(format!("no repository at {}", source.display()));
    }
    if let Some(parent) = target.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            return Created::Refused(format!("cannot create {}: {err}", parent.display()));
        }
    }
    // The branch as the forge has it now, not as this clone last saw it: a
    // pull request opened since the last fetch is the ordinary case.
    if let Err(message) = run(source, &["fetch", "origin", "--prune"]) {
        return Created::Refused(message);
    }

    let target = target.to_string_lossy().into_owned();
    let local = git(
        source,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_some();
    if local {
        // Already a branch here — check it out into the new tree as it stands.
        // git refuses if another working tree holds it, which is the answer.
        return match run(source, &["worktree", "add", &target, branch]) {
            Ok(_) => Created::Tracking,
            Err(message) => Created::Refused(message),
        };
    }

    let remote = format!("refs/remotes/origin/{branch}");
    if git(source, &["rev-parse", "--verify", "--quiet", &remote]).is_some() {
        let start = format!("origin/{branch}");
        return match run(
            source,
            &["worktree", "add", "--track", "-b", branch, &target, &start],
        ) {
            Ok(_) => Created::Tracking,
            Err(message) => Created::Refused(message),
        };
    }

    // Not this repository's branch. In a poly-repo workspace that is the
    // normal case for every repository the change does not touch, and they
    // still have to be there — on a branch of the same name, off the base.
    let start = format!("origin/{base}");
    if git(source, &["rev-parse", "--verify", "--quiet", &start]).is_none() {
        return Created::Refused(format!(
            "neither '{branch}' nor '{start}' is on this repository — nothing to grow a \
             working tree from"
        ));
    }
    match run(source, &["worktree", "add", "-b", branch, &target, &start]) {
        Ok(_) => Created::Branched(base.to_string()),
        Err(message) => Created::Refused(message),
    }
}

/// Paths git reports as unmerged, or None when it could not say.
fn unmerged(repo: &Path) -> Option<Vec<String>> {
    git(repo, &["diff", "--name-only", "--diff-filter=U"]).map(|out| {
        out.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect()
    })
}

/// A git command whose output is the answer; None when it fails.
fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A git command run for what it does. The error carries what git said,
/// because a refusal the reader cannot read is a refusal they cannot act on.
fn run(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| format!("cannot run git: {err}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        Ok(combined)
    } else {
        Err(combined.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_in(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
        std::fs::write(dir.join(file), contents).unwrap();
        run_in(dir, &["add", file]);
        run_in(dir, &["commit", "-m", message]);
    }

    /// An "origin" with a `master` and a `feature` branch checked out into a
    /// clone, which is the shape every case here starts from.
    fn checkout_with_origin(root: &Path, name: &str) -> PathBuf {
        let origin = root.join(format!("{name}.git"));
        std::fs::create_dir_all(&origin).unwrap();
        run_in(&origin, &["init", "--initial-branch=master", "-q"]);
        run_in(&origin, &["config", "user.email", "t@example.com"]);
        run_in(&origin, &["config", "user.name", "t"]);
        commit(&origin, "shared.txt", "one\n", "one");

        let clone = root.join("work").join(name);
        std::fs::create_dir_all(clone.parent().unwrap()).unwrap();
        let status = Command::new("git")
            .args(["clone", "-q"])
            .arg(&origin)
            .arg(&clone)
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        run_in(&clone, &["config", "user.email", "t@example.com"]);
        run_in(&clone, &["config", "user.name", "t"]);
        run_in(&clone, &["checkout", "-q", "-b", "feature"]);
        commit(&clone, "mine.txt", "mine\n", "mine");
        clone
    }

    /// Move origin/master on, so the clone's feature branch trails it.
    fn advance_master(root: &Path, name: &str, file: &str, contents: &str) {
        let origin = root.join(format!("{name}.git"));
        commit(&origin, file, contents, "master moves");
    }

    #[test]
    fn a_branch_that_trails_is_replayed_and_one_that_does_not_is_current() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");

        let replayed = super::rebase(&Forest::resolve(&checkout, None, &[]), "master");
        assert_eq!(replayed.repos.len(), 1);
        assert_eq!(replayed.repos[0].replay, Replay::Rebased(1));
        assert_eq!(replayed.repos[0].branch.as_deref(), Some("feature"));
        // The reader's commit survived the replay, on top of theirs.
        assert!(checkout.join("mine.txt").exists());
        assert!(checkout.join("theirs.txt").exists());

        // Immediately again: there is nothing left to replay, and that is an
        // answer rather than a no-op (§FS-004-quick-actions.6).
        let again = super::rebase(&Forest::resolve(&checkout, None, &[]), "master");
        assert_eq!(again.repos[0].replay, Replay::Current);
        assert_eq!(again.summary(), "already on master");
    }

    #[test]
    fn a_conflict_is_left_in_the_working_tree_with_its_files_named() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        // Both sides change the same file: the replay cannot decide.
        commit(&checkout, "shared.txt", "ours\n", "ours");
        advance_master(temp.path(), "app", "shared.txt", "theirs\n");

        let stopped = super::rebase(&Forest::resolve(&checkout, None, &[]), "master");
        assert_eq!(
            stopped.repos[0].replay,
            Replay::Conflicted(vec!["shared.txt".to_string()])
        );
        assert_eq!(stopped.conflicted().len(), 1);
        assert!(stopped.report().contains("mid-rebase"));
        // Left where the algorithm stopped, which is what resolving it needs.
        assert!(
            checkout.join(".git/rebase-merge").exists()
                || checkout.join(".git/rebase-apply").exists()
        );

        // Asked again, it reports the conflict it is standing in rather than
        // starting a second rebase over the first.
        let again = super::rebase(&Forest::resolve(&checkout, None, &[]), "master");
        assert!(matches!(again.repos[0].replay, Replay::Conflicted(_)));
    }

    #[test]
    fn uncommitted_work_is_reported_and_left_alone() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");
        std::fs::write(checkout.join("mine.txt"), "half-written\n").unwrap();

        let refused = super::rebase(&Forest::resolve(&checkout, None, &[]), "master");
        match &refused.repos[0].replay {
            Replay::Dirty(paths) => assert!(paths[0].contains("mine.txt")),
            other => panic!("expected Dirty, got {other:?}"),
        }
        // Nothing was replayed, so the branch still trails and the half-written
        // file is still half-written.
        assert_eq!(
            std::fs::read_to_string(checkout.join("mine.txt")).unwrap(),
            "half-written\n"
        );
        assert!(!checkout.join("theirs.txt").exists());
        assert_eq!(refused.stuck().len(), 1);
    }

    #[test]
    fn every_repository_in_a_workspace_is_replayed() {
        let temp = tempfile::tempdir().unwrap();
        let ce = checkout_with_origin(temp.path(), "ce");
        let _ee = checkout_with_origin(temp.path(), "ee");
        let workspace = ce.parent().unwrap().to_path_buf();
        advance_master(temp.path(), "ce", "theirs.txt", "theirs\n");
        advance_master(temp.path(), "ee", "theirs.txt", "theirs\n");
        // A directory that is not a repository is not one of the answers.
        std::fs::create_dir_all(workspace.join("notes")).unwrap();

        let both = super::rebase(&Forest::resolve(&workspace, None, &[]), "master");
        let names: Vec<&str> = both.repos.iter().map(|r| r.repo.as_str()).collect();
        assert_eq!(names, ["ce", "ee"]);
        assert!(both.repos.iter().all(|r| r.replay == Replay::Rebased(1)));
        assert_eq!(both.summary(), "2 rebased onto master");

        // The registry's own repo list wins where there is one.
        let named = super::rebase(
            &Forest::resolve(&workspace, None, &[crate::forest::Declaration::at("ce")]),
            "master",
        );
        assert_eq!(named.repos.len(), 1);
    }

    #[test]
    fn a_base_branch_that_is_not_on_origin_is_refused_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let refused = super::rebase(&Forest::resolve(&checkout, None, &[]), "trunk");
        match &refused.repos[0].replay {
            Replay::Refused(message) => assert!(message.contains("origin/trunk")),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    /// The branch the forge has is checked out; a repository that does not
    /// have it gets one of the same name off the base, which is what a change
    /// touching one repository of a tree looks like (§FS-004-quick-actions.7).
    #[test]
    fn a_workspace_is_made_with_the_branch_where_it_exists_and_grown_where_it_does_not() {
        let temp = tempfile::tempdir().unwrap();
        let ce = checkout_with_origin(temp.path(), "ce");
        let ee = checkout_with_origin(temp.path(), "ee");
        let source = ce.parent().unwrap().to_path_buf();
        // Only `ce`'s origin has the change's branch; `ee` is a repository the
        // change does not touch, so it sits on the base with no such branch.
        run_in(&ce, &["push", "-q", "origin", "feature"]);
        for repo in [&ce, &ee] {
            run_in(repo, &["checkout", "-q", "master"]);
            run_in(repo, &["branch", "-q", "-D", "feature"]);
        }

        let target = temp.path().join("ws").join("feature");
        let made = super::create(
            &source,
            &target,
            &Forest::resolve(&source, None, &[]).layout,
            "feature",
            "master",
        );

        let names: Vec<&str> = made.repos.iter().map(|repo| repo.repo.as_str()).collect();
        assert_eq!(names, ["ce", "ee"]);
        assert_eq!(made.repos[0].created, Created::Tracking);
        assert_eq!(
            made.repos[1].created,
            Created::Branched("master".to_string())
        );
        assert!(made.is_ready());
        assert!(made.refused().is_empty());

        // Both repositories are on disk, on the same branch name.
        for repo in ["ce", "ee"] {
            let path = target.join(repo);
            assert!(is_work_tree(&path), "{repo} is not a working tree");
            assert_eq!(
                git(&path, &["rev-parse", "--abbrev-ref", "HEAD"])
                    .unwrap()
                    .trim(),
                "feature"
            );
        }
        // The one that has the change carries its commit; the other does not.
        assert!(target.join("ce/mine.txt").exists());
        assert!(!target.join("ee/mine.txt").exists());
    }

    #[test]
    fn a_branch_another_working_tree_holds_is_refused_and_named() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let source = checkout.parent().unwrap().to_path_buf();
        // `feature` is checked out in the source itself, so git will not hand
        // it to a second working tree — and it is right not to.
        let target = temp.path().join("ws").join("feature");
        let made = super::create(
            &source,
            &target,
            &Forest::resolve(&source, None, &[]).layout,
            "feature",
            "master",
        );

        match &made.repos[0].created {
            Created::Refused(message) => assert!(message.contains("feature")),
            other => panic!("expected Refused, got {other:?}"),
        }
        assert!(!made.is_ready());
        assert_eq!(made.refused().len(), 1);
        assert!(made.report().contains("git refused"));
    }

    /// Asked twice, the second is not an error: it says what is already there.
    #[test]
    fn a_workspace_that_is_already_there_is_reported_not_remade() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let source = checkout.parent().unwrap().to_path_buf();
        run_in(&checkout, &["checkout", "-q", "master"]);

        let target = temp.path().join("ws").join("feature");
        let first = super::create(
            &source,
            &target,
            &Forest::resolve(&source, None, &[]).layout,
            "feature",
            "master",
        );
        assert_eq!(first.repos[0].created, Created::Tracking);

        let again = super::create(
            &source,
            &target,
            &Forest::resolve(&source, None, &[]).layout,
            "feature",
            "master",
        );
        assert_eq!(again.repos[0].created, Created::Present);
        assert!(again.is_ready());
        assert_eq!(again.summary(), "1 already there");
    }

    #[test]
    fn a_repository_with_neither_the_branch_nor_the_base_is_refused_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let source = checkout.parent().unwrap().to_path_buf();
        let target = temp.path().join("ws").join("nope");
        let made = super::create(
            &source,
            &target,
            &Forest::resolve(&source, None, &[]).layout,
            "nope",
            "trunk",
        );
        match &made.repos[0].created {
            Created::Refused(message) => {
                assert!(message.contains("nope") && message.contains("origin/trunk"))
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn behind_counts_against_the_last_fetched_origin() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        assert_eq!(commits_behind(&checkout, "master"), Some(0));
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");
        // Local git only: until someone fetches, the count is what was known.
        assert_eq!(commits_behind(&checkout, "master"), Some(0));
        run_in(&checkout, &["fetch", "origin", "-q"]);
        assert_eq!(commits_behind(&checkout, "master"), Some(1));
        assert_eq!(commits_behind(temp.path(), "master"), None);
    }
}
