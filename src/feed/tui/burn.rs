//! The Burn page: what this machine is spending on agents (§FS-013-burn.8).
//!
//! Watch-only, and built off the draw path like the operations board. The
//! reading is the API's — the same one `ephor burn --json` prints — so the
//! page and the command cannot come apart about what a window holds or what
//! `unpriced` looks like (§AR-009-surfaces.1).
//!
//! Nothing here reads a transcript. `w` and `B` change what is asked for and
//! the shell re-asks the API; the store behind it is refreshed from the tick,
//! never while drawing and never while a key is held (§FS-013-burn.8).

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::api::views;
use crate::burn::query::{By, Window};
use crate::burn::render;

use super::{Action, Screen, BURN_TICK};

/// How many live sessions the strip shows before it says how many more there
/// are. The reading itself is never cut — this is the page's own choice about
/// what fits above the table the strip introduces.
const STRIP: usize = 6;

pub(crate) struct BurnScreen {
    reading: views::Burn,
    /// What the next reading asks for. Held here rather than read back off
    /// the last reading, so a cycle is not lost to a window that came back
    /// empty.
    pub window: Window,
    pub by: By,
    scroll: u16,
    viewport: u16,
}

impl BurnScreen {
    pub fn new(reading: views::Burn, window: Window, by: By) -> Self {
        BurnScreen {
            reading,
            window,
            by,
            scroll: 0,
            viewport: 0,
        }
    }

    pub fn title(&self) -> String {
        " ephor — burn".to_string()
    }

    /// The keys, said on the page rather than left to be discovered. `$` is
    /// what opened this and is what closes it again.
    pub fn footer(&self) -> &'static str {
        " w window  B by (project/model/session/plan/matter)  R rescan  j/k move  esc/$ back"
    }

    /// A fresh reading under the same window and grouping — what the tick
    /// hands back when the store has moved (§FS-013-burn.8).
    pub fn replace(&mut self, reading: views::Burn) {
        self.reading = reading;
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                Action::Back
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Action::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.scroll = 0;
                Action::None
            }
            KeyCode::Char('f') | KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.viewport.max(1));
                Action::None
            }
            KeyCode::Char('b') | KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.viewport.max(1));
                Action::None
            }
            // The two cycles, and the two arguments of the command that
            // carries them (§FS-013-burn.6, §REQ-002-parity.2).
            KeyCode::Char('w') => {
                self.window = self.window.next();
                Action::ReadBurn
            }
            KeyCode::Char('B') => {
                self.by = self.by.next();
                Action::ReadBurn
            }
            // Reading the transcripts on a keypress is the one thing this
            // page must not do while drawing — it is the shell's move, off
            // the draw path, exactly as the command's `--rescan` is
            // (§FS-013-burn.8).
            KeyCode::Char('R') => Action::RescanBurn,
            _ => Action::None,
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let heading = dim.add_modifier(Modifier::BOLD);
        let mut lines = Vec::new();
        let reading = &self.reading;

        lines.push(Line::from(vec![
            Span::styled("  burn".to_string(), heading),
            Span::styled(
                format!(
                    "   window {}  ·  by {}  ·  {} lens",
                    reading.window, reading.by, reading.lens
                ),
                dim,
            ),
        ]));
        let spans: Vec<u64> = reading.now.spans.iter().map(|span| span.tokens).collect();
        lines.push(Line::from(vec![
            Span::styled(
                format!("    now  {:<12}  ", render::rate(reading.now.rate)),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(render::spark(&spans), Style::default().fg(Color::Cyan)),
            Span::styled("   5-min spans over the window".to_string(), dim),
        ]));
        for row in reading.now.live.iter().take(STRIP) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("      {:<22} {:<16}", row.model, row.project),
                    Style::default(),
                ),
                Span::styled(format!("{:>10}   live", render::rate(row.rate)), dim),
                Span::styled(
                    match row.subagent {
                        true => " · sub-agent".to_string(),
                        false => String::new(),
                    },
                    dim,
                ),
            ]));
        }
        // The strip is the busiest sessions, not all of them, and it says so
        // rather than trailing off. A machine running two dozen agents at once
        // would otherwise push the table it introduces off the screen — which
        // the machine form never does, because it carries every row.
        if let Some(rest) = reading.now.live.len().checked_sub(STRIP).filter(|n| *n > 0) {
            lines.push(Line::from(Span::styled(
                format!("      and {rest} more going — `ephor burn --json` has every one"),
                dim,
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "  {:<30} {:>9} {:>9} {:>9} {:>9}  {}",
                reading.by, "in", "out", "cache-r", "cache-w", "cost"
            ),
            heading,
        )));
        if reading.groups.is_empty() {
            lines.push(Line::from(Span::styled(
                "    nothing recorded in this window".to_string(),
                dim,
            )));
        }
        for row in &reading.groups {
            lines.push(Line::from(Span::styled(row_of(row), Style::default())));
        }
        lines.push(Line::from(Span::styled(
            format!(
                "  {:<30} {:>9} {:>9} {:>9} {:>9}  {}",
                "total",
                render::tokens(reading.totals.input),
                render::tokens(reading.totals.output),
                render::tokens(reading.totals.cache_read),
                render::tokens(reading.totals.cache_write),
                render::cost(reading.totals.cost_usd),
            ),
            heading,
        )));

        // What the lens could not measure is on the page, never left to be
        // read out of a number that looks low (§FS-013-burn.2).
        if let Some(says) = &reading.says {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(format!("    {says}"), dim)));
        }
        // The rule between the lenses, said out loud: the reader is looking
        // at one of two records that are never added together
        // (§FS-013-burn.1).
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            match reading.lens.as_str() {
                "work" => "    what the runtime metered, per plan — never added to the machine's own totals"
                    .to_string(),
                _ => "    what this machine burned, from the agent tools' own logs — B for what the runtime metered"
                    .to_string(),
            },
            dim,
        )));
        lines
    }

    pub fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        self.viewport = area.height;
        let lines = self.lines();
        let max_scroll = lines.len().saturating_sub(area.height as usize);
        self.scroll = self.scroll.min(max_scroll as u16);
        frame.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), area);
    }
}

/// One grouped row as the page draws it. The cost is the shared spelling, so
/// an unknown price looks the same here as on the command line
/// (§FS-013-burn.7).
fn row_of(row: &views::BurnGroup) -> String {
    let named = match &row.detail {
        Some(detail) => format!("{} ({detail})", row.key),
        None => row.key.clone(),
    };
    format!(
        "    {:<28} {:>9} {:>9} {:>9} {:>9}  {}",
        clipped(&named, 28),
        render::tokens(row.spend.input),
        render::tokens(row.spend.output),
        render::tokens(row.spend.cache_read),
        render::tokens(row.spend.cache_write),
        render::cost(row.spend.cost_usd),
    )
}

fn clipped(name: &str, width: usize) -> String {
    match name.chars().count() > width {
        true => format!("{}…", name.chars().take(width - 1).collect::<String>()),
        false => name.to_string(),
    }
}

/// What the shell does about this page (§FS-013-burn.8).
///
/// An inherent `impl` is the crate's rather than a module's, so the behaviour
/// behind the page's keys lives beside the page instead of in the interface's
/// own module — which is where the screens written before it are still
/// moving to. Everything here is off the draw path: opening, re-reading and
/// the tick all run in the shell, and `draw` above only draws.
impl super::App {
    /// Open the burn page over whatever is on screen, or close it back to
    /// where the reader was (§FS-013-burn.8).
    ///
    /// Opening refreshes the store first, and only where it has gone stale —
    /// the same rule the command follows before it prints, and off the draw
    /// path in both cases.
    pub(super) fn toggle_burn(&mut self) {
        if matches!(self.screen, Screen::Burn(_)) {
            self.screen = self.saved.take().unwrap_or(Screen::Navigator);
            return;
        }
        self.ctx.burn_refresh(false);
        let window = crate::burn::query::Window::default();
        let by = crate::burn::query::By::default();
        let reading = self.ctx.burn(window, by);
        let page = Screen::Burn(BurnScreen::new(reading, window, by));
        let previous = std::mem::replace(&mut self.screen, page);
        if !matches!(previous, Screen::Operations(_)) {
            self.saved = Some(previous);
        }
        self.burn_ticked_at = std::time::Instant::now();
    }

    /// Take the reading again under whatever the page is now asking for. A
    /// closed page costs nothing.
    pub(super) fn reload_burn(&mut self) -> bool {
        let Screen::Burn(page) = &self.screen else {
            return false;
        };
        let (window, by) = (page.window, page.by);
        let reading = self.ctx.burn(window, by);
        if let Screen::Burn(page) = &mut self.screen {
            page.replace(reading);
        }
        true
    }

    /// Refresh the burn store from the tick, where the page over it is open
    /// (§FS-013-burn.8).
    ///
    /// Never on the draw path, and never oftener than [`BURN_TICK`]: a scan
    /// stats every transcript on the machine before it opens any of them,
    /// which is cheap and is not free. With the page closed nothing is read
    /// at all — the store is refreshed by whoever is looking at it.
    pub(super) fn tick_burn(&mut self) -> bool {
        if !matches!(self.screen, Screen::Burn(_)) {
            return false;
        }
        if self.burn_ticked_at.elapsed() < BURN_TICK {
            return false;
        }
        self.burn_ticked_at = std::time::Instant::now();
        self.ctx.burn_refresh(false);
        self.reload_burn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spend(tokens: u64, cost: Option<f64>) -> views::Spend {
        views::Spend {
            input: tokens,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            tokens,
            cost_usd: cost,
            priced: cost.is_some(),
        }
    }

    fn reading() -> views::Burn {
        views::Burn {
            window: "1h".to_string(),
            by: "project".to_string(),
            lens: "machine".to_string(),
            from: chrono::Utc::now(),
            to: chrono::Utc::now(),
            totals: spend(15, None),
            groups: vec![
                views::BurnGroup {
                    key: "app".to_string(),
                    detail: None,
                    spend: spend(10, None),
                },
                views::BurnGroup {
                    key: "lib".to_string(),
                    detail: None,
                    spend: spend(5, Some(0.0)),
                },
            ],
            now: views::BurnNow {
                rate: 41_000,
                live: Vec::new(),
                spans: Vec::new(),
            },
            says: None,
        }
    }

    fn drawn(screen: &BurnScreen) -> String {
        screen
            .lines()
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The distinction §FS-013-burn.7 is about, on the screen rather than
    /// only in the document: a row nobody priced says `unpriced`, and one
    /// priced at nothing says `$0.00`.
    #[test]
    fn the_page_never_draws_an_unknown_price_as_a_zero() {
        let screen = BurnScreen::new(reading(), Window::Hour, By::Project);
        let page = drawn(&screen);
        let app = page.lines().find(|line| line.contains("app")).expect("app");
        assert!(app.contains("unpriced"), "{app}");
        let lib = page.lines().find(|line| line.contains("lib")).expect("lib");
        assert!(lib.contains("$0.00"), "{lib}");
    }

    /// The two cycles ask for a new reading rather than computing one, so
    /// nothing is read while a key is held (§FS-013-burn.8).
    #[test]
    fn the_two_keys_cycle_and_ask_the_shell_for_the_reading() {
        let mut screen = BurnScreen::new(reading(), Window::Hour, By::Project);
        assert!(matches!(
            screen.handle_key(KeyCode::Char('w')),
            Action::ReadBurn
        ));
        assert_eq!(screen.window, Window::SixHours);
        assert!(matches!(
            screen.handle_key(KeyCode::Char('B')),
            Action::ReadBurn
        ));
        assert_eq!(screen.by, By::Model);
        assert!(matches!(
            screen.handle_key(KeyCode::Char('R')),
            Action::RescanBurn
        ));
        // Paging is paging, on this page as on every other.
        assert!(matches!(
            screen.handle_key(KeyCode::Char('b')),
            Action::None
        ));
        assert!(matches!(screen.handle_key(KeyCode::Esc), Action::Back));
    }

    /// Which lens the reader is looking at is on the page, because the two
    /// are never added together and a reader has to know which they have
    /// (§FS-013-burn.1).
    #[test]
    fn the_page_says_which_lens_it_is_showing() {
        let machine = BurnScreen::new(reading(), Window::Hour, By::Project);
        assert!(drawn(&machine).contains("machine lens"));
        let mut work = reading();
        work.lens = "work".to_string();
        work.by = "plan".to_string();
        work.says = Some("measured: codex — 3 invocations reported none".to_string());
        let work = BurnScreen::new(work, Window::Hour, By::Plan);
        let page = drawn(&work);
        assert!(page.contains("work lens"));
        assert!(
            page.contains("never added to the machine's own totals"),
            "{page}"
        );
        assert!(page.contains("measured: codex"), "{page}");
    }
}
