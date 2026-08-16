//! The forest: a project's place on disk, and the thing every git-facing
//! feature folds over (§AR-004-forest).
//!
//! Git is assumed and nothing else is (§REQ-001-boundary.3), so a project's
//! place is an ordered set of repositories under a root — a thin workspace
//! repository composing two others is the general case, a single repository a
//! forest of one (§FS-006-project-interface.1). The layout is declared by the
//! registry row where it says anything and probed where it does not
//! (§AR-004-forest.2); a manifest's `forest` is adopted the same way once
//! there is one to read (§FS-006-project-interface.2), which is why
//! [`Declaration`] is separate from where it came from.
//!
//! Every fold answers per repository and aggregates afterwards — never the
//! other way round (§AR-004-forest.1): a number that cannot say which
//! repository it came from sends the reader to look at all of them.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Local, Utc};

/// What a row (or, later, a manifest) says about one repository, before
/// anything has been looked for on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// Where it sits under the checkout: `.` is the checkout itself.
    pub path: String,
    /// What it is to the project, for a reader rather than for code.
    pub role: Option<String>,
    /// The branch this repository is measured and replayed against, where it
    /// differs from the project's.
    pub main: Option<String>,
}

impl Declaration {
    pub fn at(path: impl Into<String>) -> Declaration {
        Declaration {
            path: path.into(),
            role: None,
            main: None,
        }
    }
}

/// One repository of a forest, found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Its path relative to the checkout — `.` for the checkout itself. This
    /// is the name every report and answer uses.
    pub name: String,
    pub path: PathBuf,
    /// The remote a fold fetches from, pushes to and measures against, read
    /// off the repository itself (§AR-004-forest.2).
    pub remote: String,
    /// What this repository's branches are measured against.
    pub main: Option<String>,
    pub role: Option<String>,
}

/// The last-resort remote: what a repository with no remote at all, or a
/// declared repository with nothing on disk to ask, is folded over as. Every
/// repository that is there is asked instead (§AR-004-forest.2).
pub const ORIGIN: &str = "origin";

/// Whether a name is a branch rather than a template nothing expanded. A
/// project type may declare its per-repository base as `{branch}`, and a state
/// machine may hand a program a `{meta.branch}` its runtime could not fill;
/// either way an unexpanded placeholder is not a branch name, and measuring
/// against one asks git for a ref no repository has (§AR-004-forest.2).
pub fn is_branch_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && !name.contains('{')
}

/// A checkout's repositories, in order.
#[derive(Debug, Clone)]
pub struct Forest {
    /// The checkout this forest is of — a branch workspace or a project root.
    pub root: PathBuf,
    /// The project's main branch, where the registry names one.
    pub main: Option<String>,
    pub repos: Vec<Repo>,
    /// Declared and not on disk. Folds skip these — there is nothing to fold
    /// over — but a report can say so rather than quietly answering for fewer
    /// repositories than the reader believes they have.
    pub absent: Vec<String>,
    /// Every repository name the layout has, in declaration order, whether or
    /// not it is on disk. What a checkout has to *make*, as against what a
    /// rebase can fold over.
    pub layout: Vec<String>,
}

impl Forest {
    /// The forest of `checkout`: the declared repositories where the row
    /// declares any, otherwise whatever is on disk — the checkout itself and
    /// every git working tree directly under it, because a workspace of
    /// several repositories sharing a branch name is the shape that a
    /// root-only answer gets wrong (§AR-004-forest.2).
    pub fn resolve(checkout: &Path, main: Option<&str>, declared: &[Declaration]) -> Forest {
        let mut repos = Vec::new();
        let mut absent = Vec::new();
        let mut layout = Vec::new();
        if declared.is_empty() {
            for path in crate::git::probe(checkout) {
                let name = relative(checkout, &path);
                layout.push(name.clone());
                repos.push(Repo {
                    name,
                    // Asked once per repository, here where the probe already
                    // runs (§AR-004-forest.2).
                    remote: crate::git::remote(&path),
                    path,
                    main: main.map(String::from),
                    role: None,
                });
            }
        } else {
            for declaration in declared {
                layout.push(declaration.path.clone());
                let path = under(checkout, &declaration.path);
                // Present means a `.git` marker — a directory for a clone, a
                // file for a linked working tree. Tested through the path
                // rather than by asking git: a fold resolves every declared
                // repository on every refresh, and a subprocess per presence
                // check answers what the path already answers
                // (§AR-004-forest.3).
                if !path.join(".git").exists() {
                    absent.push(declaration.path.clone());
                    continue;
                }
                repos.push(Repo {
                    name: declaration.path.clone(),
                    // A row may declare where a repository is; which remote it
                    // has is a fact on disk, so it is probed anyway
                    // (§AR-004-forest.2).
                    remote: crate::git::remote(&path),
                    path,
                    main: declaration.main.clone().or_else(|| main.map(String::from)),
                    role: declaration.role.clone(),
                });
            }
        }
        Forest {
            root: checkout.to_path_buf(),
            main: main.map(String::from),
            repos,
            absent,
            layout,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.repos.is_empty()
    }

    /// The repository names, in order — what a script folding over the same
    /// forest is told (§FS-005-dispatch.8).
    pub fn names(&self) -> Vec<String> {
        self.repos.iter().map(|repo| repo.name.clone()).collect()
    }

    /// What the repository called `name` fetches from and is measured against.
    /// [`ORIGIN`] where the layout names one that is not on disk: there is no
    /// repository to ask, and nothing to fold over either.
    pub fn remote_of(&self, name: &str) -> &str {
        self.repos
            .iter()
            .find(|repo| repo.name == name)
            .map(|repo| repo.remote.as_str())
            .unwrap_or(ORIGIN)
    }

    /// What this repository is measured and replayed against: its own main
    /// where it has one, the project's otherwise, and what its remote calls
    /// its default branch as the last resort — a checkout no row describes
    /// still knows where it came from.
    ///
    /// A declared base that is still a template is not a value and is passed
    /// over (§AR-004-forest.2): a project type that declares its
    /// per-repository base as `{branch}` would otherwise have every fold ask
    /// git for `{branch}`, which no repository has, so nothing was ever
    /// measured and no offer that depends on a count was ever made.
    pub fn base(&self, repo: &Repo) -> Option<String> {
        repo.main
            .clone()
            .filter(|name| is_branch_name(name))
            .or_else(|| self.main.clone().filter(|name| is_branch_name(name)))
            .or_else(|| crate::git::default_base(&repo.path, &repo.remote))
    }

    /// How far each repository trails its base, per repository and then
    /// summed (§AR-004-forest.1). Local refs only — no fetch — so this is
    /// measured against what was last fetched, and each answer carries the day
    /// that was (§FS-004-quick-actions.6). The half of [`Self::standing`] that
    /// asks about the base, taken from that one fold rather than measured
    /// again, so no caller can hold two readings of one checkout.
    pub fn staleness(&self) -> Staleness {
        self.standing().staleness()
    }

    /// Where each repository's checked-out branch stands — against its base
    /// and against its own published copy — per repository, aggregated only
    /// by the callers that need one number (§AR-004-forest.1). The branch is
    /// each repository's own `HEAD`'s, and the published copy is resolved by
    /// §DA-003-upstream-is-the-published-copy.
    pub fn standing(&self) -> Standing {
        let repos = self
            .repos
            .iter()
            .map(|repo| {
                // Resolved once and carried on the answer, so everything that
                // asks whether a copy is the base again reads this fact
                // rather than resolving its own (§AR-004-forest.1).
                let base = self.base(repo);
                let measured = crate::git::standing(&repo.path, &repo.remote, base.as_deref());
                RepoStanding {
                    name: repo.name.clone(),
                    branch: measured.branch,
                    upstream: measured.upstream,
                    ahead: measured.track.map(|(ahead, _)| ahead),
                    behind_upstream: measured.track.map(|(_, behind)| behind),
                    behind_base: measured.behind_base,
                    base_seen: measured.base_seen,
                    upstream_seen: measured.upstream_seen,
                    base,
                }
            })
            .collect();
        Standing { repos }
    }
}

/// Where a branch is published — its copy on the remote, resolved from the
/// repository's own `HEAD` (§DA-003-upstream-is-the-published-copy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upstream {
    /// `<remote>/<branch>` holds what was last pushed of this branch.
    Published { remote: String, branch: String },
    /// Never pushed to `remote`: no copy to measure against and nothing to
    /// replay onto. An answer, not an error
    /// (§DA-003-upstream-is-the-published-copy).
    Unpushed { remote: String },
    /// Nothing to read: not a working tree, or `HEAD` is not on a branch.
    Unknown,
}

/// One repository's whole standing: the branch its `HEAD` is on, where that
/// branch is published, and its two distances — from its own published copy
/// and from its base. The two are different facts, replayed onto different
/// things, and are kept apart all the way to the reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStanding {
    pub name: String,
    /// The branch `HEAD` is on — read per repository, never the workspace
    /// directory's name (§DA-003-upstream-is-the-published-copy).
    pub branch: Option<String>,
    pub upstream: Upstream,
    /// Commits `HEAD` carries that its published copy does not.
    pub ahead: Option<u64>,
    /// Commits the published copy carries that `HEAD` does not.
    pub behind_upstream: Option<u64>,
    /// Commits `HEAD` trails the base — the count [`Staleness`] carries.
    pub behind_base: Option<u64>,
    /// When the local copy of the base last moved here: how fresh
    /// `behind_base` is (§FS-004-quick-actions.6).
    pub base_seen: Option<DateTime<Utc>>,
    /// When the published copy last moved here: how fresh `behind_upstream`
    /// is (§FS-004-quick-actions.8).
    pub upstream_seen: Option<DateTime<Utc>>,
    /// The base this repository was measured against, resolved once in the
    /// fold and carried here so a reader asking whether the published copy is
    /// simply the base again reads the fold's own fact.
    pub base: Option<String>,
}

impl RepoStanding {
    /// Whether this repository's published copy is its base again — a branch
    /// parked on the main branch and tracking it has one distance wearing two
    /// names, and the copy-side sums leave that distance to the base's own
    /// count (§FS-004-quick-actions.8).
    pub fn copies_the_base(&self) -> bool {
        match &self.upstream {
            Upstream::Published { branch, .. } => self.base.as_deref() == Some(branch.as_str()),
            Upstream::Unpushed { .. } | Upstream::Unknown => false,
        }
    }
}

/// The fold of [`Forest::standing`], with the per-repository answers kept
/// (§AR-004-forest.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Standing {
    pub repos: Vec<RepoStanding>,
}

impl Standing {
    /// The behind-base half in the shape everything above already reads —
    /// derived rather than folded again, so the two counts on a row cannot
    /// come from different measurements.
    pub fn staleness(&self) -> Staleness {
        Staleness {
            repos: self
                .repos
                .iter()
                .map(|repo| RepoStale {
                    name: repo.name.clone(),
                    behind: repo.behind_base,
                    seen: repo.base_seen,
                })
                .collect(),
        }
    }

    /// Commits the checkout trails its own published copies, in total. None
    /// when nothing was measured — a checkout published nowhere is not the
    /// same answer as one level with its copy
    /// (§DA-003-upstream-is-the-published-copy). A repository whose copy is
    /// its base again contributes nothing: that distance is the base count's
    /// own, and summing it here would put one distance under two names
    /// (§FS-004-quick-actions.8).
    pub fn behind_upstream(&self) -> Option<u64> {
        self.upstream_trail().map(|trail| trail.behind)
    }

    /// The same distance with its freshness on it: how far the checkout trails
    /// its published copies, and when the oldest of those copies last moved
    /// here (§FS-004-quick-actions.8). What the entry about the copies is
    /// labelled from, so the count and the day come off one fold.
    pub fn upstream_trail(&self) -> Option<Trail> {
        Trail::oldest(
            self.repos
                .iter()
                .filter(|repo| !repo.copies_the_base())
                .filter_map(|repo| Some((repo.behind_upstream?, repo.upstream_seen))),
        )
    }
}

/// A distance and how fresh the comparison behind it is: the count, and the
/// day the local copy of what it was measured against last moved here
/// (§FS-004-quick-actions.6). Every statement of a distance — a branch row, a
/// menu entry — is rendered from one of these, so none of them can say a
/// number without saying what it is a number as of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trail {
    pub behind: u64,
    /// None where no day was recorded: a clone that has never fetched, or a
    /// forest with such a repository in it. Nothing is invented to fill it.
    pub seen: Option<DateTime<Utc>>,
}

impl Trail {
    /// The fold over a forest: the counts summed, dated with the oldest day
    /// among them — a comparison is only as fresh as its stalest half. A
    /// repository with no day at all is older than any day there is, so it
    /// takes the whole answer's day away rather than letting another
    /// repository's stand in for it (§FS-004-quick-actions.6). None where
    /// nothing was measured, which is not the same answer as zero.
    fn oldest(measured: impl Iterator<Item = (u64, Option<DateTime<Utc>>)>) -> Option<Trail> {
        measured.fold(None, |folded: Option<Trail>, (count, seen)| {
            let Some(folded) = folded else {
                return Some(Trail {
                    behind: count,
                    seen,
                });
            };
            Some(Trail {
                behind: folded.behind + count,
                seen: match (folded.seen, seen) {
                    (Some(known), Some(other)) => Some(known.min(other)),
                    _ => None,
                },
            })
        })
    }

    /// "13 behind as of Jul 28", "level as of Jul 28", or "level" where no day
    /// was recorded (§FS-004-quick-actions.6). Never "up to date": all that
    /// was measured is that the checkout matched a copy last fetched on the
    /// day named, and a reading with no day on it claims more than it knows.
    pub fn label(&self) -> String {
        let distance = if self.behind == 0 {
            "level".to_string()
        } else {
            format!("{} behind", self.behind)
        };
        match self.seen {
            Some(seen) => format!("{distance} as of {}", as_of(seen)),
            None => distance,
        }
    }
}

/// The day a ref last moved, as a reader reads days: "Jul 28" in the year it
/// is now, "Jul 28, 2025" in any other — a bare month and day would put a
/// year-old fetch and last week's on the same footing, which is the whole
/// thing this qualifier exists to stop (§FS-004-quick-actions.6). Local time,
/// because the reader's question is which day *they* last fetched.
fn as_of(seen: DateTime<Utc>) -> String {
    let seen = seen.with_timezone(&Local);
    let now = Local::now();
    if seen.year() == now.year() {
        seen.format("%b %-d").to_string()
    } else {
        seen.format("%b %-d, %Y").to_string()
    }
}

/// One repository's answer to "how far behind".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStale {
    pub name: String,
    /// None where it could not be measured: not a working tree, or no ref to
    /// measure against.
    pub behind: Option<u64>,
    /// When the ref it was measured against last moved here; None where no
    /// such day was recorded (§FS-004-quick-actions.6).
    pub seen: Option<DateTime<Utc>>,
}

/// The fold of [`Forest::staleness`], with the per-repository answers kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Staleness {
    pub repos: Vec<RepoStale>,
}

impl Staleness {
    /// The commits the checkout trails in total. None when nothing could be
    /// measured at all, which is not the same answer as zero: zero means up to
    /// date, none means there was nothing to ask.
    pub fn total(&self) -> Option<u64> {
        self.trail().map(|trail| trail.behind)
    }

    /// The same total with its freshness on it: what the checkout trails by,
    /// and the day the oldest of the refs it was measured against last moved
    /// here (§FS-004-quick-actions.6). What a row and the entry beside it are
    /// both labelled from, so they cannot say different things.
    pub fn trail(&self) -> Option<Trail> {
        Trail::oldest(
            self.repos
                .iter()
                .filter_map(|repo| Some((repo.behind?, repo.seen))),
        )
    }

    /// The repositories actually behind, in forest order.
    pub fn behind(&self) -> Vec<&RepoStale> {
        self.repos
            .iter()
            .filter(|repo| repo.behind.unwrap_or(0) > 0)
            .collect()
    }

    /// "5 behind (ce 2, ee 3) as of Jul 28" — the number, which repositories
    /// it came from where more than one did, and how fresh the whole reading
    /// is (§FS-004-quick-actions.6). A checkout that measured level says
    /// "level as of Jul 28" rather than "up to date": the day is what makes it
    /// a fact instead of a claim.
    pub fn summary(&self) -> Option<String> {
        let trail = self.trail()?;
        let behind = self.behind();
        let mut summary = if trail.behind == 0 {
            "level".to_string()
        } else if behind.len() < 2 {
            format!("{} behind", trail.behind)
        } else {
            let parts: Vec<String> = behind
                .iter()
                .map(|repo| format!("{} {}", repo.name, repo.behind.unwrap_or(0)))
                .collect();
            format!("{} behind ({})", trail.behind, parts.join(", "))
        };
        if let Some(seen) = trail.seen {
            summary.push_str(&format!(" as of {}", as_of(seen)));
        }
        Some(summary)
    }
}

/// A path under a checkout, where `.` means the checkout itself.
pub fn under(checkout: &Path, name: &str) -> PathBuf {
    if name == "." {
        checkout.to_path_buf()
    } else {
        checkout.join(name)
    }
}

/// A repository's name inside its checkout — `.` for the checkout itself.
pub fn relative(checkout: &Path, repo: &Path) -> String {
    repo.strip_prefix(checkout)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn work_tree(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .unwrap()
            .success());
    }

    fn git_in(path: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "git {args:?} failed in {}",
            path.display()
        );
    }

    /// A repository on `branch`, with a remote of its own whose
    /// `<remote>/<branch>` is `ahead` commits in front of the checkout — the
    /// shape of a branch that trails its base, with no network in it. The
    /// remote's `HEAD` names `<branch>`, so the repository can say what its
    /// default is when nothing else does.
    fn tracked(path: &Path, remote: &str, branch: &str, ahead: usize) {
        work_tree(path);
        git_in(path, &["config", "user.email", "t@example.com"]);
        git_in(path, &["config", "user.name", "t"]);
        git_in(path, &["checkout", "-q", "-b", branch]);
        for step in 0..=ahead {
            std::fs::write(path.join("log.txt"), format!("{step}\n")).unwrap();
            git_in(path, &["add", "log.txt"]);
            git_in(path, &["commit", "-q", "-m", &format!("step {step}")]);
        }
        let published = format!("refs/remotes/{remote}/{branch}");
        git_in(path, &["update-ref", &published, "HEAD"]);
        if ahead > 0 {
            git_in(path, &["reset", "--hard", "-q", &format!("HEAD~{ahead}")]);
        }
        git_in(path, &["remote", "add", remote, "."]);
        git_in(
            path,
            &[
                "symbolic-ref",
                &format!("refs/remotes/{remote}/HEAD"),
                &published,
            ],
        );
    }

    #[test]
    fn a_declared_layout_keeps_the_rows_order_and_roles() {
        let tmp = tempfile::tempdir().unwrap();
        work_tree(&tmp.path().join("ce"));
        work_tree(&tmp.path().join("ee"));
        let declared = vec![
            Declaration {
                path: "ee".to_string(),
                role: Some("enterprise".to_string()),
                main: None,
            },
            Declaration::at("ce"),
        ];
        let forest = Forest::resolve(tmp.path(), Some("master"), &declared);
        assert_eq!(forest.names(), vec!["ee", "ce"]);
        assert_eq!(forest.repos[0].role.as_deref(), Some("enterprise"));
        // Nothing to ask: a repository with no remote at all is folded over as
        // [`ORIGIN`], which is the last resort and not the assumption.
        assert_eq!(forest.repos[0].remote, ORIGIN);
        // The project's main branch reaches every repository that has none.
        assert_eq!(forest.repos[1].main.as_deref(), Some("master"));
        assert!(forest.absent.is_empty());
    }

    #[test]
    fn a_declared_repository_that_is_not_there_is_named_not_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        work_tree(&tmp.path().join("ce"));
        let declared = vec![Declaration::at("ce"), Declaration::at("ee")];
        let forest = Forest::resolve(tmp.path(), None, &declared);
        assert_eq!(forest.names(), vec!["ce"]);
        assert_eq!(forest.absent, vec!["ee"]);
    }

    #[test]
    fn nothing_declared_probes_the_checkout_and_what_is_directly_under_it() {
        let tmp = tempfile::tempdir().unwrap();
        work_tree(tmp.path());
        work_tree(&tmp.path().join("plugins"));
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
        let forest = Forest::resolve(tmp.path(), None, &[]);
        // The checkout itself first, then its working trees in order; a plain
        // directory is not a repository.
        assert_eq!(forest.names(), vec![".", "plugins"]);
    }

    #[test]
    fn a_repository_of_its_own_is_measured_against_its_own_branch() {
        let tmp = tempfile::tempdir().unwrap();
        work_tree(&tmp.path().join("vendored"));
        let declared = vec![Declaration {
            path: "vendored".to_string(),
            role: None,
            main: Some("release/24".to_string()),
        }];
        let forest = Forest::resolve(tmp.path(), Some("master"), &declared);
        assert_eq!(forest.base(&forest.repos[0]).as_deref(), Some("release/24"));
    }

    /// §AR-004-forest.2: which remote a repository has is a fact on disk, so
    /// it is read there rather than spelled `origin` in the fold.
    #[test]
    fn the_remote_is_read_off_each_repository() {
        let tmp = tempfile::tempdir().unwrap();
        tracked(&tmp.path().join("solo"), "upstream", "master", 1);

        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("solo")]);
        assert_eq!(forest.repos[0].remote, "upstream");
        assert_eq!(forest.remote_of("solo"), "upstream");
        // A repository whose remote is not called `origin` is still measured:
        // the count came from `upstream/master`, which is the only one there.
        assert_eq!(forest.staleness().total(), Some(1));
        // The layout names one that is not on disk, so there is nothing to ask.
        assert_eq!(forest.remote_of("absent"), ORIGIN);
    }

    #[test]
    fn among_several_remotes_the_branchs_own_upstream_is_taken_before_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("ce");
        tracked(&repo, ORIGIN, "master", 0);
        git_in(&repo, &["remote", "add", "fork", "."]);
        git_in(&repo, &["update-ref", "refs/remotes/fork/master", "HEAD"]);

        // Nothing tracked: `origin` is the one among several a fold means.
        assert_eq!(crate::git::remote(&repo), ORIGIN);

        // Once git records where this branch is published, that is the answer —
        // it is the repository's own word, and `origin` was only a guess.
        git_in(
            &repo,
            &["branch", "--set-upstream-to=fork/master", "master"],
        );
        assert_eq!(crate::git::remote(&repo), "fork");
    }

    /// The graal shape: the project type declares one base per repository and
    /// writes it `{branch}`, which nothing expands before a fold reads it. Left
    /// verbatim it asked git for a ref called `{branch}`, so every repository
    /// answered "cannot measure" and no count was ever shown
    /// (§AR-004-forest.2).
    #[test]
    fn a_declared_base_that_is_a_template_is_not_a_base() {
        let tmp = tempfile::tempdir().unwrap();
        tracked(&tmp.path().join("ce"), ORIGIN, "master", 2);
        let declared = vec![Declaration {
            path: "ce".to_string(),
            role: None,
            main: Some("{branch}".to_string()),
        }];

        // The project's own main answers where the repository's is a template.
        let forest = Forest::resolve(tmp.path(), Some("master"), &declared);
        assert_eq!(forest.base(&forest.repos[0]).as_deref(), Some("master"));
        assert_eq!(forest.staleness().total(), Some(2));
        assert_eq!(
            forest.staleness().summary(),
            Some(format!("2 behind as of {}", today()))
        );

        // With no project main either, what the remote calls its default does.
        let forest = Forest::resolve(tmp.path(), None, &declared);
        assert_eq!(forest.base(&forest.repos[0]).as_deref(), Some("master"));
        assert_eq!(forest.staleness().total(), Some(2));

        // And a project main that is itself a template is no more a base than
        // the repository's was.
        let forest = Forest::resolve(tmp.path(), Some("{branch}"), &declared);
        assert_eq!(forest.base(&forest.repos[0]).as_deref(), Some("master"));
        assert_eq!(forest.staleness().total(), Some(2));
    }

    /// A project-wide main branch does not hold across a forest: the workspace
    /// repository of `~/c/g` is on `main` and has no `master` at all. It is one
    /// repository that cannot be measured, not a checkout that cannot be
    /// (§AR-004-forest.1).
    #[test]
    fn a_repository_without_the_base_does_not_silence_the_ones_that_have_it() {
        let tmp = tempfile::tempdir().unwrap();
        tracked(&tmp.path().join("ce"), ORIGIN, "master", 3);
        tracked(&tmp.path().join("root"), ORIGIN, "main", 0);
        let declared = vec![Declaration::at("ce"), Declaration::at("root")];

        let forest = Forest::resolve(tmp.path(), Some("master"), &declared);
        let stale = forest.staleness();
        assert_eq!(stale.repos[0].behind, Some(3));
        assert_eq!(stale.repos[1].behind, None);
        assert_eq!(stale.total(), Some(3));
        assert_eq!(stale.summary(), Some(format!("3 behind as of {}", today())));

        // Asked with nothing declared, each repository answers for itself: the
        // one with no `master` is measured against the default its own remote
        // records (§AR-004-forest.2).
        let forest = Forest::resolve(tmp.path(), None, &declared);
        assert_eq!(forest.base(&forest.repos[1]).as_deref(), Some("main"));
        let stale = forest.staleness();
        assert_eq!(stale.repos[1].behind, Some(0));
        assert_eq!(stale.total(), Some(3));
    }

    /// The tracked shape: git records where the branch is published, and one
    /// read answers ref and both distances (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn a_tracked_branch_stands_against_its_recorded_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("solo");
        tracked(&repo, ORIGIN, "feature", 2);
        git_in(
            &repo,
            &["branch", "--set-upstream-to=origin/feature", "feature"],
        );
        // The reader also committed locally, so both distances are non-zero.
        std::fs::write(repo.join("mine.txt"), "mine\n").unwrap();
        git_in(&repo, &["add", "mine.txt"]);
        git_in(&repo, &["commit", "-q", "-m", "mine"]);

        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("solo")]);
        let standing = forest.standing();
        assert_eq!(standing.repos[0].branch.as_deref(), Some("feature"));
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: ORIGIN.to_string(),
                branch: "feature".to_string(),
            }
        );
        assert_eq!(standing.repos[0].ahead, Some(1));
        assert_eq!(standing.repos[0].behind_upstream, Some(2));
        assert_eq!(standing.behind_upstream(), Some(2));
    }

    /// The untracked-but-pushed shape `worktree add -b` leaves behind: no
    /// tracking config, but the remote has the branch — the case bare
    /// `git rebase` fails on and this must not
    /// (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn an_untracked_branch_the_remote_has_is_published_all_the_same() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("ce");
        tracked(&repo, ORIGIN, "feature", 3);
        std::fs::write(repo.join("mine.txt"), "mine\n").unwrap();
        git_in(&repo, &["add", "mine.txt"]);
        git_in(&repo, &["commit", "-q", "-m", "mine"]);

        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("ce")]);
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: ORIGIN.to_string(),
                branch: "feature".to_string(),
            }
        );
        assert_eq!(standing.repos[0].ahead, Some(1));
        assert_eq!(standing.repos[0].behind_upstream, Some(3));
    }

    /// The never-pushed shape: an answer, not an error — the fold completes,
    /// says [`Upstream::Unpushed`], and measures no distance, because there
    /// is no copy to measure against
    /// (§DA-003-upstream-is-the-published-copy). And the branch is `HEAD`'s:
    /// the workspace directory's name says nothing about it.
    #[test]
    fn a_branch_never_pushed_is_unpushed_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("jdk");
        tracked(&repo, ORIGIN, "master", 0);
        git_in(&repo, &["checkout", "-q", "-b", "debug-of-the-day"]);

        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("jdk")]);
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].branch.as_deref(),
            Some("debug-of-the-day")
        );
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Unpushed {
                remote: ORIGIN.to_string(),
            }
        );
        assert_eq!(standing.repos[0].behind_upstream, None);
        // Nothing measured is not the same answer as level with a copy.
        assert_eq!(standing.behind_upstream(), None);
        // The base half still measured: the two distances are separate facts.
        assert_eq!(standing.repos[0].behind_base, Some(0));
    }

    /// A recorded upstream that names the base is where the branch was cut
    /// (`branch.autoSetupMerge`), not where it is published: it is read as no
    /// publication, and only a pushed copy of the branch's own name counts
    /// (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn an_upstream_naming_the_base_is_read_as_no_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("root");
        tracked(&repo, ORIGIN, "master", 0);
        git_in(&repo, &["checkout", "-q", "-b", "feature"]);
        git_in(
            &repo,
            &["branch", "--set-upstream-to=origin/master", "feature"],
        );

        // Never pushed: the tracking config alone publishes nothing.
        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("root")]);
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Unpushed {
                remote: ORIGIN.to_string(),
            }
        );

        // Once pushed, the copy of the branch's own name is the answer — the
        // tracking config still says `origin/master` and still does not.
        git_in(
            &repo,
            &["update-ref", "refs/remotes/origin/feature", "HEAD"],
        );
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: ORIGIN.to_string(),
                branch: "feature".to_string(),
            }
        );
        assert_eq!(standing.repos[0].behind_upstream, Some(0));
    }

    /// Staleness is derived from the standing, so the count on the row and
    /// the count in the fold are the same measurement (§AR-004-forest.1).
    #[test]
    fn staleness_derived_from_the_standing_is_the_staleness_measured_alone() {
        let tmp = tempfile::tempdir().unwrap();
        tracked(&tmp.path().join("ce"), ORIGIN, "master", 2);
        tracked(&tmp.path().join("ee"), ORIGIN, "master", 3);
        let declared = vec![Declaration::at("ce"), Declaration::at("ee")];

        let forest = Forest::resolve(tmp.path(), Some("master"), &declared);
        let standing = forest.standing();
        assert_eq!(standing.staleness(), forest.staleness());
        assert_eq!(standing.staleness().total(), Some(5));
        // The copies here are the bases, so the copy-side sum leaves the
        // whole distance to the base count rather than carrying the same 5
        // under a second name (§FS-004-quick-actions.8) — the per-repository
        // answers keep it whole.
        assert!(standing.repos.iter().all(RepoStanding::copies_the_base));
        assert_eq!(standing.behind_upstream(), None);
    }

    /// The mixed forest: one repository on the change's own published branch,
    /// one parked on the base and tracking it. The copy-side sum counts only
    /// the first — the parked repository's distance is the base count's own —
    /// so the two offers can never carry one distance under two names
    /// (§FS-004-quick-actions.8), while each repository's answer is kept
    /// whole (§AR-004-forest.1).
    #[test]
    fn a_repository_parked_on_the_base_is_left_out_of_the_copy_side_sum() {
        let tmp = tempfile::tempdir().unwrap();
        tracked(&tmp.path().join("ce"), ORIGIN, "feature", 2);
        tracked(&tmp.path().join("ee"), ORIGIN, "master", 3);
        git_in(
            &tmp.path().join("ee"),
            &["branch", "--set-upstream-to=origin/master", "master"],
        );
        let declared = vec![Declaration::at("ce"), Declaration::at("ee")];

        let forest = Forest::resolve(tmp.path(), Some("master"), &declared);
        let standing = forest.standing();
        assert!(!standing.repos[0].copies_the_base());
        assert!(standing.repos[1].copies_the_base());
        assert_eq!(standing.repos[0].behind_upstream, Some(2));
        assert_eq!(standing.repos[1].behind_upstream, Some(3));
        assert_eq!(standing.behind_upstream(), Some(2));
        // The parked repository's distance is not lost: it is the base's.
        assert_eq!(standing.staleness().total(), Some(3));
    }

    /// A tracking upstream whose remote ref is gone (`[gone]`) holds no copy
    /// any more: it publishes nothing, and only a pushed copy of the branch's
    /// own name could answer (§DA-003-upstream-is-the-published-copy).
    #[test]
    fn a_gone_upstream_publishes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("ce");
        tracked(&repo, ORIGIN, "feature", 0);
        git_in(
            &repo,
            &["branch", "--set-upstream-to=origin/feature", "feature"],
        );
        git_in(&repo, &["update-ref", "-d", "refs/remotes/origin/feature"]);

        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("ce")]);
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Unpushed {
                remote: ORIGIN.to_string(),
            }
        );
        assert_eq!(standing.repos[0].behind_upstream, None);
        assert_eq!(standing.behind_upstream(), None);
    }

    /// The `~/c/g/master/master` shape: a branch cut from a remote branch and
    /// never pushed, in a repository where nothing names a base — no
    /// repository main, no project main, and no `refs/remotes/<remote>/HEAD`.
    /// A base nobody could resolve cannot clear the recorded upstream of
    /// naming it, so the record is not taken at face value: the branch is
    /// unpushed, not published at where it was cut
    /// (§DA-003-upstream-is-the-published-copy) — no `↓N` that is really the
    /// base's distance, and nothing for a replay onto the copy to aim at the
    /// wrong ref.
    #[test]
    fn an_unresolvable_base_fails_closed_rather_than_trusting_the_record() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("root");
        work_tree(&repo);
        git_in(&repo, &["config", "user.email", "t@example.com"]);
        git_in(&repo, &["config", "user.name", "t"]);
        git_in(&repo, &["checkout", "-q", "-b", "main"]);
        std::fs::write(repo.join("log.txt"), "0\n").unwrap();
        git_in(&repo, &["add", "log.txt"]);
        git_in(&repo, &["commit", "-q", "-m", "base"]);
        git_in(&repo, &["remote", "add", ORIGIN, "."]);
        git_in(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git_in(&repo, &["checkout", "-q", "-b", "feature"]);
        git_in(
            &repo,
            &["branch", "--set-upstream-to=origin/main", "feature"],
        );

        let forest = Forest::resolve(tmp.path(), None, &[Declaration::at("root")]);
        assert_eq!(forest.base(&forest.repos[0]), None);
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Unpushed {
                remote: ORIGIN.to_string(),
            }
        );
        assert_eq!(standing.repos[0].behind_upstream, None);
        assert_eq!(standing.behind_upstream(), None);
    }

    /// `standing` against a remote not called `origin`: both resolution steps
    /// answer with the repository's own remote (§AR-004-forest.2,
    /// §DA-003-upstream-is-the-published-copy).
    #[test]
    fn the_standing_is_read_off_a_remote_not_called_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("solo");
        tracked(&repo, "upstream", "feature", 3);

        // The pushed-copy probe: no tracking config, and the copy lives on
        // `upstream`, the only remote there is.
        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("solo")]);
        assert_eq!(forest.repos[0].remote, "upstream");
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: "upstream".to_string(),
                branch: "feature".to_string(),
            }
        );
        assert_eq!(standing.repos[0].behind_upstream, Some(3));
        assert_eq!(standing.behind_upstream(), Some(3));

        // And the recorded path, once git says where the branch is published.
        git_in(
            &repo,
            &["branch", "--set-upstream-to=upstream/feature", "feature"],
        );
        let standing = forest.standing();
        assert_eq!(
            standing.repos[0].upstream,
            Upstream::Published {
                remote: "upstream".to_string(),
                branch: "feature".to_string(),
            }
        );
        assert_eq!(standing.repos[0].behind_upstream, Some(3));
    }

    /// Not a repository, or not on a branch: nothing to read, said as such.
    #[test]
    fn a_detached_head_has_no_standing_to_speak_of() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("solo");
        tracked(&repo, ORIGIN, "master", 0);
        git_in(&repo, &["checkout", "-q", "--detach"]);

        let forest = Forest::resolve(tmp.path(), Some("master"), &[Declaration::at("solo")]);
        let standing = forest.standing();
        assert_eq!(standing.repos[0].branch, None);
        assert_eq!(standing.repos[0].upstream, Upstream::Unknown);
        assert_eq!(standing.behind_upstream(), None);
    }

    /// A repository's answer with no day on it: what a base that was never
    /// fetched here leaves behind (§FS-004-quick-actions.6).
    fn stale(name: &str, behind: Option<u64>) -> RepoStale {
        RepoStale {
            name: name.to_string(),
            behind,
            seen: None,
        }
    }

    /// A fixture's refs are written the moment the test runs, so every
    /// distance measured against one is dated today
    /// (§FS-004-quick-actions.6).
    fn today() -> String {
        Local::now().format("%b %-d").to_string()
    }

    /// Midday on a given day, in the reader's own zone, as the reflog would
    /// have recorded it.
    fn day(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Local
            .with_ymd_and_hms(year, month, day, 12, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn staleness_sums_but_keeps_which_repository_was_behind() {
        let stale = Staleness {
            repos: vec![stale("ce", Some(2)), stale("ee", Some(3))],
        };
        assert_eq!(stale.total(), Some(5));
        assert_eq!(stale.behind().len(), 2);
        assert_eq!(stale.summary().as_deref(), Some("5 behind (ce 2, ee 3)"));
    }

    #[test]
    fn one_repository_behind_needs_no_breakdown() {
        let stale = Staleness {
            repos: vec![stale("ce", Some(4)), stale("ee", Some(0))],
        };
        assert_eq!(stale.summary().as_deref(), Some("4 behind"));
    }

    #[test]
    fn nothing_measurable_is_not_the_same_answer_as_level() {
        let nothing = Staleness {
            repos: vec![stale(".", None)],
        };
        assert_eq!(nothing.total(), None);
        assert_eq!(nothing.summary(), None);

        // Level is an answer, and it says as of when — never "up to date",
        // which claimed the branch was current when all that was measured was
        // a match against a copy of unstated age (§FS-004-quick-actions.6).
        let current = Staleness {
            repos: vec![stale(".", Some(0))],
        };
        assert_eq!(current.total(), Some(0));
        assert_eq!(current.summary().as_deref(), Some("level"));
    }

    /// The fold takes the oldest day among the repositories that were
    /// measured — a comparison is only as fresh as its stalest half — and a
    /// repository with no day at all takes the answer's day away rather than
    /// letting another's stand in for it (§FS-004-quick-actions.6).
    #[test]
    fn the_fold_is_only_as_fresh_as_its_oldest_repository() {
        let mixed = Staleness {
            repos: vec![
                RepoStale {
                    seen: Some(day(2019, 7, 28)),
                    ..stale("ce", Some(2))
                },
                RepoStale {
                    seen: Some(day(2019, 8, 11)),
                    ..stale("ee", Some(3))
                },
            ],
        };
        assert_eq!(mixed.trail().unwrap().seen, Some(day(2019, 7, 28)));
        assert_eq!(
            mixed.summary().as_deref(),
            Some("5 behind (ce 2, ee 3) as of Jul 28, 2019")
        );

        let never_fetched = Staleness {
            repos: vec![
                RepoStale {
                    seen: Some(day(2019, 8, 11)),
                    ..stale("ce", Some(2))
                },
                stale("ee", Some(3)),
            ],
        };
        assert_eq!(never_fetched.trail().unwrap().seen, None);
        assert_eq!(
            never_fetched.summary().as_deref(),
            Some("5 behind (ce 2, ee 3)")
        );
    }

    /// What a row and an entry are both labelled from: the count, or the word
    /// *level*, and the day the comparison is as of where one is known
    /// (§FS-004-quick-actions.6). The year goes unsaid in the year it is now
    /// and is named in any other, so a year-old fetch cannot read as last
    /// week's.
    #[test]
    fn a_distance_is_stated_as_of_the_day_it_was_measured() {
        let dated = |behind, seen| Trail { behind, seen }.label();
        assert_eq!(
            dated(13, Some(Utc::now())),
            format!("13 behind as of {}", Local::now().format("%b %-d"))
        );
        assert_eq!(dated(0, Some(day(2019, 7, 28))), "level as of Jul 28, 2019");
        assert_eq!(dated(13, None), "13 behind");
        assert_eq!(dated(0, None), "level");
    }
}
