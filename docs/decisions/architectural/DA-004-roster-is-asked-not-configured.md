# DA-004-roster-is-asked-not-configured: the roster is asked of the binding, never kept by ephor

**Status:** Accepted
**Date:** 2026-08-14

ephor needs to know which agent/model combinations exist, so a reader can
choose one and a project can default one per action (§FS-005-dispatch.14).
Two places could hold that knowledge: ephor's own configuration — a list of
agents and models in `status.json` — or the binding's, read at the moment of
asking. This record fixes the second and names what the first would have
cost.

## 1. The decision

The roster is enumerated inside the runtime module (§AR-007-runtime.1), from
the binding's own merged settings: its built-in agent profiles, the person's
global settings file, and a work root's project overlay. Each named model
profile is paired with the agent it declares as its carrier and with each
agent it binds launch arguments for, and each agent standing alone — running
with its own default model — is a hand too, so the enumeration is the
binding's registry read out, never a cross-product ephor invented. The
registry's shape is already the right one: a model profile is a named choice
that knows its carrier, which is exactly what a free `agent × model` grid
cannot be, since models and modes are both per-agent and most of the grid
would be combinations nobody can prove valid.

Availability is computed where the roster is read — the agent's command is
looked for on `PATH` or on disk, never spawned to fail (§AR-002-summons.4)
— so an unavailable hand carries its one-sentence reason from the start.
What leaves the module is a list of hands: opaque ids with the facts a
reader needs to choose. The binding's own grammar — the settings files it
merges, and the `agent[mode]:provider:model` selector its plans carry — is
parsed and rendered inside the module and nowhere else
(§REQ-001-boundary.5), so ephor's configuration holds an id a second binding
could read, not the first binding's syntax.

## 2. The rejected alternative

A `hands` list in ephor's site configuration: each entry naming an agent, a
model, and the modes a person believes it has. It fails the way every copy
of somebody else's registry fails — it drifts the first time an agent or a
model is added on the other side, and it drifts silently: the runtime would
run hands ephor's list has never heard of, ephor would offer hands the
runtime no longer knows, and the reader holds two rosters and therefore
none. It also asks ephor to validate what it cannot: whether a model runs
under an agent is the binding's knowledge (a proxy-backed agent carries
models no static list anticipates), so ephor's copy would be either
permissive to the point of uselessness or wrong.

## 3. The cost

The shipped binding has no command that prints its merged roster — a known
gap in the runtime, not a preference of this design. Until it exists, the
module reads the binding's settings files itself, which makes their
locations and their merge order — built-ins, then global, then project;
agent entries replacing wholesale, model entries merging field-wise — part
of the coupling surface this module maintains, a second file-level contract
beside the plan language (§DA-001-runtime-bound-default.3). The built-in
agent profiles are likewise spelled once inside the module, so a machine
whose settings add nothing still has a roster; they are a copy of the
binding's own seed registry and carry the same drift risk in miniature,
accepted because they are few, stable, and confined to the one module that
already owes the binding compatibility. When the binding grows a roster
command, the module swaps the file read for the ask and nothing above it
moves.
