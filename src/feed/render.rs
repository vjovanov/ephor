//! Plain-text rendering for `ephor status` and `ephor feed`, in the same aligned
//! style as `ephor list`. ANSI color only on a tty, honoring NO_COLOR.

use std::io::IsTerminal;

use chrono::{DateTime, Utc};

use crate::feed::cache::{ProjectFeed, Seen};
use crate::feed::model::{Item, ItemKind};

pub struct Style {
    color: bool,
}

impl Style {
    pub fn detect() -> Self {
        Style {
            color: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn red(&self, text: &str) -> String {
        self.paint("31", text)
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    pub fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
}

pub fn age(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    let minutes = (now - then).num_minutes();
    if minutes < 1 {
        return "now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

pub fn render_project(feed: &ProjectFeed, seen: &Seen, style: &Style, recent_days: u64) {
    let now = Utc::now();
    match feed.fetched_at {
        Some(fetched) if age(now, fetched) == "now" => {
            println!("{}  (fetched just now)", style.bold(&feed.project))
        }
        Some(fetched) => {
            println!(
                "{}  (fetched {} ago)",
                style.bold(&feed.project),
                age(now, fetched)
            )
        }
        None => println!("{}  (never fetched)", style.bold(&feed.project)),
    }

    for (name, slot) in &feed.providers {
        if let Some(error) = &slot.error {
            eprintln!("  {} {}", style.dim(&format!("[{name}]")), style.red(error));
        }
    }

    let mut items: Vec<&Item> = feed
        .items()
        .filter(|item| item.is_visible(now, recent_days))
        .collect();
    if items.is_empty() {
        println!("  (no items)");
        return;
    }
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    // The categories of §FS-003-feed-categories.1, in the order they are
    // worked: status, then prs, ci, issues, messages — and Recent last, which
    // is every finished item whatever its kind. The plain renderer prints no
    // headers, so a row's category is its position plus its `[state]`.
    for kind in [
        ItemKind::Status,
        ItemKind::Pr,
        ItemKind::Ci,
        ItemKind::Issue,
        ItemKind::Message,
    ] {
        for item in items
            .iter()
            .filter(|item| item.kind == kind && !item.is_finished())
        {
            println!("{}", render_item_line(item, feed, seen, style, false, now));
        }
    }
    for item in items.iter().filter(|item| item.is_finished()) {
        println!("{}", render_item_line(item, feed, seen, style, false, now));
    }
}

pub fn render_item_line(
    item: &Item,
    feed: &ProjectFeed,
    seen: &Seen,
    style: &Style,
    with_project: bool,
    now: DateTime<Utc>,
) -> String {
    let unread = crate::feed::cache::is_unread(seen, item);
    let marker = if unread { "*" } else { " " };
    let mut line = format!("  {marker} {:<6}", item.kind.label());
    if with_project {
        line.push_str(&format!("{:<12}  ", item.project));
    }
    line.push_str(&format!("{:<4}  ", age(now, item.updated_at)));

    let mut title = item.title.clone();
    if let Some(state) = &item.state {
        title = format!("{title}  [{state}]");
    }
    if let Some(gate) = crate::feed::gate::Gate::of(item) {
        title = format!("{title}  {}", gate.summary());
        let breakdown = gate.breakdown();
        if !breakdown.is_empty() {
            title = format!("{title}  {}", style.dim(&format!("({breakdown})")));
        }
    }
    if feed.is_stale(&item.source) {
        title = style.dim(&format!("{title} (stale)"));
    } else if item.needs_response {
        title = style.bold(&format!("{title}  ← needs response"));
    }
    line.push_str(&title);
    if let Some(url) = &item.url {
        line.push_str(&format!("  {}", style.dim(url)));
    }
    line
}

pub fn render_summary_row(
    feed: &ProjectFeed,
    seen: &Seen,
    recent_days: u64,
) -> (String, usize, usize, usize) {
    let now = Utc::now();
    let visible = || {
        feed.items()
            .filter(|item| item.is_visible(now, recent_days))
    };
    let unread = visible()
        .filter(|item| crate::feed::cache::is_unread(seen, item))
        .count();
    let needs = visible()
        .filter(|item| item.needs_response && crate::feed::cache::is_unread(seen, item))
        .count();
    let failing = visible()
        .filter(|item| item.kind == ItemKind::Ci && item.needs_response)
        .count();
    (feed.project.clone(), unread, needs, failing)
}
