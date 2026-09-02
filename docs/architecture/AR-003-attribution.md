# AR-003-attribution: one matching engine, evidence against identity, at two scopes

The engine that realizes [§FS-008-attribution](../functional-spec/FS-008-attribution.md#fs-008-attribution-every-conversation-finds-its-project-or-says-that-it-could-not) is pure matching: it takes
**evidence** a conversation carries and **identity** the registry declares,
and returns a placement or the reason there is none. It runs inside the
pipeline's attribute stage ([§AR-008-pipeline](AR-008-pipeline.md#ar-008-pipeline-the-engine-is-seven-stages-legible-as-one-function)) and nowhere else — no source
places its own items, a split decided with its configuration cost in
[§DA-002-fetch-attribution-split](../decisions/architectural/DA-002-fetch-attribution-split.md#da-002-fetch-attribution-split-fetch-normalizes-attribution-places).

## 1. Evidence

Extracted once per discussion or event, at fetch normalization: the venue's
own subject key where the source stated one (the pull request the thread is
on, the store the ticket lives in); referenced keys found in text — ticket
patterns, pull request URLs, repository names; addresses and participants;
and the plain words that may hit an alias. Evidence is data on the item,
inspectable in `EPHOR_RAW`, so a misplacement can be debugged by looking.

## 2. Identity

Per project, compiled from the registry row with manifest hints adopted
where the row does not override ([§FS-008-attribution.1](../functional-spec/FS-008-attribution.md#1-identity-is-declared-and-the-row-has-the-last-word)) — identity lives in
the row per the three homes ([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)): ticket patterns,
the forest's repositories, the declared territory — repositories and
organizations that are the project's business without being in its forest,
which is what places a general mention or a stray issue — names and
aliases, addresses. Compiled identities form one table the engine matches
against — attribution is a function of (evidence, identity table), no IO.

## 3. Two scopes, one precedence

Stage one places a discussion or event on a matter; stage two places a
matter on a project and branch. Both apply [§FS-008-attribution.3](../functional-spec/FS-008-attribution.md#3-venue-beats-reference-beats-resemblance): an
explicit venue wins outright; a reference places on the named matter and
links onward; resemblance may only synthesize a topic matter. Ambiguity —
two projects claim the same evidence with equal strength — is not resolved
by order: the item goes to the unattributed bucket carrying its candidates,
because a guess that lands wrong amends someone's matter silently
([§FS-008-attribution.4](../functional-spec/FS-008-attribution.md#4-unattributed-is-a-place-not-a-fate)). Branch matching inside a project is the same
engine with the project's branches as the identity table — the code that
matches ticket keys and branch names today is this function's seed, promoted
rather than duplicated.
