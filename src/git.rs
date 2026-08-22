//! The git ephor does itself: how far a checkout trails its main branch,
//! replaying it there (§FS-004-quick-actions.6) or onto the branch's own
//! published copy (§FS-004-quick-actions.8), and making the workspace that is
//! not there yet (§FS-004-quick-actions.7).
//!
//! Nothing here knows what a forge is, or what the project is built with. A
//! rebase is a fetch, a replay, and an answer per repository — the same
//! operation on every project ephor watches. The reader's key and the state
//! machine's program state both run this one implementation, because two of
//! them would eventually disagree about what a clean rebase is
//! (§FS-005-dispatch.12).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};

use crate::forest::{under, Forest, Upstream, ORIGIN};

/// What a replay puts the branch on top of. `Base` is one branch name for the
/// whole forest — the project's main branch, or whatever the reader named —
/// and `Upstream` is a different ref in every repository, each branch's own
/// published copy (§FS-004-quick-actions.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Onto {
    Base(String),
    /// Resolved per repository from that repository's own `HEAD`
    /// (§DA-003-upstream-is-the-published-copy), which is why the choice is a
    /// kind rather than a second branch name: there is no one name to pass.
    Upstream,
}

impl Onto {
    /// What a report and a one-line summary call it. A per-repository ref has
    /// no single name, so the whole rebase is named by what it aimed at and
    /// each repository names the ref it actually used.
    pub fn label(&self) -> &str {
        match self {
            Onto::Base(base) => base,
            Onto::Upstream => "its published copy",
        }
    }
}

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
    /// Nothing published: this branch has no copy on the remote, so there is
    /// nothing to replay onto. An answer in the same register as an already
    /// current repository, never a refusal (§FS-004-quick-actions.8) — only
    /// [`Onto::Upstream`] can reach it.
    Unpublished,
    /// git would not, and this is what it said.
    Refused(String),
}

impl Replay {
    /// What this outcome is called, in one word a reading can carry.
    pub fn name(&self) -> &'static str {
        match self {
            Replay::Current => "current",
            Replay::Rebased(_) => "rebased",
            Replay::Conflicted(_) => "conflicted",
            Replay::Dirty(_) => "dirty",
            Replay::Unpublished => "unpublished",
            Replay::Refused(_) => "refused",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepoReplay {
    /// The repository's path relative to the checkout (`.` for the root).
    pub repo: String,
    /// The remote it was fetched from and measured against. Carried per
    /// repository rather than per rebase because a forest's repositories need
    /// not agree on one (§AR-004-forest.2), and a report naming a remote the
    /// reader does not have sends them to look for a ref that is not there.
    pub remote: String,
    /// The branch it was on, where git could say.
    pub branch: Option<String>,
    /// The ref this repository was replayed onto, remote and all. Carried per
    /// repository because under [`Onto::Upstream`] no two repositories need
    /// name the same one (§FS-004-quick-actions.8); None where there was
    /// nothing to replay onto.
    pub onto: Option<String>,
    pub replay: Replay,
}

/// One rebase of one checkout: every repository under it, in order.
#[derive(Debug, Clone)]
pub struct Rebase {
    pub checkout: PathBuf,
    /// What the whole rebase aimed at; each repository says which ref that
    /// came out as for it.
    pub onto: Onto,
    pub repos: Vec<RepoReplay>,
    /// Declared repositories with no working tree on disk. Named rather than
    /// dropped, so the answer never quietly speaks for fewer repositories
    /// than the reader has (§AR-004-forest.1); they gate nothing, because a
    /// workspace that is not there is a checkout question
    /// (§FS-004-quick-actions.7).
    pub absent: Vec<String>,
}

impl Rebase {
    pub fn conflicted(&self) -> Vec<&RepoReplay> {
        self.repos
            .iter()
            .filter(|repo| matches!(repo.replay, Replay::Conflicted(_)))
            .collect()
    }

    /// Repositories with no published copy to replay onto. Not stuck and not
    /// a failure: there is simply nothing there (§FS-004-quick-actions.8).
    pub fn unpublished(&self) -> Vec<&RepoReplay> {
        self.repos
            .iter()
            .filter(|repo| matches!(repo.replay, Replay::Unpublished))
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

    /// What each repository came to, as a reading names it
    /// (§FS-011-command-line.7). One shape for the whole outcome, so a state
    /// machine reads the same answer the report describes.
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({
            "checkout": self.checkout,
            "onto": match &self.onto {
                Onto::Upstream => "upstream".to_string(),
                Onto::Base(base) => base.clone(),
            },
            "summary": self.summary(),
            "rebased": self.rebased(),
            "conflicted": self.conflicted().len(),
            "absent": self.absent,
            "repos": self
                .repos
                .iter()
                .map(|repo| {
                    let mut row = serde_json::json!({
                        "repo": repo.repo,
                        "remote": repo.remote,
                        "replay": repo.replay.name(),
                        "paths": match &repo.replay {
                            Replay::Conflicted(paths) | Replay::Dirty(paths) => paths.clone(),
                            _ => Vec::new(),
                        },
                    });
                    // A fact that is not there is absent, never `null` typed as
                    // the fact would have been: the published shape says
                    // `behind` is a count and `says` a sentence, and emitting
                    // the key anyway made every repository that neither replayed
                    // nor was refused violate the schema it is meant to hold to
                    // (§REQ-002-parity.4). Absent is also the shape's own
                    // wording — "where it replayed any", "where it was".
                    let row = row.as_object_mut().expect("a row is an object");
                    if let Some(branch) = &repo.branch {
                        row.insert("branch".to_string(), serde_json::json!(branch));
                    }
                    if let Some(onto) = &repo.onto {
                        row.insert("onto".to_string(), serde_json::json!(onto));
                    }
                    if let Replay::Rebased(behind) = &repo.replay {
                        row.insert("behind".to_string(), serde_json::json!(behind));
                    }
                    if let Replay::Refused(why) = &repo.replay {
                        row.insert("says".to_string(), serde_json::json!(why));
                    }
                    serde_json::Value::Object(row.clone())
                })
                .collect::<Vec<_>>(),
        })
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
        let unpublished = self.unpublished().len();
        let rebased = self.rebased();
        let mut parts = Vec::new();
        if rebased > 0 {
            parts.push(format!("{rebased} rebased onto {}", self.onto.label()));
        }
        if conflicted > 0 {
            parts.push(format!("{conflicted} in conflict"));
        }
        if stuck > 0 {
            parts.push(format!("{stuck} left alone"));
        }
        if unpublished > 0 {
            parts.push(format!("{unpublished} published nowhere"));
        }
        if !self.absent.is_empty() {
            parts.push(format!("{} not on disk", self.absent.len()));
        }
        if parts.is_empty() {
            return format!("already on {}", self.onto.label());
        }
        parts.join(", ")
    }

    /// The whole outcome as markdown — what a state machine hands the agent
    /// that resolves it, and what the reader sees on their terminal.
    pub fn report(&self) -> String {
        let mut out = format!(
            "# rebase onto {} in {}\n\n",
            self.onto.label(),
            self.checkout.display()
        );
        if self.repos.is_empty() {
            out.push_str("No git repository under the checkout — nothing was done.\n");
            self.report_absent(&mut out);
            return out;
        }
        for repo in &self.repos {
            let branch = repo.branch.as_deref().unwrap_or("(unknown branch)");
            // The ref this repository used, which under §FS-004-quick-actions.8
            // is its own and not the rebase's. The fallback is only for arms
            // that never had a ref — a base rebase names the base it aimed at,
            // and a per-repository ref that was never resolved has no
            // `<remote>/…` spelling to fake.
            let onto = repo.onto.clone().unwrap_or_else(|| match &self.onto {
                Onto::Base(base) => format!("{}/{base}", repo.remote),
                Onto::Upstream => self.onto.label().to_string(),
            });
            out.push_str(&format!("## {} — {branch}\n\n", repo.repo));
            match &repo.replay {
                Replay::Current => {
                    out.push_str(&format!("Already on top of `{onto}`.\n\n"));
                }
                Replay::Rebased(commits) => {
                    out.push_str(&format!(
                        "Replayed onto `{onto}`; it had trailed by {commits} commit(s).\n\n",
                    ));
                }
                Replay::Unpublished => {
                    out.push_str(&format!(
                        "Nothing published — `{branch}` has no copy on `{}` to replay onto, \
                         so this repository was left as it is.\n\n",
                        repo.remote
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
        self.report_absent(&mut out);
        out
    }

    /// The declared repositories that are not on disk, named in the report so
    /// a fold over a partial workspace never quietly answers for fewer
    /// repositories than the reader has (§AR-004-forest.1). Making them is a
    /// checkout's move, not a rebase's (§FS-004-quick-actions.7).
    fn report_absent(&self, out: &mut String) {
        for name in &self.absent {
            out.push_str(&format!(
                "## {name}\n\nNo working tree here — the checkout is missing this repository, \
                 so nothing was measured or replayed. Checking it out is its own move.\n\n"
            ));
        }
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

/// Which remote this repository's folds fetch from, push to and measure
/// against. A fact that can be probed is probed, whatever a row says
/// (§AR-004-forest.2): the branch's own upstream where git records one, the
/// sole remote where the repository has exactly one, `origin` where it is
/// among several, and otherwise the first git lists. [`ORIGIN`] is the answer
/// only when there is no remote at all — a repository that calls its remote
/// anything else is still measured and still replayed.
pub fn remote(repo: &Path) -> String {
    let remotes: Vec<String> = git(repo, &["remote"])
        .into_iter()
        .flat_map(|listed| {
            listed
                .lines()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(String::from)
                .collect::<Vec<String>>()
        })
        .collect();
    match remotes.len() {
        0 => ORIGIN.to_string(),
        // A branch's upstream names a remote this repository has, so where it
        // has one the second question could only give the same answer.
        1 => remotes[0].clone(),
        _ => upstream_remote(repo, &remotes)
            .or_else(|| remotes.iter().find(|name| *name == ORIGIN).cloned())
            .unwrap_or_else(|| remotes[0].clone()),
    }
}

/// Which of `remotes` the checked-out branch tracks, where git records one.
/// The upstream is read as `<remote>/<branch>` and a remote name may itself
/// carry a slash, so the prefix is matched against the names git just listed
/// rather than split at the first separator.
fn upstream_remote(repo: &Path, remotes: &[String]) -> Option<String> {
    let upstream = git(
        repo,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )?;
    let upstream = upstream.trim().to_string();
    remotes
        .iter()
        .find(|name| {
            upstream
                .strip_prefix(name.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .cloned()
}

/// Commits `repo`'s HEAD is behind the base, and the day the ref it was
/// counted against last moved here; None when no usable ref — a path that is
/// not a repository answers the same way, because git itself fails there, so
/// nothing spawns a subprocess to ask that separately (§AR-004-forest.3). The
/// remote is handed in rather than spelled here: nothing under this module
/// knows which remote a project uses (§AR-004-forest.2).
///
/// The last-fetched `<remote>/<base>` is preferred, and only it has a day to
/// give (§FS-004-quick-actions.6): where the count comes from a local branch
/// of that name instead, nothing was fetched to be fresh or stale about, so
/// there is no qualifier and none is invented.
fn behind_the_base(repo: &Path, remote: &str, base: &str) -> Option<(u64, Option<DateTime<Utc>>)> {
    let fetched = format!("{remote}/{base}");
    if let Some(count) = behind_ref(repo, &fetched) {
        return Some((count, ref_seen(repo, &format!("refs/remotes/{fetched}"))));
    }
    behind_ref(repo, base).map(|count| (count, None))
}

/// When `reference` last moved on this disk: the newest entry in its own
/// reflog, which for a remote-tracking ref is the last fetch that actually
/// brought something down for it (§FS-004-quick-actions.6). None where the ref
/// has no reflog at all — a fresh clone writes the ref and no entry — because
/// a day nobody recorded is not a day to print.
///
/// The ref's reflog and not `FETCH_HEAD`: a fetch that fails to connect
/// truncates `FETCH_HEAD` and bumps its mtime, which would stamp today onto a
/// comparison it never refreshed, and `FETCH_HEAD` is per working tree while
/// the refs — and their reflogs — are shared by every worktree of the
/// repository.
fn ref_seen(repo: &Path, reference: &str) -> Option<DateTime<Utc>> {
    let stamp = git(repo, &["log", "-g", "-1", "--format=%ct", reference])?;
    DateTime::from_timestamp(stamp.trim().parse().ok()?, 0)
}

/// Commits `reference` carries that `HEAD` does not; None where there is no
/// such ref. The one measurement behind every distance here, so a count in a
/// report and a count in a menu cannot be computed two ways.
fn behind_ref(repo: &Path, reference: &str) -> Option<u64> {
    git(
        repo,
        &["rev-list", "--count", &format!("HEAD..{reference}")],
    )
    .and_then(|count| count.trim().parse().ok())
}

/// One repository's whole standing, measured in one pass so the counts on a
/// row, the offer's gate and the replay cannot come from different
/// measurements (§AR-004-forest.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    /// The branch `HEAD` is on — read off the repository, never a workspace
    /// directory's name (§DA-003-upstream-is-the-published-copy).
    pub branch: Option<String>,
    pub upstream: Upstream,
    /// `(ahead, behind)` against the published copy; None where nothing is
    /// published.
    pub track: Option<(u64, u64)>,
    /// Commits `HEAD` trails the base, against the last-fetched ref; None
    /// where no base was named or nothing could be measured.
    pub behind_base: Option<u64>,
    /// When the local copy of the base last moved here — how fresh
    /// `behind_base` is (§FS-004-quick-actions.6). None where there is no such
    /// day to report.
    pub base_seen: Option<DateTime<Utc>>,
    /// The same for the published copy: when it last moved here, which is how
    /// fresh `track` is (§FS-004-quick-actions.8).
    pub upstream_seen: Option<DateTime<Utc>>,
}

/// Where `repo`'s checked-out branch is published, how far `HEAD` sits from
/// that copy, and how far it trails `base`. The published copy is resolved in
/// three steps (§DA-003-upstream-is-the-published-copy): the recorded
/// `@{upstream}` where it is not `[gone]` and the resolved base says it does
/// not name it — a base nobody could resolve cannot clear the record of
/// naming it, so it fails closed (§FS-004-quick-actions.8); else
/// `<remote>/<branch>` where the remote has one; else the branch is unpushed
/// — an answer, not an error.
///
/// The whole answer costs two subprocesses on the recorded path: one
/// `for-each-ref` over the local branches gives the checked-out branch (the
/// `%(HEAD)` star), its recorded upstream and both distances to it at once,
/// and one `rev-list` measures the base. Its failure is also the
/// not-a-repository answer, so nothing probes that separately — the fold
/// already trusts its own repositories (§AR-004-forest.3). Local refs only —
/// no fetch — so distances are against what was last fetched, and each
/// distance that found a ref to compare with costs one further short reflog
/// read to say when that was (§FS-004-quick-actions.6).
pub fn standing(repo: &Path, remote: &str, base: Option<&str>) -> Measured {
    let mut measured = measure(repo, remote, base);
    // Asked of the ref the resolution above settled on, so what the entry
    // names and what it dates are one copy (§FS-004-quick-actions.8).
    measured.upstream_seen = match &measured.upstream {
        Upstream::Published { remote, branch } => {
            ref_seen(repo, &format!("refs/remotes/{remote}/{branch}"))
        }
        Upstream::Unpushed { .. } | Upstream::Unknown => None,
    };
    measured
}

/// Everything [`standing`] answers except the published copy's freshness,
/// which is read once from the copy this settles on.
fn measure(repo: &Path, remote: &str, base: Option<&str>) -> Measured {
    let Some(listed) = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(HEAD)%09%(refname:short)%09%(upstream:short)%09%(upstream:track)",
            "refs/heads",
        ],
    ) else {
        return Measured {
            branch: None,
            upstream: Upstream::Unknown,
            track: None,
            behind_base: None,
            base_seen: None,
            upstream_seen: None,
        };
    };
    // One reading: the count and the day the ref it was counted against last
    // moved here, so a row cannot state a distance and a freshness that were
    // measured a moment apart (§AR-004-forest.1).
    let against_base = base.and_then(|base| behind_the_base(repo, remote, base));
    let behind_base = against_base.map(|(count, _)| count);
    let base_seen = against_base.and_then(|(_, seen)| seen);
    // The starred line is the branch this working tree has checked out; no
    // star is a detached HEAD — not on a branch, so there is no publication
    // to speak of, though the distance to the base above still stands.
    let Some(line) = listed.lines().find_map(|line| line.strip_prefix('*')) else {
        return Measured {
            branch: None,
            upstream: Upstream::Unknown,
            track: None,
            behind_base,
            base_seen,
            upstream_seen: None,
        };
    };
    let mut fields = line.split('\t');
    fields.next(); // what remains of the %(HEAD) column after the star
    let branch = fields.next().unwrap_or("").trim().to_string();
    let upstream = fields.next().unwrap_or("").trim();
    let track = fields.next().unwrap_or("").trim();
    if branch.is_empty() {
        return Measured {
            branch: None,
            upstream: Upstream::Unknown,
            track: None,
            behind_base,
            base_seen,
            upstream_seen: None,
        };
    }
    let own_copy = format!("{remote}/{branch}");
    if !upstream.is_empty() && track != "[gone]" {
        // A record naming the pushed copy of the branch's own name is what
        // the probe below could only re-derive — same ref, same distances —
        // so it is taken whatever it says about the base: a branch parked on
        // the base and tracking it lands here, and the fact is true
        // (§DA-003-upstream-is-the-published-copy).
        if upstream == own_copy {
            return Measured {
                branch: Some(branch.clone()),
                upstream: Upstream::Published {
                    remote: remote.to_string(),
                    branch,
                },
                track: Some(parse_track(track)),
                behind_base,
                base_seen,
                upstream_seen: None,
            };
        }
        // Tracking that names the base records where the branch was cut, not
        // where it is published, and it falls through to the pushed-copy
        // probe — as does a base nobody could resolve, which cannot clear the
        // record of naming it (§DA-003-upstream-is-the-published-copy).
        if base.is_some_and(|base| format!("{remote}/{base}") != upstream) {
            if let Some(published) = upstream.strip_prefix(&format!("{remote}/")) {
                return Measured {
                    branch: Some(branch),
                    upstream: Upstream::Published {
                        remote: remote.to_string(),
                        branch: published.to_string(),
                    },
                    track: Some(parse_track(track)),
                    behind_base,
                    base_seen,
                    upstream_seen: None,
                };
            }
        }
    }
    // The pushed copy of the branch's own name — the shape `worktree add -b`
    // leaves behind (§DA-003-upstream-is-the-published-copy). The ref's
    // existence and its distances are one question, asked of git once: a
    // count that fails is a copy that is not there.
    match left_right(repo, &format!("refs/remotes/{own_copy}")) {
        Some(track) => Measured {
            branch: Some(branch.clone()),
            upstream: Upstream::Published {
                remote: remote.to_string(),
                branch,
            },
            track: Some(track),
            behind_base,
            base_seen,
            upstream_seen: None,
        },
        None => Measured {
            branch: Some(branch),
            upstream: Upstream::Unpushed {
                remote: remote.to_string(),
            },
            track: None,
            behind_base,
            base_seen,
            upstream_seen: None,
        },
    }
}

/// Commits on each side of `HEAD...reference`: `(ahead, behind)`. None where
/// there is no such ref to measure.
fn left_right(repo: &Path, reference: &str) -> Option<(u64, u64)> {
    let counts = git(
        repo,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{reference}"),
        ],
    )?;
    let mut counts = counts.split_whitespace();
    Some((counts.next()?.parse().ok()?, counts.next()?.parse().ok()?))
}

/// `%(upstream:track)` as counts: `[ahead A, behind B]` with either half
/// absent, and the empty string meaning level on both sides.
fn parse_track(track: &str) -> (u64, u64) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in track
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
    {
        let part = part.trim();
        if let Some(count) = part.strip_prefix("ahead ") {
            ahead = count.parse().unwrap_or(0);
        } else if let Some(count) = part.strip_prefix("behind ") {
            behind = count.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

/// What this repository's remote calls its default branch, where it recorded
/// one. The fallback for a checkout no registry entry describes, and for one
/// whose row named a base that is a template rather than a branch
/// (§AR-004-forest.2).
pub fn default_base(repo: &Path, remote: &str) -> Option<String> {
    let head = git(
        repo,
        &[
            "symbolic-ref",
            "--short",
            &format!("refs/remotes/{remote}/HEAD"),
        ],
    )?;
    let name = head.trim().strip_prefix(&format!("{remote}/"))?.to_string();
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

/// Replay every repository of the forest onto `onto` — a fold with one answer
/// per repository (§AR-004-forest.1). Under [`Onto::Upstream`] that is a
/// different ref in each of them, each branch's own published copy
/// (§FS-004-quick-actions.8); under [`Onto::Base`] it is one branch name for
/// all of them.
pub fn rebase(forest: &Forest, onto: &Onto) -> Rebase {
    let outcomes = forest
        .repos
        .iter()
        .map(|repo| {
            // The remote comes from the repository being folded over, so a
            // forest whose repositories do not agree on one still replays
            // (§AR-004-forest.2). Its base is asked for only where the
            // published copy is being resolved, which is the one thing that
            // needs it: tracking that names the base records where the branch
            // was cut, not where it is published.
            let base = match onto {
                Onto::Base(_) => None,
                Onto::Upstream => forest.base(repo),
            };
            let (reference, replay) = replay_one(&repo.path, &repo.remote, base.as_deref(), onto);
            RepoReplay {
                repo: repo.name.clone(),
                remote: repo.remote.clone(),
                branch: git(&repo.path, &["rev-parse", "--abbrev-ref", "HEAD"])
                    .map(|name| name.trim().to_string()),
                onto: reference,
                replay,
            }
        })
        .collect();
    Rebase {
        checkout: forest.root.clone(),
        onto: onto.clone(),
        repos: outcomes,
        // Declared and not on disk: nothing to fold over, and said so rather
        // than quietly answering for fewer repositories (§AR-004-forest.1).
        absent: forest.absent.clone(),
    }
}

/// One repository: the ref it was replayed onto, and what came of it. Every
/// guard is the same whichever ref that is — a rebase already stopped, an
/// uncommitted working tree, and the conflict left where it stands
/// (§FS-004-quick-actions.6).
fn replay_one(
    repo: &Path,
    remote: &str,
    base: Option<&str>,
    onto: &Onto,
) -> (Option<String>, Replay) {
    // A repository already stopped in a rebase is a conflict to finish, not a
    // rebase to start: starting a second one over it would lose the first.
    if let Some(files) = unmerged(repo) {
        if !files.is_empty() {
            return (None, Replay::Conflicted(files));
        }
    }
    match git(repo, &["status", "--porcelain", "--untracked-files=no"]) {
        Some(status) if !status.trim().is_empty() => {
            return (
                None,
                Replay::Dirty(
                    status
                        .lines()
                        .map(|line| line.trim_end().to_string())
                        .collect(),
                ),
            )
        }
        None => return (None, Replay::Refused("git status failed".to_string())),
        _ => {}
    }

    if let Err(message) = run(repo, &["fetch", remote, "--prune"]) {
        return (None, Replay::Refused(message));
    }
    // Resolved after the fetch, so a copy pushed since the last one is seen —
    // and so "nothing published" means nothing is published now.
    let reference = match onto {
        Onto::Base(base) => {
            let reference = format!("{remote}/{base}");
            if git(repo, &["rev-parse", "--verify", "--quiet", &reference]).is_none() {
                return (
                    Some(reference.clone()),
                    Replay::Refused(format!(
                        "no '{reference}' — the base branch is not on this repository's remote"
                    )),
                );
            }
            reference
        }
        Onto::Upstream => match standing(repo, remote, base).upstream {
            Upstream::Published { remote, branch } => format!("{remote}/{branch}"),
            // Never pushed, or not on a branch at all: an answer, not a
            // refusal (§FS-004-quick-actions.8).
            Upstream::Unpushed { .. } | Upstream::Unknown => return (None, Replay::Unpublished),
        },
    };
    // Unmeasurable is not zero: everything above has verified the ref, so
    // this cannot happen today, but "could not measure" reported as "already
    // on top of" is the one lie the None-not-zero rule everywhere else exists
    // to prevent (§AR-004-forest.1).
    let Some(behind) = behind_ref(repo, &reference) else {
        return (
            Some(reference.clone()),
            Replay::Refused(format!("cannot measure {reference}")),
        );
    };
    if behind == 0 {
        return (Some(reference), Replay::Current);
    }
    let replay = match run(repo, &["rebase", &reference]) {
        Ok(_) => Replay::Rebased(behind),
        Err(message) => match unmerged(repo) {
            Some(files) if !files.is_empty() => Replay::Conflicted(files),
            // A rebase that failed without leaving a conflict left nothing to
            // resolve; whatever git said is the whole answer.
            _ => Replay::Refused(message),
        },
    };
    (Some(reference), replay)
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
    /// What this outcome is called, in one word a reading can carry.
    pub fn name(&self) -> &'static str {
        match self {
            Created::Tracking => "tracking",
            Created::Branched(_) => "branched",
            Created::Present => "present",
            Created::Refused(_) => "refused",
        }
    }

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
    /// The remote the branch was looked for on, and the base grown from
    /// (§AR-004-forest.2).
    pub remote: String,
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

    /// What each repository came to, as a reading names it
    /// (§FS-011-command-line.7).
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({
            "workspace": self.target,
            "branch": self.branch,
            "ready": self.is_ready(),
            "summary": self.summary(),
            "repos": self
                .repos
                .iter()
                .map(|repo| {
                    let mut row = serde_json::json!({
                        "repo": repo.repo,
                        "remote": repo.remote,
                        "created": repo.created.name(),
                    });
                    // Absent rather than null, for the same reason the replay
                    // rows above are: the shape says these are strings
                    // (§REQ-002-parity.4).
                    let row = row.as_object_mut().expect("a row is an object");
                    if let Created::Branched(base) = &repo.created {
                        row.insert("from".to_string(), serde_json::json!(base));
                    }
                    if let Created::Refused(why) = &repo.created {
                        row.insert("says".to_string(), serde_json::json!(why));
                    }
                    serde_json::Value::Object(row.clone())
                })
                .collect::<Vec<_>>(),
        })
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
                    "The repository has no `{}`, so it was started from `{}/{base}`.\n\n",
                    self.branch, repo.remote
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
///
/// The fold is over `forest.layout` and not over its repositories: what a
/// checkout has to *make* includes the ones that are not on disk yet. The
/// forest is what says which remote each one is grown from
/// (§AR-004-forest.2).
pub fn create(source: &Path, target: &Path, forest: &Forest, branch: &str, base: &str) -> Creation {
    let repos = forest
        .layout
        .iter()
        .map(|name: &String| {
            let remote = forest.remote_of(name).to_string();
            RepoCreated {
                repo: name.clone(),
                created: create_one(
                    &under(source, name),
                    &under(target, name),
                    &remote,
                    branch,
                    base,
                ),
                remote,
            }
        })
        .collect();

    Creation {
        target: target.to_path_buf(),
        branch: branch.to_string(),
        repos,
    }
}

fn create_one(source: &Path, target: &Path, remote: &str, branch: &str, base: &str) -> Created {
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
    if let Err(message) = run(source, &["fetch", remote, "--prune"]) {
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

    let published = format!("refs/remotes/{remote}/{branch}");
    if git(source, &["rev-parse", "--verify", "--quiet", &published]).is_some() {
        let start = format!("{remote}/{branch}");
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
    let start = format!("{remote}/{base}");
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

    /// One base for the whole forest, the way every case here asks for it.
    fn onto(base: &str) -> Onto {
        Onto::Base(base.to_string())
    }

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

    /// Publish `feature` and then move the published copy on without this
    /// clone: what a teammate, a second machine or the forge does to a branch.
    /// The push records no tracking config — `checkout -b` off a local branch
    /// sets none — which is the untracked-but-pushed shape bare `git rebase`
    /// cannot replay (§DA-003-upstream-is-the-published-copy).
    fn advance_published_feature(root: &Path, name: &str, file: &str) {
        let origin = root.join(format!("{name}.git"));
        let clone = root.join("work").join(name);
        run_in(&clone, &["push", "-q", "origin", "feature"]);
        run_in(&origin, &["checkout", "-q", "feature"]);
        commit(&origin, file, "theirs\n", "somebody else pushed");
        run_in(&origin, &["checkout", "-q", "master"]);
    }

    #[test]
    fn a_branch_that_trails_is_replayed_and_one_that_does_not_is_current() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");

        let replayed = super::rebase(&Forest::resolve(&checkout, None, &[]), &onto("master"));
        assert_eq!(replayed.repos.len(), 1);
        assert_eq!(replayed.repos[0].replay, Replay::Rebased(1));
        assert_eq!(replayed.repos[0].branch.as_deref(), Some("feature"));
        // The reader's commit survived the replay, on top of theirs.
        assert!(checkout.join("mine.txt").exists());
        assert!(checkout.join("theirs.txt").exists());

        // Immediately again: there is nothing left to replay, and that is an
        // answer rather than a no-op (§FS-004-quick-actions.6).
        let again = super::rebase(&Forest::resolve(&checkout, None, &[]), &onto("master"));
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

        let stopped = super::rebase(&Forest::resolve(&checkout, None, &[]), &onto("master"));
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
        let again = super::rebase(&Forest::resolve(&checkout, None, &[]), &onto("master"));
        assert!(matches!(again.repos[0].replay, Replay::Conflicted(_)));
    }

    #[test]
    fn uncommitted_work_is_reported_and_left_alone() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");
        std::fs::write(checkout.join("mine.txt"), "half-written\n").unwrap();

        let refused = super::rebase(&Forest::resolve(&checkout, None, &[]), &onto("master"));
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

        let both = super::rebase(&Forest::resolve(&workspace, None, &[]), &onto("master"));
        let names: Vec<&str> = both.repos.iter().map(|r| r.repo.as_str()).collect();
        assert_eq!(names, ["ce", "ee"]);
        assert!(both.repos.iter().all(|r| r.replay == Replay::Rebased(1)));
        assert_eq!(both.summary(), "2 rebased onto master");

        // The registry's own repo list wins where there is one.
        let named = super::rebase(
            &Forest::resolve(&workspace, None, &[crate::forest::Declaration::at("ce")]),
            &onto("master"),
        );
        assert_eq!(named.repos.len(), 1);
    }

    /// A declared repository that is not on disk is named by the fold's
    /// answer rather than silently dropped (§AR-004-forest.1): a report must
    /// not read as if the workspace held fewer repositories than the reader
    /// has. It gates nothing — the missing tree is a checkout question, not
    /// this rebase's (§FS-004-quick-actions.7).
    #[test]
    fn a_declared_repository_not_on_disk_is_named_not_dropped() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let workspace = checkout.parent().unwrap().to_path_buf();
        let declared = vec![
            crate::forest::Declaration::at("app"),
            crate::forest::Declaration::at("gone"),
        ];

        let outcome = super::rebase(
            &Forest::resolve(&workspace, None, &declared),
            &onto("master"),
        );
        assert_eq!(outcome.repos.len(), 1);
        assert_eq!(outcome.absent, vec!["gone".to_string()]);
        assert_eq!(outcome.summary(), "1 not on disk");
        assert!(outcome.report().contains("## gone"));
        assert!(outcome.report().contains("No working tree here"));
        assert!(outcome.stuck().is_empty());
    }

    #[test]
    fn a_base_branch_that_is_not_on_origin_is_refused_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let refused = super::rebase(&Forest::resolve(&checkout, None, &[]), &onto("trunk"));
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
            &Forest::resolve(&source, None, &[]),
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
            &Forest::resolve(&source, None, &[]),
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
            &Forest::resolve(&source, None, &[]),
            "feature",
            "master",
        );
        assert_eq!(first.repos[0].created, Created::Tracking);

        let again = super::create(
            &source,
            &target,
            &Forest::resolve(&source, None, &[]),
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
            &Forest::resolve(&source, None, &[]),
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

    /// Nothing here spells `origin`: the fold is handed the remote the
    /// repository it is folding over actually has (§AR-004-forest.2), so a
    /// clone whose remote is called something else is still fetched, still
    /// measured and still replayed — and the report names the ref it used.
    #[test]
    fn a_repository_whose_remote_is_not_called_origin_is_still_replayed() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        run_in(&checkout, &["remote", "rename", "origin", "upstream"]);
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");

        let forest = Forest::resolve(&checkout, None, &[]);
        assert_eq!(forest.repos[0].remote, "upstream");
        let replayed = super::rebase(&forest, &onto("master"));
        assert_eq!(replayed.repos[0].replay, Replay::Rebased(1));
        assert!(replayed.report().contains("`upstream/master`"));

        // A base that is on no remote is refused by the name it was looked for
        // under, which is the repository's own.
        let missing = super::rebase(&forest, &onto("trunk"));
        match &missing.repos[0].replay {
            Replay::Refused(message) => assert!(message.contains("upstream/trunk")),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn a_workspace_is_grown_from_the_remote_the_repository_actually_has() {
        let temp = tempfile::tempdir().unwrap();
        let ce = checkout_with_origin(temp.path(), "ce");
        let source = ce.parent().unwrap().to_path_buf();
        run_in(&ce, &["remote", "rename", "origin", "upstream"]);
        run_in(&ce, &["checkout", "-q", "master"]);
        run_in(&ce, &["branch", "-q", "-D", "feature"]);

        let forest = Forest::resolve(&source, None, &[]);
        assert_eq!(forest.remote_of("ce"), "upstream");
        let target = temp.path().join("ws").join("nova");
        let made = super::create(&source, &target, &forest, "nova", "master");
        assert_eq!(
            made.repos[0].created,
            Created::Branched("master".to_string())
        );
        assert!(made.report().contains("`upstream/master`"));
        assert!(made.is_ready());
    }

    /// The replay onto the branch's own published copy: a different ref from
    /// the base, resolved per repository, and the one bare `git rebase` cannot
    /// reach because this branch records no upstream
    /// (§FS-004-quick-actions.8).
    #[test]
    fn a_branch_is_replayed_onto_its_own_published_copy() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        advance_published_feature(temp.path(), "app", "theirs.txt");
        // Main moved too, and by a different distance: the two replays are two
        // operations, and this one is not the other under another name.
        advance_master(temp.path(), "app", "main-moved.txt", "main\n");

        let forest = Forest::resolve(&checkout, None, &[]);
        let replayed = super::rebase(&forest, &Onto::Upstream);
        assert_eq!(replayed.repos[0].replay, Replay::Rebased(1));
        assert_eq!(replayed.repos[0].onto.as_deref(), Some("origin/feature"));
        assert_eq!(replayed.summary(), "1 rebased onto its published copy");
        assert!(replayed.report().contains("`origin/feature`"));
        // Their commit is underneath and the reader's is on top of it; main's
        // is nowhere, because that is the other rebase.
        assert!(checkout.join("theirs.txt").exists());
        assert!(checkout.join("mine.txt").exists());
        assert!(!checkout.join("main-moved.txt").exists());

        // Asked again there is nothing left to replay, which is an answer.
        let again = super::rebase(&forest, &Onto::Upstream);
        assert_eq!(again.repos[0].replay, Replay::Current);
        assert_eq!(again.summary(), "already on its published copy");
        // And the base rebase still has its own work to do.
        let onto_base = super::rebase(&forest, &onto("master"));
        assert_eq!(onto_base.repos[0].replay, Replay::Rebased(1));
        assert!(checkout.join("main-moved.txt").exists());
    }

    /// Never pushed: nothing published is an answer in the same register as an
    /// already-current repository, never a refusal (§FS-004-quick-actions.8).
    #[test]
    fn a_branch_published_nowhere_is_reported_not_refused() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");

        let outcome = super::rebase(&Forest::resolve(&checkout, None, &[]), &Onto::Upstream);
        assert_eq!(outcome.repos[0].replay, Replay::Unpublished);
        assert_eq!(outcome.repos[0].onto, None);
        // Not stuck and not a conflict: the command succeeds and says why
        // there was nothing to do.
        assert!(outcome.stuck().is_empty());
        assert!(outcome.conflicted().is_empty());
        assert_eq!(outcome.summary(), "1 published nowhere");
        assert!(outcome.report().contains("Nothing published"));
    }

    /// One workspace, two repositories, two different answers — the whole
    /// reason the ref is per repository rather than per rebase
    /// (§AR-004-forest.1, §FS-004-quick-actions.8).
    #[test]
    fn each_repository_replays_onto_its_own_copy_and_the_unpublished_ones_say_so() {
        let temp = tempfile::tempdir().unwrap();
        let ce = checkout_with_origin(temp.path(), "ce");
        let _ee = checkout_with_origin(temp.path(), "ee");
        let workspace = ce.parent().unwrap().to_path_buf();
        advance_published_feature(temp.path(), "ce", "theirs.txt");

        let outcome = super::rebase(&Forest::resolve(&workspace, None, &[]), &Onto::Upstream);
        let names: Vec<&str> = outcome
            .repos
            .iter()
            .map(|repo| repo.repo.as_str())
            .collect();
        assert_eq!(names, ["ce", "ee"]);
        assert_eq!(outcome.repos[0].replay, Replay::Rebased(1));
        assert_eq!(outcome.repos[0].onto.as_deref(), Some("origin/feature"));
        assert_eq!(outcome.repos[1].replay, Replay::Unpublished);
        assert_eq!(
            outcome.summary(),
            "1 rebased onto its published copy, 1 published nowhere"
        );
    }

    /// A branch parked on the base and tracking it: the tracking names the
    /// base, so it publishes nothing, and the pushed copy of the branch's own
    /// name is what answers — which here *is* the base
    /// (§DA-003-upstream-is-the-published-copy). The fact is true and the fold
    /// acts on it; not offering the reader the same replay twice is the
    /// menu's job (§FS-004-quick-actions.8).
    #[test]
    fn a_branch_parked_on_the_base_replays_onto_the_base_under_either_name() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        run_in(&checkout, &["checkout", "-q", "master"]);
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");

        let outcome = super::rebase(&Forest::resolve(&checkout, None, &[]), &Onto::Upstream);
        assert_eq!(outcome.repos[0].onto.as_deref(), Some("origin/master"));
        assert_eq!(outcome.repos[0].replay, Replay::Rebased(1));
    }

    /// Every other guard is the rebase's own, whichever ref it aims at
    /// (§FS-004-quick-actions.6): uncommitted work is reported and left alone,
    /// and a repository standing in a conflict is that conflict.
    #[test]
    fn the_guards_hold_the_same_way_when_replaying_onto_the_copy() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        advance_published_feature(temp.path(), "app", "theirs.txt");
        std::fs::write(checkout.join("mine.txt"), "half-written\n").unwrap();

        let forest = Forest::resolve(&checkout, None, &[]);
        let refused = super::rebase(&forest, &Onto::Upstream);
        match &refused.repos[0].replay {
            Replay::Dirty(paths) => assert!(paths[0].contains("mine.txt")),
            other => panic!("expected Dirty, got {other:?}"),
        }
        assert_eq!(refused.stuck().len(), 1);

        // Both sides wrote the same file, so the replay cannot decide and is
        // left standing where it stopped.
        run_in(&checkout, &["checkout", "-q", "--", "mine.txt"]);
        commit(&checkout, "theirs.txt", "ours\n", "ours");
        let stopped = super::rebase(&forest, &Onto::Upstream);
        assert!(matches!(stopped.repos[0].replay, Replay::Conflicted(_)));
        assert!(stopped.report().contains("mid-rebase"));
    }

    /// A count says as of when, and the day comes from the base's own ref:
    /// the last time this disk saw `origin/master` move
    /// (§FS-004-quick-actions.6). It never over-claims — a clone that has
    /// never fetched has no day at all, and a fetch that finds nothing new
    /// leaves the day where it was.
    #[test]
    fn the_distance_is_dated_from_the_bases_own_reflog() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        // A clone writes the ref and no reflog entry, so there is nothing to
        // date the reading with and nothing is invented.
        assert_eq!(
            behind_the_base(&checkout, ORIGIN, "master"),
            Some((0, None))
        );

        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");
        run_in(&checkout, &["fetch", "origin", "-q"]);
        let (count, seen) =
            behind_the_base(&checkout, ORIGIN, "master").expect("a base to measure");
        assert_eq!(count, 1);
        let moved = seen.expect("the fetch moved origin/master, and its reflog says when");
        assert!((Utc::now() - moved).num_seconds().abs() < 300);

        // A fetch that brought nothing down does not restamp the reading as
        // today's: the ref did not move, so neither did the day.
        run_in(&checkout, &["fetch", "origin", "-q"]);
        assert_eq!(
            behind_the_base(&checkout, ORIGIN, "master"),
            Some((1, Some(moved)))
        );

        // The published copy carries its own day, off the ref the entry about
        // it names (§FS-004-quick-actions.8).
        advance_published_feature(temp.path(), "app", "theirs.txt");
        let measured = standing(&checkout, ORIGIN, Some("master"));
        assert_eq!(measured.base_seen, Some(moved));
        assert!(measured.upstream_seen.is_some());

        // A base that resolved to a local branch of that name was never
        // fetched, so there is no fetch for the reading to be as of.
        assert_eq!(
            behind_the_base(&checkout, "nowhere", "master"),
            Some((0, None))
        );
    }

    #[test]
    fn behind_counts_against_the_last_fetched_origin() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_with_origin(temp.path(), "app");
        let count = |repo: &Path| behind_the_base(repo, ORIGIN, "master").map(|(count, _)| count);
        assert_eq!(count(&checkout), Some(0));
        advance_master(temp.path(), "app", "theirs.txt", "theirs\n");
        // Local git only: until someone fetches, the count is what was known.
        assert_eq!(count(&checkout), Some(0));
        run_in(&checkout, &["fetch", "origin", "-q"]);
        assert_eq!(count(&checkout), Some(1));
        assert_eq!(count(temp.path()), None);
    }

    /// Every replay outcome, and every checkout outcome, holds to the shape it
    /// publishes — with the facts it does not have *absent* rather than
    /// spelled `null` (§REQ-002-parity.4).
    ///
    /// The published shapes say `behind` is a count, `says` a sentence, `from`
    /// a branch: a repository that neither replayed nor was refused used to
    /// print all three as `null`, so every ordinary replay and every ordinary
    /// checkout violated the schema ephor ships for it. Walked over every
    /// variant here rather than through whichever one a scenario happens to
    /// reach, because that is how the two that were never reached stayed
    /// wrong.
    #[test]
    fn every_replay_and_every_checkout_row_holds_to_its_published_shape() {
        use crate::api::schema::holds;

        let replays = [
            Replay::Current,
            Replay::Rebased(3),
            Replay::Conflicted(vec!["f.txt".to_string()]),
            Replay::Dirty(vec!["f.txt".to_string()]),
            Replay::Unpublished,
            Replay::Refused("no upstream".to_string()),
        ];
        for replay in replays {
            let name = replay.name();
            let outcome = Rebase {
                checkout: PathBuf::from("/w/demo"),
                onto: Onto::Base("main".to_string()),
                repos: vec![RepoReplay {
                    repo: "app".to_string(),
                    remote: ORIGIN.to_string(),
                    branch: None,
                    onto: None,
                    replay,
                }],
                absent: Vec::new(),
            };
            let view = outcome.view();
            assert!(
                holds("rebase", &view).is_empty(),
                "a '{name}' replay does not hold to the published shape: {:?}\n{view}",
                holds("rebase", &view)
            );
            let row = &view["repos"][0];
            assert_eq!(row["replay"], name);
            assert_eq!(row.get("branch").is_none(), true, "{row}");
            assert_eq!(
                row.get("behind").is_some(),
                name == "rebased",
                "only a replay that replayed says how far: {row}"
            );
            assert_eq!(
                row.get("says").is_some(),
                name == "refused",
                "only a refusal says why: {row}"
            );
        }

        for created in [
            Created::Tracking,
            Created::Branched("main".to_string()),
            Created::Present,
            Created::Refused("no remote".to_string()),
        ] {
            let name = created.name();
            let creation = Creation {
                target: PathBuf::from("/w/demo-you/retry"),
                branch: "you/retry".to_string(),
                repos: vec![RepoCreated {
                    repo: "app".to_string(),
                    remote: ORIGIN.to_string(),
                    created,
                }],
            };
            let view = creation.view();
            assert!(
                holds("checkout", &view).is_empty(),
                "a '{name}' checkout does not hold to the published shape: {:?}\n{view}",
                holds("checkout", &view)
            );
            let row = &view["repos"][0];
            assert_eq!(row["created"], name);
            assert_eq!(row.get("from").is_some(), name == "branched", "{row}");
            assert_eq!(row.get("says").is_some(), name == "refused", "{row}");
        }
    }
}
