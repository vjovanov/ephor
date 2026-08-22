//! A window of the reader's own: the window opener (§FS-005-dispatch.22,
//! §AR-002-summons.6, §DA-007-window-is-a-bound-opener).
//!
//! ephor has one terminal and is sitting in it. Handing it over works
//! everywhere and stays the floor; a reader inside a multiplexer, or in a
//! terminal that opens windows on request, has a better move available — the
//! program in a window of its own, ephor still on screen, and *open* from then
//! on meaning bring that window forward.
//!
//! The contract is in materials (§REQ-001-boundary.1): **one command that opens
//! a window running a given command and prints a handle for the window it
//! made, and one that brings a handle forward**. The opener exits when the
//! window exists, not when the program ends.
//!
//! This module is the whole of the coupling. The three shipped bindings are
//! products, and their names are spelled here and nowhere else in the tree
//! (§REQ-001-boundary.5) — `scripts/check_boundary.py` fails the build on a
//! fourth spelling anywhere.
//!
//! **A window is the reader's.** ephor opens one and brings it forward. It
//! never closes one and never ends what is in it (§FS-005-dispatch.22): a
//! window the reader closed is a job that ended, however it ended, and the lock
//! says so without being asked.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{EphorError, Result};
use crate::seams::summons::{self, quote, Mode, Site, Summons};

/// Which binding fills the seam: a shipped one by name, or a pair of commands
/// of the reader's own (§AR-002-summons.6). `window` in site configuration.
///
/// A pair is the fourth binding: `open` is a template over `{title}` and
/// `{command}`, `focus` a template over `{handle}`, each substituted
/// shell-quoted. Anything that can print a handle and take one back can fill
/// the seam, which is what makes it a seam rather than a list.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    Named(String),
    Pair { open: String, focus: String },
}

/// One binding, resolved: the two commands, and the name it answers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opener {
    /// What it is called, for the line that says where the program went.
    pub name: String,
    open: String,
    focus: String,
}

/// The bindings ephor ships (§DA-007-window-is-a-bound-opener): a terminal
/// multiplexer's new window, and the remote-control spawn of two terminals that
/// offer one. Each carries the environment variable its product sets for
/// exactly this purpose — read to recognize where ephor is running, never
/// spawned to find out (§AR-002-summons.4).
///
/// `{title}` and `{command}` in the open template and `{handle}` in the focus
/// template are substituted shell-quoted; every other brace is the product's
/// own format language and is left alone.
const SHIPPED: &[(&str, &str, &str, &str)] = &[
    (
        "tmux",
        "TMUX",
        "tmux new-window -P -F '#{window_id}' -n {title} -- {command}",
        "tmux select-window -t {handle}",
    ),
    (
        "wezterm",
        "WEZTERM_PANE",
        "wezterm cli spawn --new-window -- sh -c {command}",
        "wezterm cli activate-pane --pane-id {handle}",
    ),
    (
        "kitty",
        "KITTY_WINDOW_ID",
        "kitty @ launch --type=os-window --title {title} sh -c {command}",
        "kitty @ focus-window --match id:{handle}",
    ),
];

/// Every shipped binding's name, for a message that has to list them.
pub fn shipped() -> Vec<&'static str> {
    SHIPPED.iter().map(|(name, ..)| *name).collect()
}

/// The opener bound here, or None where nothing is bound and nothing is
/// recognized — the terminal is the floor then, and the floor is never removed
/// (§DA-007-window-is-a-bound-opener).
///
/// Configuration first, because a person who wrote one down meant it. With
/// nothing configured the environment ephor was *started in* is read: each
/// product sets a variable for exactly this, and reading one is free. Nothing
/// is ever spawned to find out which terminal this is — discovery by failure is
/// what §AR-002-summons.4 rules out.
pub fn bound(configured: Option<&Binding>) -> Option<Opener> {
    match configured {
        Some(Binding::Pair { open, focus }) => Some(Opener {
            name: "configured".to_string(),
            open: open.clone(),
            focus: focus.clone(),
        }),
        Some(Binding::Named(name)) => shipped_named(name),
        None => recognized(),
    }
}

/// Why a configured name is not a binding, or None where it is. Returned rather
/// than printed, so the same sentence reaches a configuration error and an
/// outcome line (§AR-005-capabilities.2).
pub fn refusal(configured: Option<&Binding>) -> Option<String> {
    match configured {
        Some(Binding::Named(name)) if shipped_named(name).is_none() => Some(format!(
            "'{name}' is not a window ephor ships a binding for; it has: {}. \
             Write your own as {{ \"open\": …, \"focus\": … }}.",
            shipped().join(", ")
        )),
        _ => None,
    }
}

fn shipped_named(name: &str) -> Option<Opener> {
    SHIPPED
        .iter()
        .find(|(known, ..)| known.eq_ignore_ascii_case(name))
        .map(|(name, _, open, focus)| Opener {
            name: (*name).to_string(),
            open: (*open).to_string(),
            focus: (*focus).to_string(),
        })
}

/// The binding for the environment ephor was started in, where it is one of the
/// shipped ones. A variable set to nothing is not being inside the product.
fn recognized() -> Option<Opener> {
    SHIPPED
        .iter()
        .find(|(_, variable, ..)| {
            std::env::var(variable)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
        .map(|(name, _, open, focus)| Opener {
            name: (*name).to_string(),
            open: (*open).to_string(),
            focus: (*focus).to_string(),
        })
}

/// The verbs these fill, for messages.
const OPEN_VERB: &str = "window.open";
const FOCUS_VERB: &str = "window.focus";

/// How long the opener gets. It exits when the window exists, not when the
/// program in it ends (§AR-002-summons.6), so this is a spawn and a round trip
/// to a multiplexer or a terminal's control socket — seconds are generous. A
/// binding that hung here would hang the screen that pressed the key.
const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

impl Opener {
    /// The open command as it will run, for a message or a test.
    pub fn open_command(&self, title: &str, command: &str) -> String {
        fill(&self.open, &[("{title}", title), ("{command}", command)])
    }

    /// The focus command as it will run.
    pub fn focus_command(&self, handle: &str) -> String {
        fill(&self.focus, &[("{handle}", handle)])
    }

    /// Open a window running `command`, and hand back the handle the opener
    /// printed (§AR-002-summons.6). Captured, because the handle is the whole
    /// of what comes back across this seam.
    ///
    /// `Err` is the opener's own words: a multiplexer with no server, a
    /// terminal whose remote control is off. Reported as the command's output
    /// rather than as ephor's, because it is (§DA-007-window-is-a-bound-opener).
    pub fn open(&self, title: &str, command: &str, site: &Site) -> Result<String> {
        let binding = format!("{} 2>&1", self.open_command(title, command));
        let answer = summons::run(
            &Summons::new(OPEN_VERB, binding),
            site,
            Mode::Captured(OPEN_TIMEOUT),
        )?;
        let printed = answer.output.as_deref().unwrap_or("").trim().to_string();
        if !answer.is_done() {
            return Err(EphorError::Command(match printed.is_empty() {
                true => format!("{} could not open a window", self.name),
                false => format!(
                    "{} could not open a window: {}",
                    self.name,
                    first_line(&printed)
                ),
            }));
        }
        // The handle is the last thing the opener said: a product that warns
        // first still prints the id it made on its own line.
        let handle = printed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .next_back()
            .unwrap_or_default()
            .to_string();
        match handle.is_empty() {
            true => Err(EphorError::Command(format!(
                "{} opened a window but printed no handle, so nothing could bring it forward",
                self.name
            ))),
            false => Ok(handle),
        }
    }

    /// Bring a window forward. The handle is whatever the opener printed and
    /// whatever the product calls a window; ephor never reads into it.
    ///
    /// A multiplexer that renumbered its windows, or a terminal restarted since,
    /// makes this miss — and what comes back is the opener's own error, which is
    /// the honest report (§DA-007-window-is-a-bound-opener.3).
    pub fn focus(&self, handle: &str, site: &Site) -> Result<()> {
        let binding = format!("{} 2>&1", self.focus_command(handle));
        let answer = summons::run(
            &Summons::new(FOCUS_VERB, binding),
            site,
            Mode::Captured(OPEN_TIMEOUT),
        )?;
        if answer.is_done() {
            return Ok(());
        }
        let said = first_line(answer.output.as_deref().unwrap_or(""));
        Err(EphorError::Command(match said.is_empty() {
            true => format!("{} could not bring window {handle} forward", self.name),
            false => format!(
                "{} could not bring window {handle} forward: {said}",
                self.name
            ),
        }))
    }
}

/// The template with its placeholders filled, each substituted shell-quoted so
/// a title with a space stays one word. Only these three tokens are
/// substituted: every other brace belongs to the product's own format language
/// and is left exactly as it was written.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut filled = template.to_string();
    for (token, value) in values {
        filled = filled.replace(token, &quote(value));
    }
    filled
}

/// The first line of what a product printed that says anything.
fn first_line(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// Where the opener runs. It matters to nothing the seam promises — the
/// supervisor resolves its own place from the job it was handed
/// (§AR-002-summons.5), and a run is reached by its id — so this is only
/// somewhere that exists.
pub fn site(at: &Path) -> Site {
    Site::root(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shipped binding is chosen by name, and a name nothing ships is refused
    /// with the ones that are — never silently dropped (§REQ-001-boundary.1).
    #[test]
    fn a_binding_is_named_or_written_out() {
        let named = Binding::Named("tmux".to_string());
        let opener = bound(Some(&named)).expect("it ships one");
        assert_eq!(opener.name, "tmux");
        assert!(refusal(Some(&named)).is_none());

        let unknown = Binding::Named("no-such-terminal".to_string());
        assert!(bound(Some(&unknown)).is_none());
        let why = refusal(Some(&unknown)).expect("it is refused");
        assert!(why.contains("no-such-terminal"), "{why}");
        assert!(why.contains("tmux"), "{why}");

        // The fourth binding is a pair of commands, and needs nothing shipped.
        let pair = Binding::Pair {
            open: "my-open {title} -- {command}".to_string(),
            focus: "my-focus {handle}".to_string(),
        };
        let opener = bound(Some(&pair)).expect("a pair is a binding");
        assert_eq!(
            opener.open_command("watch run 3f9a2c", "ephor job run /j"),
            "my-open 'watch run 3f9a2c' -- 'ephor job run /j'"
        );
        assert_eq!(opener.focus_command("@7"), "my-focus '@7'");
        assert!(refusal(Some(&pair)).is_none());
    }

    /// The product's own format language survives filling: only the three
    /// tokens are substituted, and a title with a space stays one word.
    #[test]
    fn only_the_three_tokens_are_filled_and_they_are_quoted() {
        let opener = shipped_named("tmux").expect("it ships one");
        let command = opener.open_command("run 3f9a2c", "ephor job run /my jobs/j");
        assert_eq!(
            command,
            "tmux new-window -P -F '#{window_id}' -n 'run 3f9a2c' -- 'ephor job run /my jobs/j'"
        );
        assert_eq!(opener.focus_command("@7"), "tmux select-window -t '@7'");
    }

    /// Each shipped binding prints a handle and takes one back, and each names
    /// the variable its product sets for exactly this purpose.
    #[test]
    fn every_shipped_binding_has_two_verbs_and_a_variable() {
        assert_eq!(shipped().len(), 3);
        for (name, variable, open, focus) in SHIPPED {
            assert!(!variable.is_empty(), "{name} recognizes nothing");
            assert!(open.contains("{command}"), "{name} runs nothing");
            assert!(focus.contains("{handle}"), "{name} focuses nothing");
        }
    }

    /// A binding that could not open a window says so in the opener's own
    /// words, and one that opened a window and printed nothing is refused
    /// rather than recorded with an empty handle — a job nothing could bring
    /// forward is worse than one that never opened.
    #[test]
    fn what_the_opener_said_is_what_comes_back() {
        let tmp = tempfile::tempdir().unwrap();
        let refuses = Opener {
            name: "stand-in".to_string(),
            open: "sh -c \"printf 'no server running\\n' >&2; exit 1\"".to_string(),
            focus: "exit 1".to_string(),
        };
        let err = refuses
            .open("t", "true", &site(tmp.path()))
            .expect_err("it refused");
        assert!(err.to_string().contains("no server running"), "{err}");

        let silent = Opener {
            name: "stand-in".to_string(),
            open: "true".to_string(),
            focus: "true".to_string(),
        };
        let err = silent
            .open("t", "true", &site(tmp.path()))
            .expect_err("no handle is no window ephor can reach");
        assert!(err.to_string().contains("printed no handle"), "{err}");

        let works = Opener {
            name: "stand-in".to_string(),
            open: "printf '@7\\n'".to_string(),
            focus: "true".to_string(),
        };
        assert_eq!(works.open("t", "true", &site(tmp.path())).unwrap(), "@7");
        works.focus("@7", &site(tmp.path())).expect("it focused");
    }
}
