//! Which project a record belongs to, from the directory it ran in
//! (§FS-013-burn.4).
//!
//! The registry already knows where every project is on disk and how its
//! branch workspaces are named, so this is a longest-prefix match against
//! those roots and nothing cleverer. Longest wins because the layouts overlap
//! by construction: a branch workspace at `<root>/<branch>` is inside its own
//! project's root, and a first match would file it under whichever row
//! happened to be checked first.
//!
//! What matches nothing is not dropped and not guessed at: it lands in
//! [`OTHER`]. Burn ephor cannot attribute is still burn, and a total that
//! quietly excluded it would be wrong in the one direction nobody can see.

use std::path::{Path, PathBuf};

/// Where a directory under no registered root is filed (§FS-013-burn.4).
pub const OTHER: &str = "other";

/// Every project's places on disk, longest first.
pub struct Roots {
    places: Vec<(PathBuf, String)>,
}

impl Roots {
    /// Build the table from `(place, project)` pairs — a project's checkout
    /// and every branch workspace it names. Sorted here rather than by the
    /// caller: the ordering is what makes the match correct, so it is not
    /// something a caller can forget.
    pub fn new(mut places: Vec<(PathBuf, String)>) -> Roots {
        places.sort_by(|left, right| {
            right
                .0
                .components()
                .count()
                .cmp(&left.0.components().count())
                .then_with(|| left.0.cmp(&right.0))
        });
        Roots { places }
    }

    /// The project a directory belongs to, or [`OTHER`].
    pub fn project(&self, cwd: Option<&str>) -> String {
        let Some(cwd) = cwd else {
            return OTHER.to_string();
        };
        let path = Path::new(cwd);
        for (place, project) in &self.places {
            if under(path, place) {
                return project.clone();
            }
        }
        OTHER.to_string()
    }

    /// Whether the table has anything to match against. An empty one is not a
    /// failure — every record simply lands in [`OTHER`] — but a surface may
    /// want to say so rather than showing one unexplained row.
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }
}

/// Whether `path` is `place` or sits inside it. Compared by path component,
/// never by string prefix: `/w/app-old` starts with `/w/app` and is a
/// different project.
fn under(path: &Path, place: &Path) -> bool {
    let mut theirs = place.components();
    let mut ours = path.components();
    loop {
        match (theirs.next(), ours.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(one), Some(two)) if one != two => return false,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Roots {
        Roots::new(vec![
            (PathBuf::from("/w/app"), "app".to_string()),
            (PathBuf::from("/w/app/fix/issue-1"), "app".to_string()),
            (PathBuf::from("/w/lib"), "lib".to_string()),
        ])
    }

    /// The whole of §FS-013-burn.4 in one case: a checkout resolves, a branch
    /// workspace inside another project's root resolves to its own project,
    /// a lookalike directory does not, and everything else is `other`.
    #[test]
    fn a_directory_finds_its_project_or_lands_in_other() {
        let roots = table();
        assert_eq!(roots.project(Some("/w/app")), "app");
        assert_eq!(roots.project(Some("/w/app/src/deep")), "app");
        assert_eq!(roots.project(Some("/w/app/fix/issue-1/src")), "app");
        assert_eq!(roots.project(Some("/w/lib")), "lib");
        // Not a string prefix: a different directory whose name starts the
        // same way is a different project.
        assert_eq!(roots.project(Some("/w/app-old")), OTHER);
        assert_eq!(roots.project(Some("/elsewhere")), OTHER);
        assert_eq!(roots.project(None), OTHER);
    }

    /// Longest wins, whatever order the caller built the table in: a nested
    /// workspace belonging to a *second* project must not be swallowed by the
    /// project whose root contains it.
    #[test]
    fn the_longest_root_wins_whatever_order_it_arrived_in() {
        let roots = Roots::new(vec![
            (PathBuf::from("/w"), "monorepo".to_string()),
            (PathBuf::from("/w/app"), "app".to_string()),
        ]);
        assert_eq!(roots.project(Some("/w/app/src")), "app");
        assert_eq!(roots.project(Some("/w/other")), "monorepo");
    }
}
