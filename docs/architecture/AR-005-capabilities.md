# AR-005-capabilities: availability is computed once and consulted everywhere

The ladder of [§FS-006-project-interface.10](../../requirements.md#10-capability-rung-by-rung) is realized as a **capability
table**: one `CapabilitySet` per project, resolved during refresh and on
demand, recording for each rung whether it holds and — where it does not —
the one sentence that says why. Everything that offers, gates, or refuses
reads this table; nothing else runs its own `command_exists` or path check.

## 1. Resolution

Each rung names its establishment and is resolved accordingly
([§FS-006-project-interface.1](../../requirements.md#1-the-three-homes)): *placed* and *checkable* and *tasks* by
probing the checkout; *checkout-able*, *workable*, and parts of *gated* by
looking up bindings; *observable* and *branch-addressable* from the row and
the sources' last answers. Resolution is cheap by construction — stat
calls, config lookups, no spawning — so it can rerun whenever the world
may have moved (a refresh, a checkout that appeared).

## 2. Features declare needs

A feature — a menu entry, a recipe, a dispatch, a shipped CI step — carries
the list of rungs it needs. Offering is filtering on the table; refusing is
rendering the first missing rung's sentence. This gives [§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)'s
degrade rule one implementation: the sentence a person sees on a gated menu
entry, the reason `work dispatch` prints, and the log line a shipped step
emits are the same text from the same place.

## 3. The table is honest about time

A capability is what held when it was resolved. The executor re-checks the
one rung a summons is about to lean on (the place exists, the binding is
still on disk) at invocation, because a table that answers from ten minutes
ago and a directory deleted nine minutes ago must fail as the world, not as
a lie ([§FS-001-forge-interface.6](../../requirements.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not) has the same stance toward sources).
