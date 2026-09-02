# DF-001-manifest-offered: the manifest is offered, never required

**Status:** Accepted
**Date:** 2026-08-13

A project that chooses to speak places one file, `ephor.json`, at its
forest root ([§FS-006-project-interface.2](../../functional-spec/FS-006-project-interface.md#2-the-manifest-is-offered-never-required)); a project that places nothing
is fully watchable exactly as it stands. This record fixes the manifest's
character: an offer, never a requirement — every field optional, an empty
manifest valid, and nothing in it able to gate a capability that probing
or site configuration could not establish alone.

## 1. Why offered

Requiring the file would break both laws at once: a required artifact in
the watched project is exactly what [§REQ-001-boundary.3](../../requirements/REQ-001-boundary.md#3-requirements-on-a-project-are-capabilities-never-artifacts) forbids — if a
file only makes sense because ephor exists, it may be offered, never
demanded — and a project that must change before it can be watched is no
longer costless to watch ([§GOAL-005-costless](../../goals.md#goal-005-costless-watching-costs-the-watched-nothing)). The manifest therefore only
ever *adds*: identity hints the row adopts unless it overrides, verb
bindings that outrank probes and yield to site configuration, offers a
person may invoke. Absence degrades to probing, and probing was already
enough.

## 2. Recipes are excluded

Offers are in the manifest; recipes are not. The line between them is who
spends: an offer is a menu entry a person invokes, costing nothing until
chosen, while a recipe ([§FS-005-dispatch.1](../../functional-spec/FS-005-dispatch.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)) spends the person's agent time
on its own match — and a repository does not get to spend the person's
agent time ([§FS-006-project-interface.2](../../functional-spec/FS-006-project-interface.md#2-the-manifest-is-offered-never-required)). What dispatches on its own is
site configuration only, written by the person who pays.
