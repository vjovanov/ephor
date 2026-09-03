//! The parity list: every ability, the keys that reach it, and the command
//! that carries it (§REQ-002-parity.5).
//!
//! Written down where the build can read it. A convention nobody can run is a
//! convention that has already drifted, so this list is checked twice: the
//! test below holds every command named here to the actual command tree, and
//! `scripts/check_parity.py` holds every key the interface binds to this list
//! — a key with no command behind it fails the build, exactly as a product
//! literal outside its adapter does (§AR-001-layers.2).
//!
//! An ability is a key that reveals a fact or changes the world
//! (§REQ-002-parity.1). Everything else the screen does is presentation and is
//! listed in [`PRESENTATION`], which is the exemption stated rather than the
//! absence left to be read as an oversight.

/// One ability: what it is, the keys that reach it, and the command line that
/// carries it. The command is named by its path — `work offers`, not the
/// whole invocation — because arguments are the surface's business and the
/// ability is not.
pub struct Ability {
    pub what: &'static str,
    /// The keys the interface binds to it, across every screen.
    pub keys: &'static [&'static str],
    /// The command that carries it, as `ephor <path>`.
    pub command: &'static str,
}

/// Every ability both surfaces owe (§REQ-002-parity.2).
pub const ABILITIES: &[Ability] = &[
    Ability {
        what: "what may be done about a matter or a branch",
        keys: &["x"],
        command: "actions",
    },
    Ability {
        what: "run one of those entries",
        keys: &["enter", "1-9"],
        command: "actions run",
    },
    Ability {
        what: "open what is already going about a row, rather than starting it again",
        keys: &["enter"],
        command: "actions open",
    },
    Ability {
        what: "pick who a piece of work goes to, for this dispatch alone",
        keys: &["t"],
        command: "actions run --hand",
    },
    Ability {
        what: "make the branch workspace a row says is not there",
        keys: &["C"],
        command: "checkout",
    },
    Ability {
        what: "a matter's recorded conversation",
        keys: &["v"],
        command: "thread",
    },
    Ability {
        what: "post a reaction on a message",
        keys: &["+"],
        command: "react",
    },
    Ability {
        what: "tick a task the source reported",
        keys: &["t"],
        command: "tick",
    },
    Ability {
        what: "send the reply a run drafted",
        keys: &["p"],
        command: "reply",
    },
    Ability {
        what: "what is being done about a matter, and what could be",
        keys: &["w"],
        command: "work offers",
    },
    Ability {
        what: "ask a matter for something no recipe covers",
        keys: &["a"],
        command: "work ask",
    },
    Ability {
        what: "reopen work whose matter has moved",
        keys: &["s"],
        command: "work sync",
    },
    Ability {
        what: "take a ticket back",
        keys: &["c"],
        command: "work cancel",
    },
    Ability {
        what: "let the runtime work a plan",
        keys: &["R"],
        command: "work run",
    },
    Ability {
        what: "what laying a workflow down would write, before it writes it",
        keys: &["p"],
        command: "work lay --dry-run",
    },
    Ability {
        what: "the tickets that are over, shown or folded away",
        keys: &["z"],
        command: "work offers --all",
    },
    Ability {
        what: "the forge's own reasons for refusing a merge",
        keys: &["c"],
        command: "failures",
    },
    Ability {
        what: "every operation beneath the reading",
        keys: &[";"],
        command: "operations",
    },
    Ability {
        what: "watch a live run by attaching to it",
        keys: &["a"],
        command: "operations attach",
    },
    // The Burn page and its two keys (§FS-013-burn.8). Each one shows a fact
    // nothing else on either surface shows — a different window and a
    // different grouping are different numbers — so each is an ability rather
    // than a mode, and each is carried by an argument of the same command.
    Ability {
        what: "what this machine is spending on agents",
        keys: &["$"],
        command: "burn",
    },
    Ability {
        what: "how far back the spending is counted",
        keys: &["w"],
        command: "burn --window",
    },
    // `B` and not `b`, which is a page up on every screen that scrolls. A key
    // cannot be exempt and owed at once — that is the exemption swallowing a
    // command that was owed — so the grouping takes the shifted key and says
    // why here (§REQ-002-parity.1).
    Ability {
        what: "how the spending is grouped, and so which lens answers",
        keys: &["B"],
        command: "burn --by",
    },
    Ability {
        what: "read the transcripts again rather than waiting for the store to go stale",
        keys: &["R"],
        command: "burn --rescan",
    },
    Ability {
        what: "what a job wrote",
        keys: &["L"],
        command: "job log",
    },
    Ability {
        what: "mark a matter read",
        keys: &["m", "d", "space"],
        command: "mark-read",
    },
    Ability {
        what: "mark everything in view read",
        keys: &["a"],
        command: "mark-read",
    },
    Ability {
        what: "show only what is unread",
        keys: &["u"],
        command: "feed --unread",
    },
    Ability {
        what: "fetch what the sources have now",
        keys: &["r"],
        command: "refresh",
    },
];

/// Keys that move a cursor, change a mode, or hand something the command line
/// already names to the reader's own pager, editor or browser
/// (§REQ-002-parity.1). Exempt, and exempt in writing: an absence here reads
/// exactly like an oversight.
pub const PRESENTATION: &[(&str, &str)] = &[
    ("j", "move down"),
    ("k", "move up"),
    ("g", "to the top"),
    ("G", "to the end"),
    ("f", "a page down"),
    ("b", "a page up"),
    ("h", "back"),
    // `l` goes *in* — into a thread, a plan, and on a row that says *running*
    // into the thing that is running (§FS-005-dispatch.21). That last one is an
    // ability, and it is carried by a command: `ephor actions open` is the same
    // move (§FS-011-command-line.8). The key stays here rather than moving to
    // [`ABILITIES`] because it is the one motion key that happens to land on an
    // ability on one row, and a key on both lists is refused by the check.
    ("l", "in"),
    ("n", "the next project"),
    ("P", "the previous project"),
    ("q", "leave"),
    ("o", "open it in the browser — the URL is on the reading"),
    (
        "e",
        "open it in the editor or pager — the path is on the reading",
    ),
    ("[", "the previous project"),
    ("]", "the next project"),
    // The keys a terminal sends as their own codes rather than as characters.
    // They do what the letters beside them do, and they are written down for
    // the same reason those are: an absence here reads exactly like an
    // oversight, and until the check could see them it *was* one.
    ("up", "move up"),
    ("down", "move down"),
    ("left", "back"),
    ("right", "in"),
    ("home", "to the top"),
    ("end", "to the end"),
    ("pageup", "a page up"),
    ("pagedown", "a page down"),
    ("ctrl+c", "leave, however the terminal says it"),
    // Editing the line the reader is typing into, which is neither a reading
    // nor a change to anything outside the process: the line becomes an
    // argument on the command line, and how it was typed does not
    // (§REQ-002-parity.2).
    ("ctrl+u", "erase the line being typed"),
    ("ctrl+w", "erase the word being typed"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Every command this list names is a command that exists. Renaming one
    /// out from under the law is the drift §REQ-002-parity.5 exists to catch,
    /// and it is caught here rather than by a reader trying the command.
    #[test]
    fn every_ability_names_a_command_that_exists() {
        let cli = crate::cli::Cli::command();
        for ability in ABILITIES {
            // The path, without the arguments: `work offers --all` is the
            // `work offers` command, and which flags it takes is the surface's
            // business (§AR-009-surfaces.1).
            let path: Vec<&str> = ability
                .command
                .split_whitespace()
                .take_while(|word| !word.starts_with('-'))
                .collect();
            let mut at = &cli;
            for name in &path {
                at = at
                    .get_subcommands()
                    .find(|command| {
                        command.get_name() == *name
                            || command.get_all_aliases().any(|alias| alias == *name)
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "'{}' carries no command: `ephor {}` does not resolve",
                            ability.what, ability.command
                        )
                    });
            }
        }
    }

    /// A flag an ability names is a flag that command takes. `--hand` moving
    /// to a different spelling would leave the list pointing at nothing.
    #[test]
    fn every_flag_an_ability_names_is_one_that_command_takes() {
        let cli = crate::cli::Cli::command();
        for ability in ABILITIES {
            let words: Vec<&str> = ability.command.split_whitespace().collect();
            let flags: Vec<&str> = words
                .iter()
                .filter(|word| word.starts_with("--"))
                .map(|word| word.trim_start_matches('-'))
                .collect();
            if flags.is_empty() {
                continue;
            }
            let mut at = &cli;
            for name in words.iter().take_while(|word| !word.starts_with('-')) {
                at = at
                    .get_subcommands()
                    .find(|command| command.get_name() == *name)
                    .expect("the path resolves, which the test above proved");
            }
            for flag in flags {
                assert!(
                    at.get_arguments().any(|arg| arg.get_long() == Some(flag)),
                    "`ephor {}` takes no --{flag}",
                    ability.command
                );
            }
        }
    }

    /// No key is both an ability and presentation. A key in both lists is the
    /// exemption quietly swallowing a command that was owed.
    #[test]
    fn no_key_is_exempt_and_owed_at_once() {
        for ability in ABILITIES {
            for key in ability.keys {
                assert!(
                    !PRESENTATION.iter().any(|(exempt, _)| exempt == key),
                    "'{key}' is listed as presentation and as the key for '{}'",
                    ability.what
                );
            }
        }
    }
}
