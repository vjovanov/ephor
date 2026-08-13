# AR-002-summons: one executor runs everything ephor asks of the world

`Summons { verb, binding, place, dossier } → Answer { exit_code, answer, output }`
— the single operational primitive of §FS-006-project-interface.3. Configured
actions, offers, quick actions, custom-status, check verbs, gate verbs, the
checkout command, ticket-store CLIs, and the runtime's run are all instances.
One executor means one refusal path, one environment contract, and one answer
reader — and that the same operation invoked from a menu key and from a state
machine cannot drift apart (§FS-005-dispatch.12).

## 1. Resolving the place

The executor resolves where the command runs before it runs: the matter's
branch workspace where one resolves (the same resolution the tree's grouping
uses), the project's forest root otherwise, or the repository of the forest a
binding names (`cwd: repo:<name>`). A place that does not exist is a refusal
with the reason, or an offer to chain the checkout first
(§FS-004-quick-actions.7) — never a command run somewhere surprising.

## 2. The invocation

The command runs via `sh -c` in the resolved place with the dossier exported
as `EPHOR_*` — one vocabulary for every caller (§FS-005-dispatch.8). The
executor either hands over the terminal (interactive callers: menu actions,
the runtime) or captures output (verbs called during refresh and verify);
which, is a property of the call site, not of the binding. Exit semantics are
uniform: `0` done, non-zero failed, `75` parked. The whole crossing is
environment, exit code, and answer file — the seam's contract in materials
(§REQ-001-boundary.1), so the other side can always be a shell script.

## 3. The answer

Before spawning, the executor names a fresh file in `$EPHOR_ANSWER`; after
exit, it reads the file if the command wrote it, validates against the
envelope schema, normalizes the conveniences (`failures`, `features`) into
events and facts, and discards the file. No answer file is a complete answer
— the exit code stands alone (§FS-006-project-interface.4). Stdout is never
parsed for structure; custom-status's legacy stdout-JSON is honored by that
one binding, marked as such.

## 4. Refusal is computed, not discovered

The executor consults the capability table (§AR-005-capabilities) before
offering or running: a summons whose rung is missing is rendered
"(unavailable: …)" with the missing rung named, and running it is refused
with the same sentence. Discovery-by-failure — spawning to find out — is
reserved for the world's own errors, which are reported as the command's
output, not as ephor's.
