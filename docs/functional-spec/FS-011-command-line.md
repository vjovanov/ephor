# FS-011-command-line: every ability is a command, and every answer has a JSON form

The interface is one way to reach the watch; the command line is the other,
and neither is a subset of the other ([§REQ-002-parity](../requirements/REQ-002-parity.md#req-002-parity-every-ability-is-reachable-without-the-screen-and-every-answer-has-a-machine-form)). What follows is the
command-line half of abilities that were reachable only by keystroke, and
the rule that governs the machine form of every answer.

## 1. What may be done here, listed and run

`ephor actions` prints the menu a matter carries: the source's own offers,
ephor's, the project's, the person's, the work that can be handed over, and
the workflows laid down beside it — one list, in provenance order, exactly
the list the interface shows ([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions),
[§FS-005-dispatch.1](FS-005-dispatch.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)). Each row says its id, what it is, whether it can run,
and where it cannot, the sentence that says why — the ladder's own, never a
second opinion ([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)). A
branch is asked the same question and answers with ephor's own offers about
it ([§FS-004-quick-actions.6](FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).

`ephor actions run <id>` runs one. It resolves the same working directory,
exports the same context as `EPHOR_*` ([§FS-004-quick-actions.1](FS-004-quick-actions.md#1-a-quick-action-belongs-to-the-source-that-found-the-problem)), honours the
same `confirm` ([§FS-006-project-interface.9](FS-006-project-interface.md#9-offers-the-projects-actions)) —
which on the command line is `--yes`, since a second keystroke has no meaning
where there is no first one — and checks out the branch workspace first where
the entry needs one and it is not there ([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)). An entry
that hands work over dispatches it, with `--hand` picking who for this
dispatch alone ([§FS-005-dispatch.14](FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)); one that lays a workflow down lays it,
with `--set` answering its inputs ([§FS-005-dispatch.19](FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)). `--command` is the
freehand row: whatever the reader wants to run once, in the resolved place,
with the dossier already exported ([§FS-005-dispatch.10](FS-005-dispatch.md#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)).

## 2. Branches, and where each one stands

`ephor branches` prints what the registry knows about a project's branches
and what the disk says about them: whether the workspace is there, how far
each trails its main branch and how far it stands from its own published
copy ([§DA-003-upstream-is-the-published-copy](../decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config)), how fresh that comparison is,
and how many of the project's matters are on it. This is the reading behind
every branch row, and it is what makes `rebase` and `checkout` usable without
first opening a screen to learn which branch to name.

## 3. The board is answerable without the screen

`ephor operations` prints the operations board ([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)): every
execution root found by looking, each live run, each claim, each queued
ticket, each parked question, and the jobs ephor is running itself
([§FS-005-dispatch.17](FS-005-dispatch.md#17-a-move-that-needs-nobody-runs-beneath-the-screen)). `ephor job list` remains the jobs alone; this is both
halves in the order the board puts them, because "what is going on" has one
answer and a reader should not have to know which half of the machine is
doing it.

## 4. A conversation, and the moves inside it

`ephor thread <item>` prints a matter's recorded conversation — every
message, its author, when it arrived, what is on it, and the reply a run
drafted where one is waiting ([§FS-005-dispatch.13](FS-005-dispatch.md#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal)).

The moves inside a conversation are commands as well as keys: `ephor react`
posts a reaction, `ephor tick` resolves a task the source reported
([§FS-004-quick-actions.5](FS-004-quick-actions.md#5-a-task-is-ticked-where-it-is-read)), and `ephor reply` sends the drafted reply, or a
reply given in words, where the channel declares that it can carry one
([§FS-007-matters.4](FS-007-matters.md#4-a-channel-says-what-it-can-do)). Each refuses by name where the source cannot carry the
move, which is the same sentence the key answers with.

## 5. What can be done about a matter, before anything has been

`ephor work offers` prints what could be handed over about one matter and
what already has been: the recipes that match it, the workflows that could
be laid beside it, the tickets that exist with their states and verdicts, and
what ephor has run about it itself. `ephor work list` remains the ledger
across everything; this is one matter's work screen ([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)).

## 6. The gate, spelled out

`ephor failures` answers about a matter by its feed id as well as by the four
coordinates a quick action passes it, and prints the forge's own reasons for
refusing the merge beside what failed ([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)). `ephor
restart` takes the same id.

## 7. `--json` is the same answer, not a second one

Every command that prints a reading takes `--json` ([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)).
Under it, standard output carries the reading alone: notes, progress and
provider failures go to the error stream, so what a program parses is never
interleaved with what a person reads.

A command that changes something prints its outcome under `--json` too —
what was dispatched, what was rebased and how each repository ended, what a
restart asked for and what it skipped, which tickets were cancelled and which
were left waiting, which managed workspaces were rewritten. The exit code
keeps saying what it said; the JSON says what happened.

A command that is **refused** answers the same way. Under `--json` the reason
lands on standard output as an outcome with `ok` false, whatever that command
would have printed had it worked, and the exit code is unchanged. A refusal
narrated only to the error stream leaves standard output empty, which is what a
move that worked and had nothing to say also looks like — so a program acting
on the reading alone would act on a move that never happened
([§GRUND-001-overseer.2](../grund.md#2-what-this-project-does-about-it)).

Each shape is declared, and `ephor schema views` prints the document holding
every one of them ([§FS-006-project-interface.11](FS-006-project-interface.md#11-the-interface-is-versioned)) — every reading, and every
outcome a move reports. A field is added freely; renaming or removing one is a
release note ([§REQ-002-parity.4](../requirements/REQ-002-parity.md#4-the-machine-form-is-a-contract-not-a-dump)). The declaration is held to what the commands
actually print rather than to what they are named: a shape published as one
thing while its command prints another is a contract nobody can hold, and it
fails the build like any other untruth.

Three commands print a document that is somebody else's rather than ephor's:
`ephor feed` and `ephor status` print the matters a source reported, and
`ephor list` prints the registry's own rows. Those shapes are declared where
they belong — the forge answer schema and the registry schema — and named
here as pointing there, because one document described twice is two
descriptions to keep true and only one of them will be.

`ephor work states` prints a fourth such document — the state machine tickets
run under, which is the runtime's own language ([§FS-005-dispatch.11](FS-005-dispatch.md#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed)). It is not
declared by reference, because ephor knows something about it the document does
not say: *which* machine is in force, the one ephor ships or one this site
configured. So the machine rides in a field of a shape of ephor's own, beside
where it came from. A reader who got only the document could not tell the two
apart, and they are different answers to "what states can a ticket be in".


## 8. What is going is said, and the way in is printed

`ephor actions` marks every entry that has work going about its subject
([§FS-005-dispatch.21](FS-005-dispatch.md#21-what-is-already-going-is-shown-where-it-could-be-started-again)) with the same facts the screen sets apart: what is
running — the job, the run, or the window — since when, and what it is at. And
it prints **the way in**, because the way in is the ability and spawning the
reader's own program on it is not ([§REQ-002-parity.1](../requirements/REQ-002-parity.md#1-an-ability-is-a-key-that-reveals-a-fact-or-changes-the-world)): a job's log path, a
run's id and the runner's own attach command in the runner's own words, its
control address while it serves one, a window's handle. `ephor operations`
prints the same for every live root, beside the runner's own command for
stopping it ([§FS-005-dispatch.20](FS-005-dispatch.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)) — shown, never run.

`ephor actions open <id>` is the key on a running row
([§FS-005-dispatch.21](FS-005-dispatch.md#21-what-is-already-going-is-shown-where-it-could-be-started-again)) as a command: it follows the log, attaches to the run,
or brings the window forward, by the same binding the key uses
([§FS-005-dispatch.22](FS-005-dispatch.md#22-a-window-of-the-readers-own-where-one-is-bound)), and refuses by name where the entry has nothing going.
`ephor operations attach <run>` is the same move from the board's side, where
the row is an execution root and not an entry ([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)): every key
the board holds is a command too ([§REQ-002-parity.2](../requirements/REQ-002-parity.md#2-parity-runs-both-ways)), and a run somebody
started in another terminal is reachable by its id alone.

`ephor work run` starts the runtime detached and prints the run's id
([§FS-005-dispatch.20](FS-005-dispatch.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)); `--watch` keeps the terminal and watches the run as
before, which is also what a runner that cannot detach does unasked, saying so.


## 9. A scope selector is honoured or refused

`--workspace`, `--tag` and `--org` are declared once and carried by every
command, so every command's help advertises all three. A command therefore
either **honours** the selector it was given or **refuses** it by name and
exits non-zero. A flag that parses, prints in its help and changes
nothing is worse than one that errors, because the caller cannot tell the two
apart from the output: a sweep that believes it is scoped to one organization
runs over the whole site and nothing in what it prints says so
([§GRUND-001-overseer.2](../grund.md#2-what-this-project-does-about-it)).

The three project selectors — `--workspace`, `--tag`, `--org` — name a set of
projects, read from the registry. The verbs that read a set of projects honour
them: `list`, `status`, `feed`, `refresh`, `mark-read`, `branches`, the screen,
`work list`, `work dispatch`, `work sync`, `work run`, and the managed-workspace
verbs `validate`, `ensure-agents` and `update`. Every other verb names one
target or reads no project set at all, and refuses each selector it was given,
naming itself and the flag and saying which selectors it does take. The
classification is total: every verb is on one side of it or the other.

The registry and the site's watch list are two different files, and the
selectors name rows of the first while `status`, `feed`, `refresh`,
`mark-read`, `branches` and the screen pick their projects from the second. A
selector is therefore resolved against the registry and then intersected with
what the site watches, and the intersection is applied where each verb picks
its projects rather than in a helper beside them.

A selection that comes out empty is **said**, never left looking like a quiet
site: a mistyped organization and an organization with nothing in it otherwise
print the same empty table, and that table is the failure this rule exists to
end. The verbs that read the watched projects refuse it, naming which end came
out empty — no such project in the registry, or none of the selected ones
watched here — and `ephor list`, whose whole reading is the rows, says in words
that no project matched.

`--all` is not one of them, and that is the second half of this rule: a flag
belongs to the verbs that read it. It was global, it meant **every branch
entry rather than only the active ones** to `validate`, `ensure-agents` and
`update`, and it meant "every project" to `mark-read` — one flag, two
meanings, and no way to tell from the output which one a run had used. It is
now declared by each verb that reads it and says there what that verb means by
it: the three managed-workspace verbs keep the branch-entry meaning, and
`mark-read --all` sweeps every watched project the way `work offers --all`
shows the tickets that are over. A verb that would ignore it does not declare
it, so it is refused before ephor is asked rather than advertised in help that
has nothing behind it.

A refusal is an answer like any other: under `--json` it lands on standard
output as an outcome with `ok` false ([§7](#7---json-is-the-same-answer-not-a-second-one),
[§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)). It is decided before any registry is read, so the
verbs that need no registry — `schema`, `check`, `validate --manifest` — still
need none in order to refuse.
