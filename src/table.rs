//! Plain aligned-text table for `ephor list`, byte-compatible with the Python
//! `print_project_table` output.

use serde_json::Value;

use crate::paths;
use crate::registry::{branch_entries, id_of, str_field};

pub fn print_project_table(projects: &[&Value]) {
    if projects.is_empty() {
        println!("No projects found.");
        return;
    }

    const COLS: [&str; 6] = ["id", "org", "type", "branch", "branches", "root"];
    const HEADERS: [&str; 6] = ["Project", "Org", "Type", "Branch", "Branches", "Root"];

    let rows: Vec<[String; 6]> = projects
        .iter()
        .map(|project| {
            let release_count = branch_entries(project, "release_branches").len();
            let branch_count = branch_entries(project, "branches").len();
            let branches_str = if release_count > 0 {
                format!("{release_count}r/{branch_count}b")
            } else {
                format!("{branch_count}b")
            };
            [
                id_of(project).to_string(),
                project
                    .get("organization")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
                str_field(project, "type").unwrap_or("").to_string(),
                paths::resolve_path(str_field(project, "root").unwrap_or(""))
                    .to_string_lossy()
                    .into_owned(),
                str_field(project, "main_branch").unwrap_or("-").to_string(),
                branches_str,
            ]
        })
        .collect();

    // Row layout is [id, org, type, root, branch, branches] to mirror the
    // Python dict; column order below is id, org, type, branch, branches, root.
    let col_index = |col: &str| match col {
        "id" => 0,
        "org" => 1,
        "type" => 2,
        "root" => 3,
        "branch" => 4,
        "branches" => 5,
        _ => unreachable!(),
    };

    let widths: Vec<usize> = COLS
        .iter()
        .zip(HEADERS.iter())
        .map(|(col, header)| {
            rows.iter()
                .map(|row| row[col_index(col)].len())
                .chain(std::iter::once(header.len()))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let header_line: Vec<String> = HEADERS
        .iter()
        .zip(widths.iter())
        .map(|(header, width)| format!("{header:<width$}"))
        .collect();
    println!("{}", header_line.join("  "));
    let separator: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    println!("{}", separator.join("  "));

    for row in &rows {
        let cells: Vec<String> = COLS
            .iter()
            .zip(widths.iter())
            .map(|(col, width)| format!("{:<width$}", row[col_index(col)]))
            .collect();
        println!("{}", cells.join("  "));
    }
}
