# DA-006-hands-fill-a-workflows-targets: who a workflow's agents are is ephor's answer, not the workflow's

**Status:** Accepted
**Date:** 2026-08-21

A workflow the runtime offers is mostly a list of agents (§FS-005-dispatch.19):
which one reviews, which one adjudicates, which one writes the tests. Each of
those is an input, and each carries a default its author happened to be running
— a model name, at an effort, spelled in the binding's own selector grammar.
When ephor instantiates such a workflow it has to decide whose answer those
inputs take: the workflow's, or its own (§FS-005-dispatch.14,
§FS-006-project-interface.9). This record fixes ephor's and names what the
other would have cost.

## 1. The decision

An input a workflow declares to be an execution target resolves through the
seven steps like every other hand, and is rendered into the binding's selector
in the runtime module alone (§AR-007-runtime.1). Which inputs those are is read
from the workflow where its manifest says so, and from the entry that names the
workflow where it does not — an entry lists them, and listing one is how a
person says "this input is a hand" about a workflow whose author never marked
it.

The narrowing binds here exactly as it binds everywhere else
(§FS-006-project-interface.9): a hand a project does not permit is refused with
that reason wherever it was named, and a workflow's own default is a naming. A
project that narrows the roster and instantiates a workflow whose defaults sit
outside the narrowing is told so, before anything is written.

## 2. The rejected alternative

Leaving every input at the workflow's default unless an entry maps it. Its
appeal is real and worth stating: what you get from ephor is then exactly what
you would get from running the workflow by hand, and a workflow that was tuned
— this reviewer is deliberately a different model from that one — keeps its
tuning without anybody restating it in ephor's words.

It fails on the policy. `work.hands`, the reader's pick at the moment of
asking, and `work.permitted_hands` are all promises about *who sees this
repository's code*, and under that alternative all three hold for tickets and
none of them holds for workflows — with no sign on the screen that the rule
stopped applying. A repository under a policy about which models may read it
would be observing that policy right up to the keystroke that mattered. And
the failure is quiet: the reader who narrowed the roster sees a workflow
instantiate cleanly, and learns which models actually ran it from the other
side, which is the exact confusion §FS-006-project-interface.9 refuses for
configured hands.

## 3. The cost

A workflow's deliberate spread of models collapses to one hand unless the entry
says otherwise, and "unless the entry says otherwise" is configuration somebody
has to write — the tuning is not lost, but it is no longer free. An entry may
answer a list-shaped target input with several hands, which is where a spread
is restated in ephor's own vocabulary; nothing is expressible in the workflow
that is not expressible in the entry.

The second cost is a dependency on what the binding reports. Where its listing
does not say which inputs are execution targets, ephor reads the workflow's
manifest for it, and where it can read neither — a workflow the binding keeps
inside itself and does not describe that far — the entry has to list them by
hand or the workflow's defaults stand. That is a gap in what the binding
publishes, not a decision, and it closes the moment the binding publishes it.
