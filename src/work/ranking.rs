//! An ordering of item ids, read from a file (§FS-005-dispatch.26).
//!
//! Ephor does not compute a rank — it reads one a project already wrote, and
//! the order the file names *is* the rank: no scores, no bands, nothing here
//! interprets a tie. Absent, empty, or unreadable is not an error; it is one
//! of three reasons dispatch falls back to the order it always used, and the
//! caller says which.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

/// What reading the file at `path` came to.
pub struct Ranking {
    pub path: PathBuf,
    /// Ids in file order, best first. Empty when the file could not be used.
    pub order: Vec<String>,
    /// Set when the file was actually read, for how old it is.
    pub modified: Option<DateTime<Utc>>,
    /// Set when the file could not be used to order anything, and why.
    pub fallback: Option<Fallback>,
}

pub enum Fallback {
    Absent,
    Empty,
    Unreadable(String),
}

impl Ranking {
    /// One line, said in prose and carried into `--json` alike
    /// (§REQ-002-parity.3): which file was used and how old it is, or which
    /// of the three ways it could not be.
    pub fn says(&self) -> String {
        match &self.fallback {
            None => format!(
                "ranking {} ({} old, {} item(s) named) orders this dispatch",
                self.path.display(),
                self.modified
                    .map(|modified| crate::feed::render::age(Utc::now(), modified))
                    .unwrap_or_else(|| "unknown age".to_string()),
                self.order.len(),
            ),
            Some(Fallback::Absent) => format!(
                "ranking {} is not there — falling back to the existing order",
                self.path.display()
            ),
            Some(Fallback::Empty) => format!(
                "ranking {} names nothing — falling back to the existing order",
                self.path.display()
            ),
            Some(Fallback::Unreadable(err)) => format!(
                "ranking {} could not be read ({err}) — falling back to the existing order",
                self.path.display()
            ),
        }
    }
}

/// Read the ranking at `path`, tolerating everything the spec asks it to.
pub fn read(path: &Path) -> Ranking {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ranking {
                path: path.to_path_buf(),
                order: Vec::new(),
                modified: None,
                fallback: Some(Fallback::Absent),
            };
        }
        Err(err) => {
            return Ranking {
                path: path.to_path_buf(),
                order: Vec::new(),
                modified: None,
                fallback: Some(Fallback::Unreadable(err.to_string())),
            };
        }
    };
    let modified = metadata
        .modified()
        .ok()
        .map(|modified| DateTime::<Utc>::from(modified));
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            return Ranking {
                path: path.to_path_buf(),
                order: Vec::new(),
                modified,
                fallback: Some(Fallback::Unreadable(err.to_string())),
            };
        }
    };
    let order = parse(&text);
    let fallback = order.is_empty().then_some(Fallback::Empty);
    Ranking {
        path: path.to_path_buf(),
        order,
        modified,
        fallback,
    }
}

/// One item id per line; blank lines are skipped and nothing else is special
/// — the file names ids, not a format of its own.
fn parse(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Ranked items first, in `ranked_ids`' own order, then everything else
/// keeping the order it already had. The ranking orders; it never filters
/// — every input item is still in the output, just possibly moved forward.
/// A repeated id is silently idempotent: the first occurrence wins and later
/// ones neither move the item again nor count as unmatched. The second
/// return value is every id in `ranked_ids` that matched nothing.
pub fn order<T>(
    items: Vec<T>,
    ranked_ids: &[String],
    id_of: impl Fn(&T) -> &str,
) -> (Vec<T>, Vec<String>) {
    if ranked_ids.is_empty() {
        return (items, Vec::new());
    }
    let mut pool: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let mut ranked = Vec::with_capacity(pool.len());
    let mut unmatched = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for wanted in ranked_ids {
        if !seen.insert(wanted.as_str()) {
            continue;
        }
        let found = pool
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|item| id_of(item) == wanted));
        match found {
            Some(slot) => ranked.push(slot.take().expect("just matched")),
            None => unmatched.push(wanted.clone()),
        }
    }
    ranked.extend(pool.into_iter().flatten());
    (ranked, unmatched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_id_per_line_blank_lines_skipped() {
        assert_eq!(
            parse("first\n\nsecond\n   \nthird\n"),
            vec!["first", "second", "third"]
        );
    }

    #[test]
    fn surrounding_space_on_a_line_is_trimmed() {
        assert_eq!(parse("  first  \n second"), vec!["first", "second"]);
    }

    #[test]
    fn empty_text_names_nothing() {
        assert!(parse("").is_empty());
        assert!(parse("   \n\n").is_empty());
    }

    #[test]
    fn an_absent_file_falls_back_and_says_so() {
        let reading = read(Path::new("/does/not/exist/ranking.txt"));
        assert!(reading.order.is_empty());
        assert!(matches!(reading.fallback, Some(Fallback::Absent)));
        assert!(reading.says().contains("is not there"));
    }

    #[test]
    fn an_empty_file_falls_back_and_says_so() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("ranking.txt");
        std::fs::write(&path, "\n\n").unwrap();
        let reading = read(&path);
        assert!(reading.order.is_empty());
        assert!(matches!(reading.fallback, Some(Fallback::Empty)));
        assert!(reading.says().contains("names nothing"));
    }

    #[test]
    fn an_unreadable_path_falls_back_and_says_so() {
        // A directory has metadata but cannot be read as a file — the third
        // of the three fallback kinds, and portable unlike permission bits.
        let dir = tempfile::tempdir().expect("a temp dir");
        let reading = read(dir.path());
        assert!(reading.order.is_empty());
        assert!(matches!(reading.fallback, Some(Fallback::Unreadable(_))));
        assert!(reading.says().contains("could not be read"));
    }

    #[test]
    fn a_populated_file_is_read_in_order_with_an_age() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("ranking.txt");
        std::fs::write(&path, "b\na\nc\n").unwrap();
        let reading = read(&path);
        assert_eq!(reading.order, vec!["b", "a", "c"]);
        assert!(reading.fallback.is_none());
        assert!(reading.modified.is_some());
        assert!(reading.says().contains("3 item(s) named"));
    }

    #[test]
    fn ranked_items_come_first_in_file_order_unranked_keep_their_order() {
        let items = vec!["a", "b", "c", "d"];
        let ranked_ids = vec!["c".to_string(), "a".to_string()];
        let (ordered, unmatched) = order(items, &ranked_ids, |item| item);
        assert_eq!(ordered, vec!["c", "a", "b", "d"]);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn an_id_matching_nothing_is_reported_and_the_rest_still_orders() {
        let items = vec!["a", "b"];
        let ranked_ids = vec!["z".to_string(), "b".to_string()];
        let (ordered, unmatched) = order(items, &ranked_ids, |item| item);
        assert_eq!(ordered, vec!["b", "a"]);
        assert_eq!(unmatched, vec!["z".to_string()]);
    }

    #[test]
    fn a_duplicated_id_is_ranked_once_and_never_reported_unmatched() {
        let items = vec!["a", "b", "c"];
        let ranked_ids = vec!["b".to_string(), "b".to_string(), "a".to_string()];
        let (ordered, unmatched) = order(items, &ranked_ids, |item| item);
        assert_eq!(ordered, vec!["b", "a", "c"]);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn no_ranking_ids_leaves_the_order_untouched() {
        let items = vec!["a", "b", "c"];
        let (ordered, unmatched) = order(items, &[], |item| item);
        assert_eq!(ordered, vec!["a", "b", "c"]);
        assert!(unmatched.is_empty());
    }
}
