//! Who can be asked: the roster, the binding's fourth verb (§AR-007-runtime.1).
//!
//! Work is handed to an agent carrying a model at an effort, and two of those
//! follow from the first — so the roster has one axis the reader chooses and
//! one dependent on it, and it is read from the binding's own registry rather
//! than kept as a list of ephor's (§FS-005-dispatch.14,
//! §DA-004-roster-is-asked-not-configured). The binding's grammar — its
//! settings files, their merge order, the `agent[mode]:provider:model`
//! selector its plans carry — is parsed and rendered here and nowhere else
//! (§REQ-001-boundary.5): above this module a hand is an opaque id.
//!
//! Choosing one of them is [`resolve`], seven steps deep, because who does a
//! piece of work is a thing a project defaults and a person overrides
//! (§FS-006-project-interface.9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::work::headroom::{self, Evidence, Member};
use crate::work::recipe::{HandList, HandPin, DEFAULT_HAND};

/// One entry of the roster (§FS-005-dispatch.14): a named choice that knows
/// its carrier. The id is what configuration names; agent, model and provider
/// are shown so the reader knows what they are choosing, never chosen apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hand {
    /// The name configuration uses — opaque above this module.
    pub id: String,
    /// The agent summoned. None where the binding declares a model profile
    /// with no carrier anywhere, which `available` explains.
    pub agent: Option<String>,
    /// The concrete model that agent will carry. None where the agent runs
    /// with its own default.
    pub model: Option<String>,
    /// Who serves that model, where the profile says.
    pub provider: Option<String>,
    /// The efforts this entry declares — the binding's modes on the carrying
    /// agent, in the order it declares them, which is the binding's own order
    /// for them. Declaring none is an answer, not a gap (§FS-005-dispatch.14):
    /// such a hand is asked plainly.
    pub efforts: Vec<String>,
    /// None where the hand is ready; the one sentence why where it is not.
    /// Computed where the roster is read — the command is looked for, never
    /// spawned to fail (§AR-002-summons.4) — and shown beside the entry
    /// rather than hiding it (§FS-005-dispatch.14).
    pub available: Option<String>,
}

impl Hand {
    /// What this hand resolves to, for a reader deciding whether to choose it.
    pub fn resolves_to(&self) -> String {
        let agent = self.agent.as_deref().unwrap_or("no agent");
        match (&self.provider, &self.model) {
            (Some(provider), Some(model)) => format!("{agent} · {provider}:{model}"),
            (None, Some(model)) => format!("{agent} · {model}"),
            _ => format!("{agent} · its own default model"),
        }
    }

    /// The binding's own spelling of this hand — the execution-target
    /// selector a plan's `**Target:**` carries — rendered here and nowhere
    /// else (§FS-005-dispatch.14). None where the hand carries no model of
    /// its own: that hand is handed to the binding as its agent flags, not as
    /// a selector, and the binding picks the model itself.
    pub fn target(&self, effort: Option<&str>) -> Option<String> {
        let agent = self.agent.as_deref()?;
        let model = self.model.as_deref()?;
        let mode = effort.map(|e| format!("[{e}]")).unwrap_or_default();
        Some(match self.provider.as_deref() {
            Some(provider) => format!("{agent}{mode}:{provider}:{model}"),
            None => format!("{agent}{mode}:{model}"),
        })
    }
}

/// Everyone who can be asked, or why nobody can.
#[derive(Debug, Clone)]
pub struct Roster {
    pub hands: Vec<Hand>,
    /// Why the roster is empty (§FS-005-dispatch.14) — with no runtime there
    /// is nobody to ask, in the workable rung's own words; a settings file
    /// that does not parse empties it too, with the file named — or None
    /// where the runtime is there and the hands speak for themselves.
    pub refusal: Option<String>,
    /// What reading this roster had to say that takes nothing away: today,
    /// that the work root's overlay answered under the deprecated `.agents/`
    /// name, or was passed over there for its home
    /// (§FS-006-project-interface.12). It is news and not a fault, so it is
    /// carried here rather than in `refusal`, which empties the roster
    /// (§FS-005-dispatch.14).
    pub notes: Vec<String>,
}

impl Roster {
    /// Whether any hand carries a concrete model of its own. A roster with
    /// hands but no such entry is the agent-default-only state whose remedy
    /// `capabilities` needs to explain (§FS-005-dispatch.14).
    pub(crate) fn has_model_carrying_hand(&self) -> bool {
        self.hands
            .iter()
            .any(|hand| hand.agent.is_some() && hand.model.is_some())
    }
}

/// How the shipped binding grows a nameable model-carrying hand. This owns
/// the Rhei settings grammar for both the roster renderer and a missing-name
/// refusal, so neither caller has to know a binding-specific key or path
/// (§REQ-001-boundary.5).
pub(crate) fn model_profile_help(id: Option<&str>) -> String {
    match id {
        Some(id) => format!(
            "a model profile named '{id}' with an agent carrier in the Rhei settings `models` \
             registry (normally `~/.config/rhei/settings.json`) creates '{id}' as a nameable \
             model-carrying hand"
        ),
        None => "model profiles with an agent carrier in the Rhei settings `models` registry \
                 (normally `~/.config/rhei/settings.json`) create nameable model-carrying hands"
            .to_string(),
    }
}

/// The roster, read from the binding's merged settings: built-in profiles,
/// then the person's global file, then the work root's overlay
/// (§DA-004-roster-is-asked-not-configured). `root` is the execution root a
/// project overlay would live under; None reads the site-wide roster.
pub fn roster(config: &crate::work::recipe::WorkConfig, root: Option<&Path>) -> Roster {
    if let Some(reason) = super::refusal(config) {
        return Roster {
            hands: Vec::new(),
            refusal: Some(reason),
            notes: Vec::new(),
        };
    }
    let global = match read_settings(&global_settings_path()) {
        Ok(settings) => settings,
        Err(reason) => {
            return Roster {
                hands: Vec::new(),
                refusal: Some(reason),
                notes: Vec::new(),
            }
        }
    };
    // Where the overlay was found is said whichever way the file then reads:
    // a file that does not parse still empties the roster, and the reader is
    // still told which of the two names it was read under
    // (§FS-006-project-interface.12).
    let overlay = root.and_then(project_settings);
    let notes = overlay
        .as_ref()
        .and_then(|found| found.note.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let project = match &overlay {
        Some(found) => match read_settings(&found.path) {
            Ok(settings) => settings,
            Err(reason) => {
                return Roster {
                    hands: Vec::new(),
                    refusal: Some(reason),
                    notes,
                }
            }
        },
        None => SettingsDoc::default(),
    };
    Roster {
        hands: enumerate(&merge(built_in_agents(), global.typed, project)),
        refusal: None,
        notes,
    }
}

/// What answered "who does this piece of work" (§FS-005-dispatch.14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Nobody chose, so the binding picks unasked — the seventh step. `note` is
    /// what the reader should know about that silence: that a hand was named
    /// and there is nobody to name it to, or that a narrowing cannot bind what
    /// nobody chose (§FS-006-project-interface.9).
    Unasked { note: Option<String> },
    /// This hand, at this effort. `whence` names the step that answered, in
    /// the reader's words, so a surprising answer says where it came from.
    Chosen {
        hand: Hand,
        effort: Option<String>,
        whence: String,
        note: Option<String>,
        /// The pool this hand's work is bought against
        /// (§FS-005-dispatch.29), so that what refuses a start can be
        /// recorded against what actually refused it.
        pool: Option<String>,
        /// What choosing among the list had to say, in ephor's own words —
        /// which member it went to, which were passed over and until when.
        /// None where there was nothing to choose among and nothing to say:
        /// a single name with no evidence bearing on it answers exactly as a
        /// bare name always did. Written onto the ticket rather than into the
        /// runtime's plan language, which is the runtime's
        /// (§REQ-001-boundary.1).
        said: Option<String>,
    },
    /// The choice cannot stand here, and this is the whole reason
    /// (§FS-006-project-interface.9): never dropped, never quietly replaced.
    Refused(String),
}

impl Choice {
    /// What the reader should know about this answer, where anything.
    pub fn note(&self) -> Option<&str> {
        match self {
            Choice::Unasked { note } | Choice::Chosen { note, .. } => note.as_deref(),
            Choice::Refused(_) => None,
        }
    }

    /// What the ticket records about who got this work and why
    /// (§FS-005-dispatch.29). None where nothing was chosen among.
    pub fn said(&self) -> Option<&str> {
        match self {
            Choice::Chosen { said, .. } => said.as_deref(),
            _ => None,
        }
    }

    /// The pool the chosen hand's work is bought against, where one was
    /// chosen.
    pub fn pool(&self) -> Option<&str> {
        match self {
            Choice::Chosen { pool, .. } => pool.as_deref(),
            _ => None,
        }
    }

    /// What this choice pins on a ticket: `(target, model)` in the binding's
    /// own words, rendered by [`Hand::target`] and nowhere else
    /// (§FS-005-dispatch.14). Both None where nothing can be pinned — nobody
    /// was chosen, or the hand names an agent with no model of its own, which
    /// the plan language has no line for: that choice rides the run instead,
    /// as [`Choice::flags`].
    pub fn pin(&self) -> (Option<String>, Option<String>) {
        match self {
            Choice::Chosen { hand, effort, .. } => (hand.target(effort.as_deref()), None),
            _ => (None, None),
        }
    }

    /// The run-flag spelling of this choice — the one the plan language
    /// cannot carry (§FS-005-dispatch.14). Some only for a chosen hand that
    /// names an agent and no model of its own; a hand that carries a model is
    /// pinned on the ticket by [`Choice::pin`] and never travels as flags —
    /// one choice binds in one spelling, and the ticket's full line is the
    /// stronger one: the binding resolves such a ticket from the line alone,
    /// with the run's agent flags invisible to it.
    pub fn flags(&self) -> Option<HandFlags> {
        match self {
            Choice::Chosen { hand, effort, .. } if hand.target(effort.as_deref()).is_none() => {
                hand.agent.clone().map(|agent| HandFlags {
                    agent,
                    effort: effort.clone(),
                })
            }
            _ => None,
        }
    }
}

/// A chosen hand as run flags: the spelling for the choice the plan language
/// has no line for (§FS-005-dispatch.14). The fields are facts about the
/// choice; the flag words themselves are rendered where the run invocation is
/// built, beside the plan flag, as part of the same coupling
/// (§AR-007-runtime.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandFlags {
    /// The agent summoned, in the binding's own id.
    pub agent: String,
    /// The effort chosen, where one was — the binding's mode on that agent.
    pub effort: Option<String>,
}

impl HandFlags {
    /// Who is getting this run, in one phrase — the same sentence on every
    /// surface that starts one, because the command line and the key in the
    /// interface are the same run (§FS-005-dispatch.14).
    pub fn describe(&self) -> String {
        match &self.effort {
            Some(effort) => format!("agent {} at {effort}", self.agent),
            None => format!("agent {}", self.agent),
        }
    }
}

/// Who does one action (§FS-005-dispatch.14, §FS-006-project-interface.9), in
/// the order that spec sets: what the reader picked for this dispatch alone,
/// the pin the action or recipe carries, the project's table — this action's
/// id, then its default — then the site's table read the same way, and where
/// none of them answered, whatever the binding picks unasked.
///
/// The order is the binding's own resolution mirrored deliberately, so the two
/// cannot come to disagree about what one configuration means: `--model` /
/// `--agent` over the state's own words over the project's settings over the
/// person's is the same shape, narrow before broad, with the caller's choice
/// at the top (§DA-004-roster-is-asked-not-configured).
pub fn resolve(
    roster: &Roster,
    site: &crate::work::recipe::WorkConfig,
    project: Option<&crate::work::recipe::ProjectWorkConfig>,
    action: &str,
    picked: Option<&HandList>,
    pinned: Option<&HandList>,
    evidence: &Evidence,
) -> Choice {
    let permitted: &[String] = project.map_or(&[], |work| work.permitted_hands.as_slice());
    let table = |work: Option<&BTreeMap<String, HandList>>, key: &str| {
        work.and_then(|hands| hands.get(key)).cloned()
    };
    let project_hands = project.map(|work| &work.hands);
    let steps = [
        (
            "what you picked for this dispatch".to_string(),
            picked.cloned(),
        ),
        (format!("the hand pinned on '{action}'"), pinned.cloned()),
        (
            format!("this project's hand for '{action}'"),
            table(project_hands, action),
        ),
        (
            "this project's default hand".to_string(),
            table(project_hands, DEFAULT_HAND),
        ),
        (
            format!("the site's hand for '{action}'"),
            table(Some(&site.hands), action),
        ),
        (
            "the site's default hand".to_string(),
            table(Some(&site.hands), DEFAULT_HAND),
        ),
    ];
    let Some((whence, pin)) = steps
        .into_iter()
        .find_map(|(whence, pin)| pin.map(|pin| (whence, pin)))
    else {
        // The seventh step. A narrowing cannot reach in here — what the binding
        // would pick unasked is not something ephor was told
        // (§FS-006-project-interface.9), so the silence is said out loud.
        return Choice::Unasked {
            note: (!permitted.is_empty()).then(|| {
                format!(
                    "this project permits only {}, and nothing names a hand for '{action}' — \
                     the runtime picks unasked, which the narrowing cannot bind",
                    permitted.join(", ")
                )
            }),
        };
    };
    // A step that answered has answered (§FS-005-dispatch.14): everything below
    // is about the list it carried, and no later step is consulted because a
    // member of it turned out to be unreachable.
    //
    // Permission first, and against every member: it is policy, and policy runs
    // before evidence. One unpermitted name refuses the whole list — never
    // filtered down to the permitted members, because a policy that quietly
    // used the second choice is indistinguishable from one that was never
    // asked (§FS-006-project-interface.9). Against the name rather than the
    // roster, so it holds with no runtime bound too.
    for member in pin.members() {
        if let Some(why) = refuse_narrowed(permitted, &whence, member) {
            return Choice::Refused(why);
        }
    }
    // Nobody to be named to: the roster's own sentence — the workable rung's,
    // or the settings file that would not parse (§FS-005-dispatch.14). The
    // ticket is written all the same.
    if let Some(refusal) = &roster.refusal {
        return Choice::Unasked {
            note: Some(format!(
                "{whence} is {}, and nobody can be asked: {refusal}",
                pin.describe()
            )),
        };
    }
    // Every member is settled against the roster before any of them is chosen,
    // so a name that is not a hand refuses the list rather than being skipped
    // over: a member silently dropped is the same erosion of a written pin that
    // a silently dropped narrowing is (§FS-006-project-interface.9).
    let mut members: Vec<Member> = Vec::new();
    for pin in pin.members() {
        match settle(roster, pin, &whence) {
            Ok(member) => members.push(member),
            Err(why) => return Choice::Refused(why),
        }
    }
    // Which of them can be reached right now — a fact about the world, and no
    // judgment at all (§FS-005-dispatch.29). It may veto and it may not
    // reorder, so what comes back is an index into the order as written.
    let (index, mut said) = headroom::choose(&members, evidence);
    let member = &members[index];
    // A pin naming one hand and meeting no evidence has nothing to record: it
    // answered exactly as a bare name always did, and the ticket's own
    // execution line already says who. A pin naming alternates always has
    // something, because which of them it went to is the thing a reader who
    // was not there cannot work out from the line alone.
    if said.is_none() && pin.is_alternates() {
        said = Some(format!(
            "'{}' takes this — the first hand named here, and nothing says its pool \
             cannot be had right now",
            member.hand.id
        ));
    }
    chosen(
        member.hand.clone(),
        member.effort.clone(),
        whence,
        member.note.clone(),
        Some(member.pool.clone()),
        said,
    )
}

/// One written member, settled against the roster: which hand it names, at
/// what effort, and which pool its work would be bought against.
fn settle(roster: &Roster, pin: &HandPin, whence: &str) -> std::result::Result<Member, String> {
    let member = |hand: Hand, effort, note| Member {
        pool: headroom::pool_of(&hand),
        hand,
        effort,
        note,
    };
    match pin {
        HandPin::Named { id, effort } => {
            let Some(hand) = roster.hands.iter().find(|hand| &hand.id == id) else {
                return Err(format!(
                    "{whence} names '{id}', which is not a hand here (the roster has: {}); {}",
                    roster
                        .hands
                        .iter()
                        .map(|hand| hand.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    model_profile_help(Some(id))
                ));
            };
            let (effort, note) = settle_effort(hand, effort.as_deref(), whence)?;
            Ok(member(hand.clone(), effort, note))
        }
        // A pair the registry never enumerated is accepted with a note rather
        // than refused: ephor cannot prove it invalid, and a proxy serving a
        // model the registry does not list is exactly what it is for
        // (§FS-006-project-interface.9).
        HandPin::Spelled {
            agent,
            model,
            effort,
        } => match roster.hands.iter().find(|hand| {
            hand.agent.as_deref() == Some(agent.as_str())
                && hand.model.as_deref() == Some(model.as_str())
        }) {
            Some(known) => {
                let (effort, note) = settle_effort(known, effort.as_deref(), whence)?;
                Ok(member(known.clone(), effort, note))
            }
            None => {
                let note = format!(
                    "{whence} spells out {}, which the runtime's registry does not list — \
                     ephor cannot check the pair, and passes it through as written",
                    pin.describe()
                );
                Ok(member(
                    Hand {
                        id: format!("{agent}:{model}"),
                        agent: Some(agent.clone()),
                        model: Some(model.clone()),
                        provider: None,
                        efforts: Vec::new(),
                        available: None,
                    },
                    effort.clone(),
                    Some(note),
                ))
            }
        },
    }
}

/// The hands a picker may offer on a project (§FS-005-dispatch.14): the
/// roster's, without those the project's narrowing excludes. An unavailable
/// hand stays, with its reason — its time is wrong, not its name — but a hand
/// the project does not permit is not a choice at the moment of asking at
/// all: what the narrowing refuses loudly is a *named* choice
/// (§FS-006-project-interface.9), and a picker offering a name only to refuse
/// it would teach the policy one wasted keystroke at a time.
pub fn pickable(
    roster: &Roster,
    project: Option<&crate::work::recipe::ProjectWorkConfig>,
) -> Vec<Hand> {
    let permitted: &[String] = project.map_or(&[], |work| work.permitted_hands.as_slice());
    roster
        .hands
        .iter()
        .filter(|hand| permitted.is_empty() || permitted.iter().any(|name| name == &hand.id))
        .cloned()
        .collect()
}

/// Whether a project's narrowing lets this work be pinned by something no hand
/// id named — a selector written out in the binding's own words. Public
/// because such a pin is a recipe's field rather than a hand, and the caller
/// that reads it is the one that has to refuse it
/// (§FS-006-project-interface.9).
pub fn refuse_unnamed(
    project: Option<&crate::work::recipe::ProjectWorkConfig>,
    what: &str,
) -> Option<String> {
    let permitted = project.map_or(&[] as &[String], |work| work.permitted_hands.as_slice());
    (!permitted.is_empty()).then(|| {
        format!(
            "{what}, which no hand named — this project permits only {}",
            permitted.join(", ")
        )
    })
}

/// A hand outside the project's narrowing, refused with that reason. A hand
/// spelled out in full is outside it by construction: nothing in the list
/// authorized that pair (§FS-006-project-interface.9).
fn refuse_narrowed(permitted: &[String], whence: &str, pin: &HandPin) -> Option<String> {
    if permitted.is_empty() {
        return None;
    }
    match pin {
        HandPin::Named { id, .. } if permitted.iter().any(|name| name == id) => None,
        HandPin::Named { id, .. } => Some(format!(
            "{whence} names '{id}', which this project does not permit (it permits: {})",
            permitted.join(", ")
        )),
        HandPin::Spelled { .. } => Some(format!(
            "{whence} spells out {}, which no hand named — this project permits only {}",
            pin.describe(),
            permitted.join(", ")
        )),
    }
}

/// The effort a choice of this hand runs at, settled where the choice is made
/// (§FS-005-dispatch.14): a named effort must be one the hand declares; a
/// choice naming none is completed where the hand declares exactly one — a
/// single declared effort is a fact about the hand, not a choice left open,
/// and the completion is said in the returned note — and refused where it
/// declares several, because the binding's two spellings do not agree on what
/// an effort-less ask would mean and neither answer is the reader's choice. A
/// hand declaring none is asked plainly. Computed before anything is written,
/// which is the binding's refusal moved off the spawn (§AR-002-summons.4).
fn settle_effort(
    hand: &Hand,
    asked: Option<&str>,
    whence: &str,
) -> std::result::Result<(Option<String>, Option<String>), String> {
    match asked {
        Some(effort) if hand.efforts.iter().any(|declared| declared == effort) => {
            Ok((Some(effort.to_string()), None))
        }
        Some(effort) => Err(match hand.efforts.is_empty() {
            true => format!(
                "{whence} asks '{}' for effort '{effort}', and it declares none — ask it plainly",
                hand.id
            ),
            false => format!(
                "{whence} asks '{}' for effort '{effort}', which it does not declare (it declares: {})",
                hand.id,
                hand.efforts.join(", ")
            ),
        }),
        None => match hand.efforts.as_slice() {
            [] => Ok((None, None)),
            [only] => Ok((
                Some(only.clone()),
                Some(format!(
                    "{whence} is '{}' with no effort named — '{only}' is the one it declares, \
                     and it is asked at it",
                    hand.id
                )),
            )),
            several => Err(format!(
                "{whence} names '{}' and no effort, and it declares several ({}) — name one, \
                 as '{}:<effort>'",
                hand.id,
                several.join(", "),
                hand.id
            )),
        },
    }
}

/// The chosen hand, with what the reader should know about it: why it cannot
/// be asked right now, or that it names an agent the plan language has no line
/// for — appended to whatever the choosing itself had to say. All of it notes
/// rather than refusals — a ticket is written before it is run, and who it is
/// for is worth recording either way.
fn chosen(
    hand: Hand,
    effort: Option<String>,
    whence: String,
    note: Option<String>,
    pool: Option<String>,
    said: Option<String>,
) -> Choice {
    let standing = match &hand.available {
        Some(why) => Some(format!("cannot be asked right now: {why}")),
        None => hand.target(effort.as_deref()).is_none().then(|| {
            "names no model of its own — the ticket pins nobody, and the choice rides each \
             run as the runtime's own agent flags"
                .to_string()
        }),
    };
    let note = match (note, standing) {
        (Some(said), Some(fact)) => Some(format!("{said}; and it {fact}")),
        (Some(said), None) => Some(said),
        (None, Some(fact)) => Some(format!("{whence} is '{}', which {fact}", hand.id)),
        (None, None) => None,
    };
    Choice::Chosen {
        hand,
        effort,
        whence,
        note,
        pool,
        said,
    }
}

/// Where the binding keeps a person's settings. Fixed like the plan flag:
/// rebinding `work.runner` swaps the word that executes, never the language
/// and homes of the coupling (§AR-007-runtime.2).
fn global_settings_path() -> PathBuf {
    crate::paths::home_dir()
        .join(".config")
        .join(super::RUNNER)
        .join("settings.json")
}

/// The overlay a work root may carry, found where the toolchain keeps its own
/// files: under the home first, then the deprecated `.agents/` name
/// (§FS-006-project-interface.12). Composed from the same word as the global
/// path, so rebinding `work.runner` swaps the word that executes and never
/// where the settings are read from (§AR-007-runtime.2). None where the root
/// carries no overlay under either name.
fn project_settings(root: &Path) -> Option<crate::grounds::Found> {
    crate::grounds::under_the_home(Path::new(super::RUNNER).join("settings.json")).find(root)
}

/// The names of a profile's named flag sets, in declaration order — the
/// binding keeps its modes ordered as written, and "the first declared mode"
/// is an answer its own resolution gives, so sorting them here would be a
/// quiet disagreement.
#[derive(Debug, Clone, Default, PartialEq)]
struct Modes(Vec<String>);

impl<'de> Deserialize<'de> for Modes {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Modes, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Modes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of mode names to flag sets")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Modes, A::Error> {
                let mut names = Vec::new();
                while let Some((name, _)) = map.next_entry::<String, serde::de::IgnoredAny>()? {
                    names.push(name);
                }
                Ok(Modes(names))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// One agent transport profile, as the binding's settings spell it. Only what
/// the roster needs is read; the rest of the entry is the runner's business.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
struct AgentProfile {
    #[serde(default)]
    command: Vec<String>,
    /// Named flag sets; the roster needs only the names, which are the
    /// efforts a hand on this agent declares.
    #[serde(default)]
    modes: Modes,
}

impl AgentProfile {
    fn efforts(&self) -> Vec<String> {
        self.modes.0.clone()
    }

    /// Why this agent cannot be summoned, or None where its command is
    /// there. Looked for, never spawned (§AR-002-summons.4).
    fn available(&self) -> Option<String> {
        let Some(word) = self.command.first() else {
            return Some("declares no command to run".to_string());
        };
        if word.contains('/') {
            if Path::new(word).is_file() {
                None
            } else {
                Some(format!("{word} is not on disk"))
            }
        } else if crate::feed::provider::command_exists(word) {
            None
        } else {
            Some(format!("{word} is not on PATH"))
        }
    }
}

/// One model profile: the semantic identity the binding's configuration
/// names, knowing its provider, its concrete model, and its carriers.
#[derive(Debug, Clone, Default, Deserialize)]
struct ModelProfile {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    default_agent: Option<String>,
    /// Per-agent launch bindings; the roster needs only the keys — each is a
    /// carrier this model is declared to run under.
    #[serde(default)]
    agents: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SettingsDefaults {
    #[serde(default)]
    agent: Option<String>,
}

/// One settings layer, as far as the roster reads it.
#[derive(Debug, Clone, Default, Deserialize)]
struct Settings {
    #[serde(default)]
    defaults: SettingsDefaults,
    /// The binding's older spelling of `defaults.agent`, still read below it.
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    agents: BTreeMap<String, AgentProfile>,
    #[serde(default)]
    models: BTreeMap<String, ModelProfile>,
}

/// One settings file, typed and raw at once: the binding merges an overlay by
/// which fields the file *wrote*, not by which are non-null — an explicit
/// `null` clears an inherited value — and only the raw document can tell the
/// two apart (§DA-004-roster-is-asked-not-configured).
#[derive(Debug, Clone, Default)]
struct SettingsDoc {
    raw: serde_json::Value,
    typed: Settings,
}

/// A settings file read leniently — a layer that is not there is empty — but
/// never guessed at: a file that exists and does not parse is a refusal with
/// the file named, because a roster read around it would be a list missing
/// whatever the person just added (§AR-002-summons.4, §FS-005-dispatch.14).
fn read_settings(path: &Path) -> std::result::Result<SettingsDoc, String> {
    if !path.is_file() {
        return Ok(SettingsDoc::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("{} cannot be read: {err}", path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("{} does not parse: {err}", path.display()))?;
    let typed: Settings = serde_json::from_value(raw.clone())
        .map_err(|err| format!("{} does not parse: {err}", path.display()))?;
    Ok(SettingsDoc { raw, typed })
}

/// Whether a raw JSON object wrote this key at all — `null` included, which is
/// how the binding lets an overlay clear what it inherits.
fn field_present(raw: &serde_json::Value, key: &str) -> bool {
    raw.as_object().is_some_and(|obj| obj.contains_key(key))
}

fn child<'a>(raw: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    raw.as_object()
        .and_then(|obj| obj.get(key))
        .unwrap_or(&serde_json::Value::Null)
}

/// The merged registry the hands are enumerated from.
struct Registry {
    agents: BTreeMap<String, AgentProfile>,
    models: BTreeMap<String, ModelProfile>,
    /// The settings-level default carrier — `defaults.agent`, then the older
    /// top-level `agent`, each merged project-over-global by field presence —
    /// which the binding reads *before* a model profile's own carrier.
    default_agent: Option<String>,
}

/// The binding's merge, mirrored: built-ins seed the agents, global then
/// project entries replace an id wholesale; model entries merge field-wise
/// by field presence — an explicit `null` clears — with their carrier maps
/// united by agent id; defaults override per field, by presence too
/// (§DA-004-roster-is-asked-not-configured).
fn merge(
    builtin: BTreeMap<String, AgentProfile>,
    global: Settings,
    project: SettingsDoc,
) -> Registry {
    let SettingsDoc {
        raw: project_raw,
        typed: project,
    } = project;
    let mut agents = builtin;
    agents.extend(global.agents);
    agents.extend(project.agents);

    let mut models = global.models;
    let models_raw = child(&project_raw, "models");
    for (id, overlay) in project.models {
        let overlay_raw = child(models_raw, &id);
        match models.get_mut(&id) {
            Some(existing) => {
                if field_present(overlay_raw, "provider") {
                    existing.provider = overlay.provider;
                }
                if field_present(overlay_raw, "model") {
                    existing.model = overlay.model;
                }
                if field_present(overlay_raw, "default_agent") {
                    existing.default_agent = overlay.default_agent;
                }
                existing.agents.extend(overlay.agents);
            }
            None => {
                models.insert(id, overlay);
            }
        }
    }

    let defaults_agent = match field_present(child(&project_raw, "defaults"), "agent") {
        true => project.defaults.agent,
        false => global.defaults.agent,
    };
    let legacy_agent = match field_present(&project_raw, "agent") {
        true => project.agent,
        false => global.agent,
    };
    Registry {
        agents,
        models,
        default_agent: defaults_agent.or(legacy_agent),
    }
}

/// Every hand the registry declares (§FS-005-dispatch.14): each model profile
/// under its carrier and under each agent it binds arguments for, then each
/// agent standing alone with its own default model. Never a cross-product —
/// the pairings are the ones the registry wrote down. Ids are unique: an
/// agent whose name a model profile claimed keeps its stand-alone hand under
/// `@<agent>`, the same `@` that already reads "on this agent" in a pairing's
/// id — two rows under one name would make one of them unaddressable.
fn enumerate(registry: &Registry) -> Vec<Hand> {
    let mut hands = Vec::new();
    for (id, profile) in &registry.models {
        // The plain id is the pairing the binding itself would pick for this
        // model beneath a state's own words: the settings-level default
        // carrier first, the profile's own only below it — the order the
        // binding resolves a carrier in.
        let carrier = registry
            .default_agent
            .clone()
            .or_else(|| profile.default_agent.clone());
        match &carrier {
            Some(agent) => hands.push(model_hand(id.clone(), profile, agent, registry)),
            // A model nobody carries is shown with the reason the binding
            // would refuse it with, not hidden (§FS-005-dispatch.14).
            None => hands.push(Hand {
                id: id.clone(),
                agent: None,
                model: profile.model.clone(),
                provider: profile.provider.clone(),
                efforts: Vec::new(),
                available: Some("names no agent, and no default agent is configured".to_string()),
            }),
        }
        for agent in profile.agents.keys() {
            if Some(agent) == carrier.as_ref() {
                continue;
            }
            hands.push(model_hand(
                format!("{id}@{agent}"),
                profile,
                agent,
                registry,
            ));
        }
    }
    for (id, profile) in &registry.agents {
        // The binding's model and agent registries are separate namespaces,
        // so a model profile may share an agent's name — and the profile
        // holds the plain id, since that is what the binding's own
        // configuration means by it.
        let hand_id = match registry.models.contains_key(id) {
            true => format!("@{id}"),
            false => id.clone(),
        };
        hands.push(Hand {
            id: hand_id,
            agent: Some(id.clone()),
            model: None,
            provider: None,
            efforts: profile.efforts(),
            available: profile.available(),
        });
    }
    hands
}

fn model_hand(id: String, profile: &ModelProfile, agent: &str, registry: &Registry) -> Hand {
    let (efforts, available) = match registry.agents.get(agent) {
        Some(agent_profile) => (agent_profile.efforts(), agent_profile.available()),
        None => (
            Vec::new(),
            Some(format!("names agent '{agent}', which is not declared")),
        ),
    };
    let available = available.or_else(|| {
        profile
            .model
            .is_none()
            .then(|| "declares no model name".to_string())
    });
    Hand {
        id,
        agent: Some(agent.to_string()),
        model: profile.model.clone(),
        provider: profile.provider.clone(),
        efforts,
        available,
    }
}

/// The binding's seed registry, spelled once here so a machine whose settings
/// add nothing still has a roster. A settings entry with the same id replaces
/// one of these wholesale, exactly as it does in the binding
/// (§DA-004-roster-is-asked-not-configured.3 names the accepted drift risk).
fn built_in_agents() -> BTreeMap<String, AgentProfile> {
    let entry = |command: &[&str], modes: &[&str]| AgentProfile {
        command: command.iter().map(|word| (*word).to_string()).collect(),
        modes: Modes(modes.iter().map(|name| (*name).to_string()).collect()),
    };
    [
        ("claude-code", entry(&["claude"], &["yolo"])),
        ("codex", entry(&["codex", "exec"], &["yolo"])),
        ("cursor", entry(&["cursor-agent"], &["yolo"])),
        ("gemini", entry(&["gemini"], &["yolo"])),
        ("kilocode", entry(&["kilo"], &["yolo"])),
        // pi has no permission layer, so it declares no modes: a hand with no
        // efforts is asked plainly (§FS-005-dispatch.14).
        ("pi", entry(&["pi"], &[])),
    ]
    .into_iter()
    .map(|(id, profile)| (id.to_string(), profile))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Settings {
        serde_json::from_str(text).unwrap()
    }

    fn doc(text: &str) -> SettingsDoc {
        let raw: serde_json::Value = serde_json::from_str(text).unwrap();
        SettingsDoc {
            typed: serde_json::from_value(raw.clone()).unwrap(),
            raw,
        }
    }

    fn registry(global: &str, project: &str) -> Registry {
        merge(built_in_agents(), parse(global), doc(project))
    }

    fn hand<'a>(hands: &'a [Hand], id: &str) -> &'a Hand {
        hands
            .iter()
            .find(|hand| hand.id == id)
            .unwrap_or_else(|| panic!("no hand '{id}' in {:?}", ids(hands)))
    }

    fn ids(hands: &[Hand]) -> Vec<&str> {
        hands.iter().map(|hand| hand.id.as_str()).collect()
    }

    /// The binding's example settings enumerate to its own names: each model
    /// profile under its carrier, and every agent standing alone
    /// (§FS-005-dispatch.14). No cross-product — `review-deep` does not grow
    /// a claude-code variant nobody declared.
    #[test]
    fn the_roster_is_the_registry_read_out_not_a_grid() {
        let hands = enumerate(&registry(
            r#"{
                "models": {
                    "impl-fast": {
                        "provider": "anthropic", "model": "claude-sonnet-4-6",
                        "default_agent": "claude-code",
                        "agents": { "claude-code": {}, "codex": {} }
                    },
                    "review-deep": {
                        "provider": "openai", "model": "o3",
                        "default_agent": "codex",
                        "agents": { "codex": {} }
                    }
                }
            }"#,
            "{}",
        ));
        assert_eq!(
            ids(&hands),
            vec![
                "impl-fast",
                "impl-fast@codex",
                "review-deep",
                "claude-code",
                "codex",
                "cursor",
                "gemini",
                "kilocode",
                "pi",
            ]
        );
        let fast = hand(&hands, "impl-fast");
        assert_eq!(fast.agent.as_deref(), Some("claude-code"));
        assert_eq!(fast.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(fast.provider.as_deref(), Some("anthropic"));
        // The dependent axis: efforts are the carrying agent's modes.
        assert_eq!(fast.efforts, vec!["yolo"]);
        assert_eq!(
            hand(&hands, "impl-fast@codex").agent.as_deref(),
            Some("codex")
        );
        // An agent standing alone runs with its own default model.
        let alone = hand(&hands, "claude-code");
        assert_eq!(alone.model, None);
        assert_eq!(alone.resolves_to(), "claude-code · its own default model");
    }

    /// A settings entry with a built-in's id replaces it wholesale, exactly
    /// as the binding merges (§DA-004-roster-is-asked-not-configured): the
    /// person's pi carries the modes theirs declares, not the built-in's none.
    #[test]
    fn a_persons_agent_entry_replaces_the_built_in_wholesale() {
        let hands = enumerate(&registry(
            r#"{
                "agents": {
                    "pi": {
                        "command": ["/nowhere/pi", "--provider", "openai-codex"],
                        "modes": { "high": ["--thinking", "high"] }
                    }
                }
            }"#,
            "{}",
        ));
        let pi = hand(&hands, "pi");
        assert_eq!(pi.efforts, vec!["high"]);
        // And availability is judged against the replacing command.
        assert_eq!(pi.available.as_deref(), Some("/nowhere/pi is not on disk"));
    }

    /// Model entries merge field-wise, project over global, carriers united —
    /// the binding's own semantics, mirrored so the two rosters cannot
    /// disagree (§DA-004-roster-is-asked-not-configured).
    #[test]
    fn a_project_overlay_retunes_a_model_without_redeclaring_it() {
        let hands = enumerate(&registry(
            r#"{
                "models": {
                    "impl-fast": {
                        "provider": "anthropic", "model": "claude-sonnet-4-6",
                        "default_agent": "claude-code"
                    }
                }
            }"#,
            r#"{ "models": { "impl-fast": { "model": "claude-opus-4-7" } } }"#,
        ));
        let fast = hand(&hands, "impl-fast");
        assert_eq!(fast.model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(fast.provider.as_deref(), Some("anthropic"));
        assert_eq!(fast.agent.as_deref(), Some("claude-code"));
    }

    /// A model with no carrier of its own rides `defaults.agent`; one with no
    /// carrier anywhere is shown with the reason the binding would refuse it
    /// with, never hidden (§FS-005-dispatch.14) — and the pairings it does
    /// declare still stand on their own ids.
    #[test]
    fn a_model_nobody_carries_is_shown_with_its_reason() {
        let hands = enumerate(&registry(
            r#"{
                "defaults": { "agent": "codex" },
                "models": { "drafting": { "provider": "openai", "model": "o3" } }
            }"#,
            "{}",
        ));
        assert_eq!(hand(&hands, "drafting").agent.as_deref(), Some("codex"));

        let hands = enumerate(&registry(
            r#"{
                "models": {
                    "drafting": {
                        "provider": "openai", "model": "o3",
                        "agents": { "codex": {} }
                    }
                }
            }"#,
            "{}",
        ));
        let orphan = hand(&hands, "drafting");
        assert_eq!(orphan.agent, None);
        assert!(
            orphan
                .available
                .as_deref()
                .unwrap()
                .contains("names no agent"),
            "{orphan:?}"
        );
        // The declared pairing is its own hand, and it works.
        assert_eq!(
            hand(&hands, "drafting@codex").agent.as_deref(),
            Some("codex")
        );
    }

    /// A model naming a carrier nobody declared is unavailable with that
    /// reason — computed at the read, not discovered at the spawn
    /// (§AR-002-summons.4).
    #[test]
    fn a_carrier_nobody_declared_is_a_computed_reason() {
        let hands = enumerate(&registry(
            r#"{
                "models": {
                    "drafting": {
                        "provider": "acme", "model": "m1",
                        "default_agent": "no-such-agent"
                    }
                }
            }"#,
            "{}",
        ));
        assert_eq!(
            hand(&hands, "drafting").available.as_deref(),
            Some("names agent 'no-such-agent', which is not declared")
        );
    }

    /// The binding's model and agent registries are separate namespaces, and
    /// its own examples name model profiles after agents — the profile holds
    /// the plain id, the agent stands alone under `@<agent>`, and every id on
    /// the roster is unique, because two rows under one name would make one
    /// of them unaddressable (§FS-005-dispatch.14).
    #[test]
    fn a_model_profile_that_claims_an_agents_name_leaves_both_addressable() {
        let hands = enumerate(&registry(
            r#"{
                "models": {
                    "codex": {
                        "provider": "openai", "model": "mock-codex",
                        "default_agent": "claude-code"
                    }
                }
            }"#,
            "{}",
        ));
        let profile = hand(&hands, "codex");
        assert_eq!(profile.model.as_deref(), Some("mock-codex"));
        assert_eq!(profile.agent.as_deref(), Some("claude-code"));
        let alone = hand(&hands, "@codex");
        assert_eq!(alone.agent.as_deref(), Some("codex"));
        assert_eq!(alone.model, None);
        let mut ids: Vec<&str> = ids(&hands);
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate hand ids");
    }

    /// The carrier of a model resolves the way the binding resolves it:
    /// `defaults.agent`, then the older top-level `agent`, and only then the
    /// profile's own `default_agent` (§DA-004-roster-is-asked-not-configured).
    #[test]
    fn the_settings_level_carrier_outranks_the_profiles_own() {
        let profile =
            r#""m1": { "provider": "acme", "model": "x", "default_agent": "claude-code" }"#;
        let hands = enumerate(&registry(
            &format!(r#"{{ "defaults": {{ "agent": "codex" }}, "models": {{ {profile} }} }}"#),
            "{}",
        ));
        assert_eq!(hand(&hands, "m1").agent.as_deref(), Some("codex"));

        // The older top-level spelling still binds, beneath `defaults.agent`.
        let hands = enumerate(&registry(
            &format!(r#"{{ "agent": "gemini", "models": {{ {profile} }} }}"#),
            "{}",
        ));
        assert_eq!(hand(&hands, "m1").agent.as_deref(), Some("gemini"));
        let hands = enumerate(&registry(
            &format!(
                r#"{{ "defaults": {{ "agent": "codex" }}, "agent": "gemini", "models": {{ {profile} }} }}"#
            ),
            "{}",
        ));
        assert_eq!(hand(&hands, "m1").agent.as_deref(), Some("codex"));
    }

    /// The overlay merges by which fields the file wrote, not by which are
    /// non-null — an explicit `null` clears an inherited value, exactly as
    /// the binding merges (§DA-004-roster-is-asked-not-configured).
    #[test]
    fn an_explicit_null_in_the_overlay_clears_what_it_names() {
        let global = r#"{
            "defaults": { "agent": "codex" },
            "models": {
                "impl-fast": {
                    "provider": "anthropic", "model": "claude-sonnet-4-6",
                    "default_agent": "claude-code"
                }
            }
        }"#;
        let merged = registry(
            global,
            r#"{ "models": { "impl-fast": { "provider": null } } }"#,
        );
        let fast = merged.models.get("impl-fast").unwrap();
        assert_eq!(fast.provider, None);
        assert_eq!(fast.model.as_deref(), Some("claude-sonnet-4-6"));

        // And a field the overlay never wrote stays inherited.
        assert_eq!(
            registry(global, r#"{ "defaults": { "agent": null } }"#).default_agent,
            None
        );
        assert_eq!(
            registry(global, r#"{ "defaults": {} }"#)
                .default_agent
                .as_deref(),
            Some("codex")
        );
    }

    /// Efforts keep the order the settings declare them in — the binding
    /// reads its modes as written, and "the first declared mode" is an answer
    /// its own resolution gives (§DA-004-roster-is-asked-not-configured).
    #[test]
    fn efforts_keep_the_declared_order() {
        let hands = enumerate(&registry(
            r#"{
                "agents": {
                    "ours": {
                        "command": ["sh"],
                        "modes": { "zeta": null, "alpha": null, "mid": null }
                    }
                }
            }"#,
            "{}",
        ));
        assert_eq!(hand(&hands, "ours").efforts, vec!["zeta", "alpha", "mid"]);
    }

    /// A settings file that exists and does not parse is a refusal naming the
    /// file (§FS-005-dispatch.14): a roster read around it would be a list
    /// missing whatever the person just added (§AR-002-summons.4).
    #[test]
    fn a_settings_file_that_does_not_parse_refuses_with_the_file_named() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let why = read_settings(&path).unwrap_err();
        assert!(why.contains("does not parse"), "{why}");
        assert!(why.contains("settings.json"), "{why}");
    }

    /// Availability is the agent's command looked for, never spawned: a bare
    /// word on PATH holds, a bare word off it says so, an absolute path is
    /// asked of the disk (§AR-002-summons.4).
    #[test]
    fn availability_is_looked_for_not_spawned() {
        let held = AgentProfile {
            command: vec!["sh".to_string()],
            modes: Modes::default(),
        };
        assert_eq!(held.available(), None);
        let missing = AgentProfile {
            command: vec!["no-such-agent-anywhere".to_string()],
            modes: Modes::default(),
        };
        assert_eq!(
            missing.available().unwrap(),
            "no-such-agent-anywhere is not on PATH"
        );
        let empty = AgentProfile::default();
        assert_eq!(empty.available().unwrap(), "declares no command to run");
    }

    /// The binding's grammar is rendered here and nowhere else
    /// (§FS-005-dispatch.14): the selector with and without a provider and an
    /// effort, and None for a hand the grammar cannot carry — an agent on its
    /// own default model is handed over as flags, not as a selector.
    #[test]
    fn a_hand_is_rendered_into_the_bindings_grammar_only_here() {
        let full = Hand {
            id: "impl-fast".to_string(),
            agent: Some("claude-code".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            provider: Some("anthropic".to_string()),
            efforts: vec!["yolo".to_string()],
            available: None,
        };
        assert_eq!(
            full.target(Some("yolo")).unwrap(),
            "claude-code[yolo]:anthropic:claude-sonnet-4-6"
        );
        assert_eq!(
            full.target(None).unwrap(),
            "claude-code:anthropic:claude-sonnet-4-6"
        );
        let bare = Hand {
            provider: None,
            ..full.clone()
        };
        assert_eq!(bare.target(None).unwrap(), "claude-code:claude-sonnet-4-6");
        let alone = Hand {
            model: None,
            provider: None,
            ..full
        };
        assert_eq!(alone.target(Some("yolo")), None);
    }

    // ---------------------------------------------------------------------
    // Choosing one of them (§FS-006-project-interface.9)
    // ---------------------------------------------------------------------

    /// Agents whose commands are certainly there, so which hand is available
    /// is this fixture's fact rather than the machine's.
    const SETTINGS: &str = r#"{
        "agents": {
            "claude-code": { "command": ["sh"], "modes": { "high": null, "yolo": null } },
            "codex": { "command": ["sh"], "modes": { "high": null } }
        },
        "models": {
            "gpt-5":  { "provider": "openai", "model": "m5", "default_agent": "codex" },
            "luna":   { "provider": "acme", "model": "m-luna", "default_agent": "claude-code" },
            "pinned": { "provider": "anthropic", "model": "m-pin", "default_agent": "claude-code" },
            "review-deep": { "provider": "openai", "model": "m-deep", "default_agent": "codex" },
            "sonnet": { "provider": "anthropic", "model": "m-son", "default_agent": "claude-code" }
        }
    }"#;

    fn full_roster() -> Roster {
        Roster {
            hands: enumerate(&registry(SETTINGS, "{}")),
            refusal: None,
            notes: Vec::new(),
        }
    }

    /// The seven steps with nothing reported about any pool, which is the
    /// ordinary case and the one every case below is about: evidence that says
    /// nothing vetoes nothing, so what these pin down is the order alone.
    fn resolve_(
        roster: &Roster,
        site: &crate::work::recipe::WorkConfig,
        project: Option<&crate::work::recipe::ProjectWorkConfig>,
        action: &str,
        picked: Option<&HandList>,
        pinned: Option<&HandList>,
    ) -> Choice {
        resolve(
            roster,
            site,
            project,
            action,
            picked,
            pinned,
            &Evidence::default(),
        )
    }

    fn pin(text: &str) -> HandList {
        HandList::parse(text).unwrap()
    }

    fn table(entries: &[(&str, &str)]) -> BTreeMap<String, HandList> {
        entries
            .iter()
            .map(|(action, hand)| ((*action).to_string(), pin(hand)))
            .collect()
    }

    fn site_of(entries: &[(&str, &str)]) -> crate::work::recipe::WorkConfig {
        crate::work::recipe::WorkConfig {
            hands: table(entries),
            ..crate::work::recipe::WorkConfig::default()
        }
    }

    fn project_of(
        entries: &[(&str, &str)],
        permitted: &[&str],
    ) -> crate::work::recipe::ProjectWorkConfig {
        crate::work::recipe::ProjectWorkConfig {
            hands: table(entries),
            permitted_hands: permitted.iter().map(|id| (*id).to_string()).collect(),
            ..crate::work::recipe::ProjectWorkConfig::default()
        }
    }

    fn chose(choice: &Choice) -> (&str, &str, Option<&str>) {
        match choice {
            Choice::Chosen {
                hand,
                effort,
                whence,
                ..
            } => (hand.id.as_str(), whence.as_str(), effort.as_deref()),
            other => panic!("nobody was chosen: {other:?}"),
        }
    }

    /// The seven steps, each displacing the one below it
    /// (§FS-005-dispatch.14, §FS-006-project-interface.9). Peeled one at a
    /// time off the same site and project, so what is being tested is the
    /// order rather than seven independent lookups.
    #[test]
    fn each_step_displaces_the_one_below_it() {
        let roster = full_roster();
        let site = site_of(&[("default", "sonnet:yolo"), ("rebase", "gpt-5")]);
        let project = project_of(&[("default", "review-deep"), ("rebase", "luna:high")], &[]);
        let pinned = pin("pinned:high");
        let picked = pin("gpt-5:high");

        // 1. What the reader picked for this dispatch alone.
        let choice = resolve_(
            &roster,
            &site,
            Some(&project),
            "rebase",
            Some(&picked),
            Some(&pinned),
        );
        assert_eq!(
            chose(&choice),
            ("gpt-5", "what you picked for this dispatch", Some("high"))
        );

        // 2. The pin the action or recipe itself carries.
        let choice = resolve_(
            &roster,
            &site,
            Some(&project),
            "rebase",
            None,
            Some(&pinned),
        );
        assert_eq!(
            chose(&choice),
            ("pinned", "the hand pinned on 'rebase'", Some("high"))
        );

        // 3. The project's hand for this action id.
        let choice = resolve_(&roster, &site, Some(&project), "rebase", None, None);
        assert_eq!(
            chose(&choice),
            ("luna", "this project's hand for 'rebase'", Some("high"))
        );
        // And it is the binding's own selector that gets written down —
        // rendered by the roster and nowhere else (§FS-005-dispatch.14).
        assert_eq!(
            choice.pin(),
            (Some("claude-code[high]:acme:m-luna".to_string()), None)
        );

        // 4. The project's default, for an id its table does not name. The
        // pin names no effort, and 'review-deep' declares exactly one — the
        // choice is completed to it, and the note says so.
        let choice = resolve_(&roster, &site, Some(&project), "fix-gate", None, None);
        assert_eq!(
            chose(&choice),
            ("review-deep", "this project's default hand", Some("high"))
        );
        assert!(
            choice.note().unwrap().contains("the one it declares"),
            "{choice:?}"
        );

        // 5–6. The site's table, read the same way: this id before its
        // default.
        let bare = project_of(&[], &[]);
        let choice = resolve_(&roster, &site, Some(&bare), "rebase", None, None);
        assert_eq!(
            chose(&choice),
            ("gpt-5", "the site's hand for 'rebase'", Some("high"))
        );
        let choice = resolve_(&roster, &site, Some(&bare), "fix-gate", None, None);
        assert_eq!(
            chose(&choice),
            ("sonnet", "the site's default hand", Some("yolo"))
        );

        // 7. Nobody chose at all: the binding picks unasked, and nothing is
        // pinned onto the ticket.
        let choice = resolve_(&roster, &site_of(&[]), None, "fix-gate", None, None);
        assert_eq!(choice, Choice::Unasked { note: None });
        assert_eq!(choice.pin(), (None, None));
    }

    /// A hand outside the project's narrowing is refused with that reason,
    /// wherever it was named — never dropped, never quietly replaced
    /// (§FS-006-project-interface.9). This is what a repository under a policy
    /// about which models may see its code is asking for.
    #[test]
    fn a_hand_the_project_does_not_permit_is_refused_with_that_reason() {
        let roster = full_roster();
        let site = site_of(&[("default", "gpt-5")]);
        let project = project_of(&[("rebase", "luna:high")], &["luna", "sonnet"]);

        // The site's default, refused on a project that permits two others.
        let why = match resolve_(&roster, &site, Some(&project), "fix-gate", None, None) {
            Choice::Refused(why) => why,
            other => panic!("the narrowing let it through: {other:?}"),
        };
        assert!(why.contains("names 'gpt-5'"), "{why}");
        assert!(why.contains("does not permit"), "{why}");
        assert!(why.contains("luna, sonnet"), "{why}");

        // What it does permit still runs.
        assert_eq!(
            chose(&resolve_(
                &roster,
                &site,
                Some(&project),
                "rebase",
                None,
                None
            ))
            .0,
            "luna"
        );

        // The reader's own choice is bound by it too, and so is a pin on the
        // action — a narrowing nothing above it obeys is not a policy.
        let outside = pin("gpt-5");
        for (picked, pinned) in [(Some(&outside), None), (None, Some(&outside))] {
            assert!(
                matches!(
                    resolve_(&roster, &site, Some(&project), "rebase", picked, pinned),
                    Choice::Refused(_)
                ),
                "{picked:?} {pinned:?}"
            );
        }

        // And a hand spelled out in full is outside every list by
        // construction: nothing in it authorized that pair.
        let spelled = HandPin::Spelled {
            agent: "claude-code".to_string(),
            model: "m-luna".to_string(),
            effort: None,
        };
        assert!(matches!(
            resolve_(
                &roster,
                &site,
                Some(&project),
                "rebase",
                None,
                Some(&HandList::one(spelled.clone()))
            ),
            Choice::Refused(_)
        ));

        // What a narrowing cannot bind is what nobody chose: the binding's
        // own unasked pick is not something ephor was told, and the silence
        // is said out loud rather than mistaken for the policy holding.
        let choice = resolve_(
            &roster,
            &site_of(&[]),
            Some(&project),
            "fix-gate",
            None,
            None,
        );
        let note = choice.note().expect("the unbindable case is said");
        assert!(note.contains("permits only luna, sonnet"), "{note}");
        assert!(note.contains("picks unasked"), "{note}");
    }

    /// A pair the registry never enumerated — a proxy serving a model it does
    /// not list — is accepted with a note rather than refused: ephor cannot
    /// prove it invalid (§FS-006-project-interface.9).
    #[test]
    fn a_pair_the_registry_never_listed_is_passed_through_with_a_note() {
        let roster = full_roster();
        let spelled = HandPin::Spelled {
            agent: "claude-code".to_string(),
            model: "some-proxy-model".to_string(),
            effort: Some("yolo".to_string()),
        };
        let site = crate::work::recipe::WorkConfig {
            hands: BTreeMap::from([("default".to_string(), HandList::one(spelled))]),
            ..crate::work::recipe::WorkConfig::default()
        };
        let choice = resolve_(&roster, &site, None, "fix-gate", None, None);
        assert_eq!(
            choice.pin(),
            (Some("claude-code[yolo]:some-proxy-model".to_string()), None)
        );
        let note = choice.note().expect("an unprovable pair is noted");
        assert!(note.contains("does not list"), "{note}");

        // A pair the registry does list is that hand, with everything the
        // roster knows about it — including its one declared effort, which an
        // effort-less spelling is completed to like any named hand's.
        let known = HandPin::Spelled {
            agent: "codex".to_string(),
            model: "m5".to_string(),
            effort: Some("high".to_string()),
        };
        let site = crate::work::recipe::WorkConfig {
            hands: BTreeMap::from([("default".to_string(), HandList::one(known))]),
            ..crate::work::recipe::WorkConfig::default()
        };
        let choice = resolve_(&roster, &site, None, "fix-gate", None, None);
        assert_eq!(
            chose(&choice),
            ("gpt-5", "the site's default hand", Some("high"))
        );
        assert_eq!(choice.note(), None);

        let effortless = HandPin::Spelled {
            agent: "codex".to_string(),
            model: "m5".to_string(),
            effort: None,
        };
        let site = crate::work::recipe::WorkConfig {
            hands: BTreeMap::from([("default".to_string(), HandList::one(effortless))]),
            ..crate::work::recipe::WorkConfig::default()
        };
        let choice = resolve_(&roster, &site, None, "fix-gate", None, None);
        assert_eq!(chose(&choice).2, Some("high"));
        assert!(
            choice.note().unwrap().contains("the one it declares"),
            "{choice:?}"
        );
    }

    /// A name nothing can resolve is refused rather than quietly falling
    /// through to the next step: a typo in who does the work is exactly the
    /// thing the person configured (§FS-006-project-interface.9).
    #[test]
    fn a_name_the_roster_does_not_have_is_refused_with_what_it_does() {
        let roster = full_roster();
        let site = site_of(&[("default", "lnua")]);
        let why = match resolve_(&roster, &site, None, "fix-gate", None, None) {
            Choice::Refused(why) => why,
            other => panic!("a typo went through: {other:?}"),
        };
        assert!(why.contains("names 'lnua'"), "{why}");
        assert!(why.contains("the roster has:"), "{why}");
        assert!(why.contains("luna"), "{why}");
        assert!(
            why.contains("a model profile named 'lnua' with an agent carrier"),
            "{why}"
        );
        assert!(why.contains("Rhei settings `models` registry"), "{why}");
        assert!(why.contains("nameable model-carrying hand"), "{why}");

        // An effort is checked against what the hand declares, before
        // anything is written — the binding refuses it at the spawn.
        let site = site_of(&[("default", "gpt-5:yolo")]);
        let why = match resolve_(&roster, &site, None, "fix-gate", None, None) {
            Choice::Refused(why) => why,
            other => panic!("an undeclared effort went through: {other:?}"),
        };
        assert!(
            why.contains("it does not declare (it declares: high)"),
            "{why}"
        );

        // A hand that declares none is asked plainly, and asking it for an
        // effort says that rather than listing an empty set.
        let plain = Roster {
            hands: vec![Hand {
                id: "pi".to_string(),
                agent: Some("pi".to_string()),
                model: Some("m".to_string()),
                provider: None,
                efforts: Vec::new(),
                available: None,
            }],
            refusal: None,
            notes: Vec::new(),
        };
        let why = match resolve_(
            &plain,
            &site_of(&[("default", "pi:high")]),
            None,
            "x",
            None,
            None,
        ) {
            Choice::Refused(why) => why,
            other => panic!("{other:?}"),
        };
        assert!(why.contains("declares none"), "{why}");
    }

    /// A choice naming no effort is settled by what the hand declares
    /// (§FS-005-dispatch.14): none declared is asked plainly, exactly one is
    /// completed to it with a note, several are refused with the list — the
    /// binding's two spellings disagree about an effort-less ask, one
    /// dropping the effort silently and the other refusing the run outright,
    /// and neither answer is the reader's choice.
    #[test]
    fn an_effortless_choice_is_settled_by_what_the_hand_declares() {
        let roster = full_roster();

        // Several declared: refused, before anything is written.
        let why = match resolve_(
            &roster,
            &site_of(&[("default", "sonnet")]),
            None,
            "x",
            None,
            None,
        ) {
            Choice::Refused(why) => why,
            other => panic!("an ambiguous effort went through: {other:?}"),
        };
        assert!(why.contains("declares several (high, yolo)"), "{why}");
        assert!(why.contains("'sonnet:<effort>'"), "{why}");

        // Exactly one declared: completed to it, and the pin carries it — an
        // effort-less selector would run without any of the hand's efforts.
        let choice = resolve_(
            &roster,
            &site_of(&[("default", "review-deep")]),
            None,
            "x",
            None,
            None,
        );
        assert_eq!(
            choice.pin(),
            (Some("codex[high]:openai:m-deep".to_string()), None)
        );
        assert!(
            choice
                .note()
                .unwrap()
                .contains("'high' is the one it declares"),
            "{choice:?}"
        );

        // None declared: asked plainly, in both spellings — there is nothing
        // for the ask to drop. An agent-only such hand rides as the agent
        // flag alone, and both facts about it are one note.
        let plain = Roster {
            hands: vec![Hand {
                id: "pi".to_string(),
                agent: Some("pi".to_string()),
                model: None,
                provider: None,
                efforts: Vec::new(),
                available: None,
            }],
            refusal: None,
            notes: Vec::new(),
        };
        let choice = resolve_(
            &plain,
            &site_of(&[("default", "pi")]),
            None,
            "x",
            None,
            None,
        );
        assert_eq!(chose(&choice), ("pi", "the site's default hand", None));
        assert_eq!(
            choice.flags(),
            Some(HandFlags {
                agent: "pi".to_string(),
                effort: None,
            })
        );
    }

    /// An unavailable hand is chosen with its reason rather than refused: a
    /// ticket is written before it is run, and who it is for is worth
    /// recording either way (§FS-005-dispatch.14). A hand naming an agent and
    /// no model of its own pins nothing, and says that too — the plan
    /// language has no line for it, so the choice rides the run as flags.
    #[test]
    fn a_hand_that_cannot_be_asked_is_still_named_with_its_reason() {
        let roster = Roster {
            hands: vec![
                Hand {
                    id: "away".to_string(),
                    agent: Some("nowhere".to_string()),
                    model: Some("m".to_string()),
                    provider: None,
                    efforts: Vec::new(),
                    available: Some("nowhere is not on PATH".to_string()),
                },
                Hand {
                    id: "plain-agent".to_string(),
                    agent: Some("sh".to_string()),
                    model: None,
                    provider: None,
                    efforts: Vec::new(),
                    available: None,
                },
            ],
            refusal: None,
            notes: Vec::new(),
        };
        let choice = resolve_(
            &roster,
            &site_of(&[("default", "away")]),
            None,
            "x",
            None,
            None,
        );
        assert_eq!(chose(&choice).0, "away");
        assert!(
            choice.note().unwrap().contains("nowhere is not on PATH"),
            "{choice:?}"
        );

        let choice = resolve_(
            &roster,
            &site_of(&[("default", "plain-agent")]),
            None,
            "x",
            None,
            None,
        );
        assert_eq!(choice.pin(), (None, None));
        assert!(
            choice.note().unwrap().contains("names no model of its own"),
            "{choice:?}"
        );
    }

    /// A choice binds in one of two spellings, never both
    /// (§FS-005-dispatch.14): a hand carrying a model is pinned on the ticket
    /// and yields no run flags — the ticket's line wins — while a hand naming
    /// an agent alone pins nothing and rides the run as flags, the effort
    /// alongside. Nobody chosen is nothing to spell either way.
    #[test]
    fn a_choice_is_spelled_on_the_ticket_or_as_run_flags_never_both() {
        let roster = full_roster();

        let carried = resolve_(
            &roster,
            &site_of(&[("default", "luna:high")]),
            None,
            "x",
            None,
            None,
        );
        assert_eq!(
            carried.pin(),
            (Some("claude-code[high]:acme:m-luna".to_string()), None)
        );
        assert_eq!(carried.flags(), None);

        let alone = resolve_(
            &roster,
            &site_of(&[("default", "claude-code:high")]),
            None,
            "x",
            None,
            None,
        );
        assert_eq!(alone.pin(), (None, None));
        assert_eq!(
            alone.flags(),
            Some(HandFlags {
                agent: "claude-code".to_string(),
                effort: Some("high".to_string()),
            })
        );

        let unasked = resolve_(&roster, &site_of(&[]), None, "x", None, None);
        assert_eq!(unasked.pin(), (None, None));
        assert_eq!(unasked.flags(), None);
    }

    /// The picker's roster is the narrowing applied, not announced
    /// (§FS-005-dispatch.14): a hand the project does not permit does not
    /// appear at all, while an unavailable one stays with its reason — its
    /// time is wrong, not its name. No narrowing offers everything, and an
    /// empty roster offers nothing.
    #[test]
    fn the_picker_is_offered_only_what_the_project_permits() {
        let mut roster = full_roster();
        roster.hands.push(Hand {
            id: "away".to_string(),
            agent: Some("nowhere".to_string()),
            model: None,
            provider: None,
            efforts: Vec::new(),
            available: Some("nowhere is not on PATH".to_string()),
        });

        let everyone = pickable(&roster, None);
        assert_eq!(everyone.len(), roster.hands.len());
        assert!(
            everyone.iter().any(|hand| hand.available.is_some()),
            "an unavailable hand is shown, never hidden"
        );

        let narrowed = pickable(&roster, Some(&project_of(&[], &["luna", "away"])));
        let ids: Vec<&str> = narrowed.iter().map(|hand| hand.id.as_str()).collect();
        assert_eq!(ids, ["luna", "away"]);

        let empty = Roster {
            hands: Vec::new(),
            refusal: Some("nobody".to_string()),
            notes: Vec::new(),
        };
        assert!(pickable(&empty, None).is_empty());
    }

    /// With nobody to ask, a configured hand resolves to nothing and says so
    /// in the workable rung's own words rather than failing the dispatch
    /// (§FS-006-project-interface.9): the ticket is written as it would have
    /// been, because who does the work is not what makes one.
    #[test]
    fn with_no_runtime_a_configured_hand_resolves_to_nothing_and_says_so() {
        let absent = crate::work::recipe::WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            hands: table(&[("default", "luna:high")]),
            ..crate::work::recipe::WorkConfig::default()
        };
        let roster = roster(&absent, None);
        let choice = resolve_(&roster, &absent, None, "fix-gate", None, None);
        assert_eq!(choice.pin(), (None, None));
        let note = choice.note().expect("the silence is said");
        assert!(note.contains("'luna' at effort 'high'"), "{note}");
        // The rung's own sentence, carried rather than paraphrased.
        assert!(note.contains(&roster.refusal.clone().unwrap()), "{note}");
    }

    /// With no runtime there is nobody to ask, and the roster says so in the
    /// workable rung's own words (§FS-005-dispatch.14) — the same sentence
    /// running refuses with, because two sentences would drift.
    #[test]
    fn no_runtime_is_an_empty_roster_in_the_workable_rungs_words() {
        let absent = crate::work::recipe::WorkConfig {
            runner: Some("no-such-runtime-anywhere".to_string()),
            ..crate::work::recipe::WorkConfig::default()
        };
        let roster = roster(&absent, None);
        assert!(roster.hands.is_empty());
        let why = roster.refusal.unwrap();
        assert!(
            why.starts_with("no-such-runtime-anywhere is not on PATH"),
            "{why}"
        );
        assert_eq!(why, super::super::refusal(&absent).unwrap());
    }
}
