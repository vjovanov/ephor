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
///
/// **One of them takes no title.** A product whose spawn has no title option
/// gets a template without `{title}` in it, and the window it opens is named by
/// whatever runs in it. Said here rather than left to be discovered from a
/// window with the wrong name (§REQ-001-boundary.1): the absence is stated, and
/// the module's own test counts it.
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

/// Why a configured binding is not one, or None where it is. Returned rather
/// than printed, so the same sentence reaches a configuration error and an
/// outcome line (§AR-005-capabilities.2).
///
/// A pair is checked as well as a name. The contract is two commands with a
/// place for what they are given: an `open` with no `{command}` in it opens an
/// empty window and prints whatever it prints as the handle, and a `focus` with
/// no `{handle}` brings the same wrong window forward forever. Both are absences
/// that have to be stated rather than degraded into silently (§REQ-001-boundary.1).
pub fn refusal(configured: Option<&Binding>) -> Option<String> {
    match configured {
        Some(Binding::Named(name)) if shipped_named(name).is_none() => Some(format!(
            "'{name}' is not a window ephor ships a binding for; it has: {}. \
             Write your own as {{ \"open\": …, \"focus\": … }}.",
            shipped().join(", ")
        )),
        Some(Binding::Pair { open, .. }) if !open.contains(COMMAND) => Some(format!(
            "the configured window's 'open' has no {COMMAND} in it, so it would open a window \
             with nothing running in it"
        )),
        Some(Binding::Pair { focus, .. }) if !focus.contains(HANDLE) => Some(format!(
            "the configured window's 'focus' has no {HANDLE} in it, so it could never bring the \
             window ephor opened forward"
        )),
        _ => None,
    }
}

/// The tokens a binding is written over. Named, because the refusal that says
/// one is missing has to spell the same thing [`fill`] substitutes.
const TITLE: &str = "{title}";
const COMMAND: &str = "{command}";
const HANDLE: &str = "{handle}";

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
        fill(&self.open, &[(TITLE, title), (COMMAND, command)])
    }

    /// The focus command as it will run.
    pub fn focus_command(&self, handle: &str) -> String {
        fill(&self.focus, &[(HANDLE, handle)])
    }

    /// Open a window running `command`, and hand back the handle the opener
    /// printed (§AR-002-summons.6). Captured, because the handle is the whole
    /// of what comes back across this seam.
    ///
    /// Three answers, and they are three different facts:
    ///
    /// - `Err` — the opener refused and nothing was spawned: a multiplexer with
    ///   no server, a terminal whose remote control is off. The opener's own
    ///   words, reported as the command's output rather than as ephor's,
    ///   because they are (§DA-007-window-is-a-bound-opener).
    /// - `Ok(None)` — it ran the command and named no window. The window
    ///   **exists** and the program in it is running; what is unknown is only
    ///   which window, so nothing can bring it forward. Never confused with the
    ///   refusal: treating it as one used to delete the directory of a job that
    ///   was at that moment running (§FS-005-dispatch.22).
    /// - `Ok(Some(handle))` — the window, and the way back to it.
    ///
    /// The handle is read from standard output **alone**. The opener's
    /// diagnostics travel beside it, so a product that warns after printing its
    /// id — or warns at all on a stream merged into the answer — cannot have the
    /// warning recorded as the window's name (§AR-002-summons.3).
    pub fn open(&self, title: &str, command: &str, site: &Site) -> Result<Option<String>> {
        let answer = summons::run(
            &Summons::new(OPEN_VERB, self.open_command(title, command)),
            site,
            Mode::Captured(OPEN_TIMEOUT),
        )?;
        if !answer.is_done() {
            let said = first_line(answer.errors.as_deref().unwrap_or_default());
            let said = match said.is_empty() {
                true => first_line(answer.output.as_deref().unwrap_or_default()),
                false => said,
            };
            return Err(EphorError::Command(match said.is_empty() {
                true => format!("{} could not open a window", self.name),
                false => format!("{} could not open a window: {said}", self.name),
            }));
        }
        // The handle is the last thing the opener said on its own stream: a
        // product that prints a note first still prints the id it made on a
        // line of its own.
        let handle = answer
            .output
            .as_deref()
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .next_back()
            .unwrap_or_default()
            .to_string();
        Ok((!handle.is_empty()).then_some(handle))
    }

    /// Bring a window forward. The handle is whatever the opener printed and
    /// whatever the product calls a window; ephor never reads into it.
    ///
    /// A multiplexer that renumbered its windows, or a terminal restarted since,
    /// makes this miss — and what comes back is the opener's own error, which is
    /// the honest report (§DA-007-window-is-a-bound-opener.3).
    pub fn focus(&self, handle: &str, site: &Site) -> Result<()> {
        let answer = summons::run(
            &Summons::new(FOCUS_VERB, self.focus_command(handle)),
            site,
            Mode::Captured(OPEN_TIMEOUT),
        )?;
        if answer.is_done() {
            return Ok(());
        }
        let said = first_line(answer.errors.as_deref().unwrap_or_default());
        let said = match said.is_empty() {
            true => first_line(answer.output.as_deref().unwrap_or_default()),
            false => said,
        };
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
    ///
    /// The title is the one thing a product may not take. One of the three has
    /// no title option on its spawn, so its template carries no `{title}` and
    /// the window it opens is named by what runs in it — counted here rather
    /// than left as a silent drop (§REQ-001-boundary.1), so that a fourth
    /// binding losing its title is a failing test and not a surprise.
    #[test]
    fn every_shipped_binding_has_two_verbs_and_a_variable() {
        assert_eq!(shipped().len(), 3);
        for (name, variable, open, focus) in SHIPPED {
            assert!(!variable.is_empty(), "{name} recognizes nothing");
            assert!(open.contains(COMMAND), "{name} runs nothing");
            assert!(focus.contains(HANDLE), "{name} focuses nothing");
        }
        let titled = SHIPPED
            .iter()
            .filter(|(_, _, open, _)| open.contains(TITLE))
            .count();
        assert_eq!(titled, 2, "exactly one shipped spawn takes no title");
    }

    /// The environment ephor was *started in* picks the binding where nothing
    /// is configured, read and never spawned (§FS-005-dispatch.22,
    /// §AR-002-summons.4). The order of [`SHIPPED`] is the precedence: a reader
    /// inside a multiplexer inside a terminal is inside the multiplexer, which
    /// is what their `open` would have to go through anyway. A variable set to
    /// whitespace is not being inside the product, and none of them set means
    /// the terminal, which is the floor.
    ///
    /// Held under [`ENVIRONMENT`] while it runs: the variables it sets belong to
    /// the whole process, and the test harness runs its cases on threads.
    #[test]
    fn the_environment_picks_the_binding_and_nothing_is_spawned_to_find_out() {
        let _guard = ENVIRONMENT.lock().unwrap_or_else(|held| held.into_inner());
        let clear = || {
            for (_, variable, ..) in SHIPPED {
                std::env::remove_var(variable);
            }
        };

        clear();
        assert!(bound(None).is_none(), "nothing set is the terminal");

        for (name, variable, ..) in SHIPPED {
            clear();
            std::env::set_var(variable, "something");
            assert_eq!(bound(None).expect("it recognizes one").name, *name);
        }

        // Two at once: the first shipped binding wins, and the order is the
        // table's rather than the environment's.
        clear();
        for (_, variable, ..) in SHIPPED {
            std::env::set_var(variable, "something");
        }
        assert_eq!(bound(None).expect("one of them").name, SHIPPED[0].0);

        // Set to whitespace is not being inside it.
        clear();
        std::env::set_var(SHIPPED[0].1, "   ");
        assert!(bound(None).is_none(), "a blank variable is nothing");

        // And configuration beats the environment, because a person who wrote
        // one down meant it.
        clear();
        std::env::set_var(SHIPPED[0].1, "something");
        let named = Binding::Named(SHIPPED[2].0.to_string());
        assert_eq!(bound(Some(&named)).expect("configured").name, SHIPPED[2].0);
        clear();
    }

    /// The lock any test that sets these process-wide variables takes, so that
    /// two of them can never be inside the environment at once.
    static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Three answers, and they are three different facts (§AR-002-summons.6):
    /// the opener refused and nothing ran, it ran and named no window, or it
    /// ran and handed back the handle.
    ///
    /// The middle one is not a refusal. The opener's contract is *exit when the
    /// window exists*, so a binding that ran the command and said nothing has a
    /// window open with the program in it — treating that as "the window never
    /// opened" is how a live program loses its record.
    ///
    /// And the handle is read from standard output alone: a product that warns
    /// on standard error still made the window it named, and the warning is not
    /// its name.
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
        assert_eq!(
            silent.open("t", "true", &site(tmp.path())).unwrap(),
            None,
            "a window with no name is still a window"
        );

        let works = Opener {
            name: "stand-in".to_string(),
            open: "printf '@7\\n'".to_string(),
            focus: "true".to_string(),
        };
        assert_eq!(
            works.open("t", "true", &site(tmp.path())).unwrap(),
            Some("@7".to_string())
        );
        works.focus("@7", &site(tmp.path())).expect("it focused");

        // A note on the error stream is a note, not a handle. Merged into one
        // stream it used to become the window's name, and `focus` then missed
        // forever while the row said `running in window <warning text>`.
        let chatty = Opener {
            name: "stand-in".to_string(),
            open: "sh -c \"printf '@7\\n'; printf 'warning: old config\\n' >&2\"".to_string(),
            focus: "true".to_string(),
        };
        assert_eq!(
            chatty.open("t", "true", &site(tmp.path())).unwrap(),
            Some("@7".to_string())
        );

        // And a refusal still says what the opener said, wherever it said it.
        let quiet_refusal = Opener {
            name: "stand-in".to_string(),
            open: "sh -c \"printf 'no session\\n'; exit 1\"".to_string(),
            focus: "true".to_string(),
        };
        let err = quiet_refusal
            .open("t", "true", &site(tmp.path()))
            .expect_err("it refused");
        assert!(err.to_string().contains("no session"), "{err}");
    }

    /// A pair is checked as well as a name (§REQ-001-boundary.1): a template
    /// with nowhere to put what it is given is an absence, and an absence is
    /// stated rather than degraded into. An `open` with no `{command}` opened
    /// an empty window and recorded whatever it printed; a `focus` with no
    /// `{handle}` brought the same wrong window forward forever.
    #[test]
    fn a_pair_with_nowhere_to_put_what_it_is_given_is_refused() {
        let no_command = Binding::Pair {
            open: "my-open {title}".to_string(),
            focus: "my-focus {handle}".to_string(),
        };
        let why = refusal(Some(&no_command)).expect("it is refused");
        assert!(why.contains("{command}"), "{why}");

        let no_handle = Binding::Pair {
            open: "my-open {title} {command}".to_string(),
            focus: "my-focus".to_string(),
        };
        let why = refusal(Some(&no_handle)).expect("it is refused");
        assert!(why.contains("{handle}"), "{why}");

        // A pair with both is a binding, title or no title: a window named by
        // what runs in it is a window.
        let titleless = Binding::Pair {
            open: "my-open -- {command}".to_string(),
            focus: "my-focus {handle}".to_string(),
        };
        assert!(refusal(Some(&titleless)).is_none());
    }
}
