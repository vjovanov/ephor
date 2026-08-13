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
    /// The remote a fold fetches from and pushes to. `origin` until something
    /// says otherwise.
    pub remote: String,
    /// What this repository's branches are measured against.
    pub main: Option<String>,
    pub role: Option<String>,
}

/// The default remote. Nothing has ever declared another one, but the folds
/// ask the repository rather than assuming, so that the day one does is a
/// field and not a rewrite.
pub const ORIGIN: &str = "origin";

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
                    path,
                    remote: ORIGIN.to_string(),
                    main: main.map(String::from),
                    role: None,
                });
            }
        } else {
            for declaration in declared {
                layout.push(declaration.path.clone());
                let path = under(checkout, &declaration.path);
                if !crate::git::is_work_tree(&path) {
                    absent.push(declaration.path.clone());
                    continue;
                }
                repos.push(Repo {
                    name: declaration.path.clone(),
                    path,
                    remote: ORIGIN.to_string(),
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

    /// What this repository is measured and replayed against: its own main
    /// where it has one, the project's otherwise, and what its remote calls
    /// its default branch as the last resort — a checkout no row describes
    /// still knows where it came from.
    pub fn base(&self, repo: &Repo) -> Option<String> {
        repo.main
            .clone()
            .or_else(|| self.main.clone())
            .or_else(|| crate::git::default_base(&repo.path))
    }

    /// How far each repository trails its base, per repository and then
    /// summed (§AR-004-forest.1). Local refs only — no fetch — so this is
    /// measured against what was last fetched.
    pub fn staleness(&self) -> Staleness {
        let repos = self
            .repos
            .iter()
            .map(|repo| RepoStale {
                name: repo.name.clone(),
                behind: self
                    .base(repo)
                    .and_then(|base| crate::git::commits_behind(&repo.path, &base)),
            })
            .collect();
        Staleness { repos }
    }
}

/// One repository's answer to "how far behind".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStale {
    pub name: String,
    /// None where it could not be measured: not a working tree, or no ref to
    /// measure against.
    pub behind: Option<u64>,
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
        self.repos
            .iter()
            .filter_map(|repo| repo.behind)
            .fold(None, |total: Option<u64>, count| {
                Some(total.unwrap_or(0) + count)
            })
    }

    /// The repositories actually behind, in forest order.
    pub fn behind(&self) -> Vec<&RepoStale> {
        self.repos
            .iter()
            .filter(|repo| repo.behind.unwrap_or(0) > 0)
            .collect()
    }

    /// "5 behind (ce 2, ee 3)" — the number a row shows, and which
    /// repositories it came from where more than one did.
    pub fn summary(&self) -> Option<String> {
        let total = self.total()?;
        if total == 0 {
            return Some("up to date".to_string());
        }
        let behind = self.behind();
        if behind.len() < 2 {
            return Some(format!("{total} behind"));
        }
        let parts: Vec<String> = behind
            .iter()
            .map(|repo| format!("{} {}", repo.name, repo.behind.unwrap_or(0)))
            .collect();
        Some(format!("{total} behind ({})", parts.join(", ")))
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

    fn work_tree(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .unwrap()
            .success());
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

    #[test]
    fn staleness_sums_but_keeps_which_repository_was_behind() {
        let stale = Staleness {
            repos: vec![
                RepoStale {
                    name: "ce".to_string(),
                    behind: Some(2),
                },
                RepoStale {
                    name: "ee".to_string(),
                    behind: Some(3),
                },
            ],
        };
        assert_eq!(stale.total(), Some(5));
        assert_eq!(stale.behind().len(), 2);
        assert_eq!(stale.summary().as_deref(), Some("5 behind (ce 2, ee 3)"));
    }

    #[test]
    fn one_repository_behind_needs_no_breakdown() {
        let stale = Staleness {
            repos: vec![
                RepoStale {
                    name: "ce".to_string(),
                    behind: Some(4),
                },
                RepoStale {
                    name: "ee".to_string(),
                    behind: Some(0),
                },
            ],
        };
        assert_eq!(stale.summary().as_deref(), Some("4 behind"));
    }

    #[test]
    fn nothing_measurable_is_not_the_same_answer_as_up_to_date() {
        let nothing = Staleness {
            repos: vec![RepoStale {
                name: ".".to_string(),
                behind: None,
            }],
        };
        assert_eq!(nothing.total(), None);
        assert_eq!(nothing.summary(), None);

        let current = Staleness {
            repos: vec![RepoStale {
                name: ".".to_string(),
                behind: Some(0),
            }],
        };
        assert_eq!(current.total(), Some(0));
        assert_eq!(current.summary().as_deref(), Some("up to date"));
    }
}
