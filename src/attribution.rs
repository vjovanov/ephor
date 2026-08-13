//! One matching engine, evidence against identity (§AR-003-attribution).
//!
//! Placement is pure matching: it takes the **evidence** a conversation
//! carries and the **identity** a registry row declares, and returns a
//! placement or the reason there is none (§FS-008-attribution). It runs in one
//! place and nowhere else — no source places its own items — so a
//! misplacement is debugged by looking at the evidence rather than by
//! rereading a provider.
//!
//! There is no IO here and nothing vendor-shaped: a function of
//! (evidence, identity table).

use crate::matter::SubjectKey;

/// What a conversation carries about where it belongs (§AR-003-attribution.1).
/// Extracted once at fetch normalization and kept on the matter, so it can be
/// looked at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    /// The venue's own subject key, where the source stated one — the pull
    /// request a thread is on, the store a ticket lives in.
    pub venue: Option<SubjectKey>,
    /// The repository the venue belongs to, as the forge names it.
    pub repo: Option<String>,
    /// Ticket keys found in the text.
    pub tickets: Vec<String>,
    /// Repositories named in the text or the url.
    pub repos: Vec<String>,
    /// Addresses and participants.
    pub addresses: Vec<String>,
    /// The plain words that may hit an alias.
    pub words: String,
}

/// A project's identity: the signals by which its matters are recognized
/// (§FS-008-attribution.1). Compiled from the registry row, with a manifest's
/// hints adopted where the row does not override — the row has the last word,
/// because attribution keys must not be forgeable by a checkout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub project: String,
    /// Ticket prefixes this project's keys start with (`GR`, `ABC`).
    pub tickets: Vec<String>,
    /// The repositories of its forest.
    pub repos: Vec<String>,
    /// Repositories and organizations that are the project's business without
    /// being in its forest — what places the general case: a mention on some
    /// repository of the project's ecosystem, an issue filed there
    /// (§FS-008-attribution.1). An entry without a `/` is a whole
    /// organization.
    pub territory: Vec<String>,
    /// Names it answers to.
    pub aliases: Vec<String>,
    pub addresses: Vec<String>,
}

/// How firmly the evidence points at a project (§FS-008-attribution.3). An
/// explicit venue wins outright; a reference places on the named matter; only
/// resemblance may be argued with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    Resemblance,
    Reference,
    Venue,
}

/// Where the engine put it, or why it could not (§FS-008-attribution.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placed {
    /// One project claimed it more firmly than any other.
    On { project: String, how: Strength },
    /// Two projects claimed it with equal strength. Not resolved by order:
    /// a guess that lands wrong amends someone's matter silently.
    Ambiguous { candidates: Vec<String> },
    /// Nothing claimed it.
    Nothing,
}

impl Identity {
    /// How firmly this identity claims that evidence, or None where it does
    /// not claim it at all.
    pub fn claim(&self, evidence: &Evidence) -> Option<Strength> {
        // An explicit venue wins outright: the source said which repository
        // this is on, and a repository of the forest is not a resemblance.
        if let Some(repo) = &evidence.repo {
            if self.repos.iter().any(|own| own == repo) {
                return Some(Strength::Venue);
            }
            if self.claims_territory(repo) {
                return Some(Strength::Reference);
            }
        }
        // A reference places on the named matter: a ticket key this project
        // owns, or one of its repositories named in the text.
        if evidence
            .tickets
            .iter()
            .any(|ticket| self.owns_ticket(ticket))
        {
            return Some(Strength::Reference);
        }
        if evidence
            .repos
            .iter()
            .any(|repo| self.repos.contains(repo) || self.claims_territory(repo))
        {
            return Some(Strength::Reference);
        }
        if evidence
            .addresses
            .iter()
            .any(|address| self.addresses.contains(address))
        {
            return Some(Strength::Reference);
        }
        // Resemblance may only be argued with, never asserted.
        if self.resembles(&evidence.words) {
            return Some(Strength::Resemblance);
        }
        None
    }

    /// Whether a ticket key is this project's: its prefix is one the project
    /// declared.
    fn owns_ticket(&self, ticket: &str) -> bool {
        let prefix = ticket.split_once('-').map(|(prefix, _)| prefix);
        prefix.is_some_and(|prefix| self.tickets.iter().any(|own| own == prefix))
    }

    /// Whether a repository is in the project's territory — named outright, or
    /// under an organization the project claims whole.
    fn claims_territory(&self, repo: &str) -> bool {
        self.territory.iter().any(|claimed| {
            claimed == repo
                || (!claimed.contains('/')
                    && repo
                        .split_once('/')
                        .is_some_and(|(owner, _)| owner == claimed))
        })
    }

    /// Whether the words name the project. Whole words only: a project called
    /// `api` must not claim every sentence containing "rapid".
    fn resembles(&self, words: &str) -> bool {
        let lowered = words.to_lowercase();
        let names = std::iter::once(&self.project).chain(&self.aliases);
        names.filter(|name| !name.is_empty()).any(|name| {
            let name = name.to_lowercase();
            lowered
                .split(|character: char| !character.is_alphanumeric())
                .any(|word| word == name)
        })
    }
}

/// Place one piece of evidence against every project's identity
/// (§AR-003-attribution.3). Ambiguity — two projects claiming it with equal
/// strength — is not resolved by order: it goes to the unattributed bucket
/// carrying its candidates, because a guess that lands wrong amends someone
/// else's matter silently (§FS-008-attribution.4).
pub fn place(evidence: &Evidence, identities: &[Identity]) -> Placed {
    let mut claims: Vec<(&Identity, Strength)> = identities
        .iter()
        .filter_map(|identity| identity.claim(evidence).map(|how| (identity, how)))
        .collect();
    claims.sort_by(|left, right| right.1.cmp(&left.1));

    match claims.as_slice() {
        [] => Placed::Nothing,
        [(identity, how)] => Placed::On {
            project: identity.project.clone(),
            how: *how,
        },
        [(first, how), rest @ ..] => {
            let tied: Vec<String> = std::iter::once((*first).clone())
                .chain(
                    rest.iter()
                        .filter(|(_, other)| other == how)
                        .map(|(identity, _)| (*identity).clone()),
                )
                .map(|identity| identity.project)
                .collect();
            if tied.len() == 1 {
                return Placed::On {
                    project: first.project.clone(),
                    how: *how,
                };
            }
            Placed::Ambiguous { candidates: tied }
        }
    }
}

/// Which of a project's branches this evidence is work on — the same engine at
/// the second scope, with the project's branches as the identity table
/// (§AR-003-attribution.3). The code that matched ticket keys and branch names
/// is this function's seed, promoted rather than duplicated.
pub fn branch(evidence: &Evidence, branches: &[(String, Option<String>)]) -> Option<String> {
    // A ticket the branch's own key names is the firm answer.
    for (name, ticket) in branches {
        if let Some(ticket) = ticket {
            if evidence.tickets.iter().any(|found| found == ticket) {
                return Some(name.clone());
            }
        }
    }
    // Then the branch named outright in what was said.
    branches
        .iter()
        .find(|(name, _)| !name.is_empty() && evidence.words.contains(name.as_str()))
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget() -> Identity {
        Identity {
            project: "widget".to_string(),
            tickets: vec!["ABC".to_string()],
            repos: vec!["acme/widget".to_string()],
            territory: vec!["acme-labs".to_string(), "other/plugin".to_string()],
            aliases: vec!["the widget".to_string()],
            addresses: vec!["widget@acme.example".to_string()],
        }
    }

    fn gadget() -> Identity {
        Identity {
            project: "gadget".to_string(),
            tickets: vec!["XYZ".to_string()],
            repos: vec!["acme/gadget".to_string()],
            ..Identity::default()
        }
    }

    fn words(text: &str) -> Evidence {
        Evidence {
            words: text.to_string(),
            ..Evidence::default()
        }
    }

    #[test]
    fn a_repository_of_the_forest_is_a_venue_and_wins_outright() {
        let evidence = Evidence {
            repo: Some("acme/widget".to_string()),
            ..Evidence::default()
        };
        assert_eq!(widget().claim(&evidence), Some(Strength::Venue));
        assert_eq!(
            place(&evidence, &[gadget(), widget()]),
            Placed::On {
                project: "widget".to_string(),
                how: Strength::Venue
            }
        );
    }

    /// Territory is what places the general case: a mention on some repository
    /// of the project's ecosystem, in no forest at all
    /// (§FS-008-attribution.1).
    #[test]
    fn a_repository_beyond_the_forest_is_claimed_where_the_territory_says_so() {
        let named = Evidence {
            repo: Some("other/plugin".to_string()),
            ..Evidence::default()
        };
        assert_eq!(widget().claim(&named), Some(Strength::Reference));

        // A whole organization is claimed by naming it without a repository.
        let anywhere = Evidence {
            repo: Some("acme-labs/anything".to_string()),
            ..Evidence::default()
        };
        assert_eq!(widget().claim(&anywhere), Some(Strength::Reference));

        // And one nobody claimed stays unplaced.
        let stranger = Evidence {
            repo: Some("someone/else".to_string()),
            ..Evidence::default()
        };
        assert_eq!(place(&stranger, &[widget(), gadget()]), Placed::Nothing);
    }

    #[test]
    fn a_ticket_key_places_by_the_prefix_its_project_owns() {
        let evidence = Evidence {
            tickets: vec!["ABC-42".to_string()],
            ..Evidence::default()
        };
        assert_eq!(widget().claim(&evidence), Some(Strength::Reference));
        assert_eq!(gadget().claim(&evidence), None);
    }

    #[test]
    fn resemblance_is_whole_words_and_never_a_substring() {
        assert_eq!(
            widget().claim(&words("the widget release is stuck")),
            Some(Strength::Resemblance)
        );
        // "widget" inside another word claims nothing.
        assert_eq!(widget().claim(&words("rewidgeting the frame")), None);
    }

    #[test]
    fn a_venue_outranks_a_reference_however_the_projects_are_ordered() {
        let evidence = Evidence {
            repo: Some("acme/gadget".to_string()),
            tickets: vec!["ABC-42".to_string()],
            ..Evidence::default()
        };
        // gadget has the venue, widget only the ticket reference.
        assert_eq!(
            place(&evidence, &[widget(), gadget()]),
            Placed::On {
                project: "gadget".to_string(),
                how: Strength::Venue
            }
        );
    }

    /// Two projects claiming the same evidence with equal strength is not
    /// resolved by order — it goes to the bucket, carrying both
    /// (§FS-008-attribution.4).
    #[test]
    fn equal_claims_go_to_the_bucket_rather_than_to_whoever_was_first() {
        let mut also_widget = gadget();
        also_widget.aliases = vec!["widget".to_string()];
        let evidence = words("the widget is stuck");
        assert_eq!(
            place(&evidence, &[widget(), also_widget]),
            Placed::Ambiguous {
                candidates: vec!["widget".to_string(), "gadget".to_string()]
            }
        );
    }

    #[test]
    fn an_address_the_project_declared_is_a_reference() {
        let evidence = Evidence {
            addresses: vec!["widget@acme.example".to_string()],
            ..Evidence::default()
        };
        assert_eq!(widget().claim(&evidence), Some(Strength::Reference));
    }

    #[test]
    fn the_same_engine_places_work_on_a_branch_inside_a_project() {
        let branches = vec![
            (
                "you/ABC-42-retry-window".to_string(),
                Some("ABC-42".to_string()),
            ),
            ("main".to_string(), None),
        ];
        let by_ticket = Evidence {
            tickets: vec!["ABC-42".to_string()],
            ..Evidence::default()
        };
        assert_eq!(
            branch(&by_ticket, &branches).as_deref(),
            Some("you/ABC-42-retry-window")
        );
        // Named outright in the words, where no ticket says it.
        let by_name = words("landed on you/ABC-42-retry-window yesterday");
        assert_eq!(
            branch(&by_name, &branches).as_deref(),
            Some("you/ABC-42-retry-window")
        );
        assert_eq!(branch(&words("nothing here"), &branches), None);
    }
}
