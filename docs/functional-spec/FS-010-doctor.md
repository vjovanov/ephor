# FS-010-doctor: ephor can be asked whether it still works, and answers in one screen

The watch is only worth having if it is believed when it says there is
nothing to do ([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)). Everything that makes that claim
false is quiet: a credential that expired, a forge whose extension left
`PATH`, a checkout somebody deleted, a runner that a system upgrade removed.
None of them announces itself — each one simply makes a section of the feed
empty, which is the one thing an empty section must never mean
([§FS-001-forge-interface.6](FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).

So ephor can be **asked**. `doctor` answers "is this still working" for the
whole site in one run, and it is built to be run on a timer by whoever wants
one: it needs no argument, it prints what is wrong and what would fix it,
and it says so in its exit code.

## 1. It reports what is already judged, and judges nothing itself

What a project can do is the ladder of [§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung), and why
a rung is missing is a sentence that already exists. What a source did is
[§FS-001-forge-interface.6](FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)'s answer, with its own split between a
configuration to go and fix and a network to wait out. `doctor` composes
those two and adds no opinion of its own.

This is the whole of the design rule. A second opinion about whether a
project is checkout-able would be a diagnosis that drifts from the one the
menu refuses with, and a reader holding two answers has none: the sentence
`doctor` prints and the sentence a greyed entry shows are the same sentence.

A third fact composes the same way. Where a project's checkout still keeps a
piece of tool configuration under the deprecated `.agents/` name, that is the
probe's answer and never `doctor`'s ([§FS-006-project-interface.12](FS-006-project-interface.md#12-what-the-toolchain-keeps-in-a-checkout-has-a-home-and-a-deprecated-one)): `doctor`
prints the sentence the probe wrote, per project, in the report a person reads
and in the machine form a program reads alike ([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)). It is news
rather than a fault, so it moves neither the project's health nor the exit code
(§5) — a deprecated path stops the project doing nothing, and the ladder has no
rung for it ([§FS-006-project-interface.10](FS-006-project-interface.md#10-capability-rung-by-rung)).

## 2. The ladder is answerable on its own

The same table is worth having without a sweep. **`capabilities`** prints one
project's rungs — held, and missing with their reasons — so that "why is this
action not offered here" is a question with a cheap answer. `doctor`'s first
pass is this for every configured project.

## 3. Two passes: the site, and ephor itself

**The site pass** asks the world: the registry parses, every project's row
resolves, each ladder is computed, and every configured source is asked —
refreshed rather than read from cache, since a cached answer cannot say
whether a source still answers. What it reports is per project, and a
project that is entirely well says so in one line.

**The self pass** asks the binary. It builds a throwaway project — its own
state directory, registry, configuration and checkout, in a temporary place —
and walks the seams end to end against it: a forge reached out of process
([§FS-001-forge-interface.2](FS-001-forge-interface.md#2-two-transports-one-interface)), a refresh that categorizes what came back, a
summons answering by exit code and by envelope ([§FS-006-project-interface.3](FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out)),
a check verb bound by manifest and by probe ([§FS-006-project-interface.5](FS-006-project-interface.md#5-checks-are-verbs-and-every-script-is-self-contained)),
the git operations both a key and a state machine run
([§FS-004-quick-actions.6](FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase), [§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)), a dispatch that writes a
plan and reads its ledger back ([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)), and a task store
read where it lives ([§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).

It touches nothing of the person's, reads no registry of theirs, and reaches
no forge: it is hermetic, or it is not a diagnosis. This is what a test suite
cannot answer — `cargo test` speaks for the tree it was run in, and the
question here is whether the binary on *this* machine still works.

**Both passes say what they are doing while they do it.** Asking every source
of every project takes as long as the slowest forge, and a diagnostic that
prints nothing until it is finished is one a reader kills half way through
and reports as hung — which is the same failure the tool exists to name, with
ephor as the source that did not answer. So each step is announced as it is
reached and each answer is given as it arrives, on the error stream, where it
narrates the run without becoming part of it: what a program reads is the
report, and a progress line that reached a parser would be ephor writing to
its own contract.

## 4. Nothing of the reader's is written

The site pass refreshes and reads; the self pass writes only inside the
temporary place it made and removes it afterwards. `doctor` posts nothing,
dispatches nothing, and changes no checkout of the reader's — a diagnostic
that repaired things as it went would be a second thing to debug, and one
that could not be run while unsure is one nobody runs.

## 5. The answer is in the exit code

A timer reads exit codes, not screens. `0` is well. `4` is degraded — a rung
missing or a source lost, which is the condition a person acts on. `3` is
nothing on the site reachable at all. `1` is the self pass failing, which is
ephor itself being wrong rather than the world being away, and is the one
answer that does not improve by waiting.

