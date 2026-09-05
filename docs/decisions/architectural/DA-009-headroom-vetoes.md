# DA-009-headroom-vetoes: headroom is a report ephor is given, and it vetoes a member rather than ranking the list

**Status:** Accepted
**Date:** 2026-09-05

A pin that may name alternates ([§FS-005-dispatch.14](../../functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)) needs something to say
which of them cannot be had right now, and there are two questions hiding in
that sentence. Where does the capacity number come from — is it something
ephor works out, or something ephor is told? And what may it do to a list a
person ordered — reorder it, or only strike members out of it? This record
fixes both answers, and they are one answer twice: capacity is a fact about
somebody else's system, so ephor consumes it as evidence and lets it refuse,
and evidence never outranks a judgment.

## 1. The decision

**ephor consumes a report and never derives one.** The two channels
[§FS-005-dispatch.29](../../functional-spec/FS-005-dispatch.md#29-headroom-is-reported-to-ephor-and-vetoes-a-member-it-never-reorders) sets out are both reports: a refusal the provider itself
made, which ephor recorded because ephor made the failed start
([§FS-005-dispatch.4](../../functional-spec/FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)), and a bound verb whose answer is somebody else's
statement about somebody else's quota. Neither is a calculation. That follows
directly from the law this tool is built on — ephor observes and summons, and
it never governs ([§REQ-001-boundary](../../requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy)) — and the law is not decoration here: a
quota belongs to whoever sells it, is spent by everything holding the same
credential, and is changed by its owner without telling anyone. Anything ephor
computed about it would be a second opinion competing with the only real one.

**Headroom vetoes, and the written order picks.** A member is passed over
exactly when its pool is known spent; every survivor keeps the position its
author gave it. The list is a ranking by fitness — the first name is first
because it is the right hand for this work — so the only thing capacity is
allowed to say is *not this one, not now*.

**Unknown is a third value, and the rule has no slot for it.** Making the rule
a veto rather than a sort is what makes "unknown is not zero" structural
instead of advisory. A sort has to place every candidate, so it must decide
what an absent number sorts as, and every answer to that question is wrong in
a machine where absent is the ordinary case. A veto asks one question of one
member — is this pool known to be spent — and a pool nobody reported on is not,
so it survives with its position untouched and nothing had to be decided about
it at all.

## 2. The rejected alternatives

**Sort by most remaining, then by least spent since reset.** This was the
first proposal, and rejecting it is what produced the veto. It breaks the
list's meaning: the alternates are not equals, and sorting them by a capacity
needle makes which model reads a person's code flip between families while the
preferred pool is still healthy — a quality regression bought with no
throughput at all, since the preferred pool could have taken the work. Its
second key is worse: *least spent since reset* can only be measured by ephor's
own count of what ephor spawned, which is deriving a quota under another name
and is wrong the first time a terminal session, another tool, or another
machine spends from the same credential. Most-remaining survives as the right
tiebreak inside a rank of genuine equals — one model reachable through two
credentials — which the payload can express when such a rank exists.

**Let ephor derive headroom by counting its own spawns.** Free, needs no verb,
and no vendor knows anything about it. It is also not a quota: it counts one
process's acts and calls the result a property of a shared allowance, so it
is wrong by however much everything else used, and its error grows exactly
when the machine is busiest. Rejected on [§REQ-001-boundary](../../requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy). The count stays,
and stays display-only, because what ephor spawned is worth showing a reader
and is not worth believing.

**Round-robin across the permitted hands, with no evidence at all.** It
spreads load and needs nothing bound. It also cannot tell an exhausted pool
from a healthy one, so it moves work off the chosen model roughly half the
time for no reason, and it still stalls when the pool it lands on is the spent
one. It trades a visible stall for an invisible quality regression, which is
the worse of the two because nobody can see it happening.

**Solve it in the runtime instead.** The runtime could pick among carriers
itself and ephor would need none of this. But `work.permitted_hands` is a
*project's* policy about which models may see its code, and it lives on
ephor's side; a runtime choosing carriers underneath that policy is a
narrowing with a hole in it. Whoever enforces which names are allowed has to
be whoever picks among them.

## 3. The cost

**A rule that only ever refuses cannot balance.** With two healthy pools every
ticket goes to the first, and the second is touched only once the first is
spent. That is deliberate — it is what keeps the author's choice — but it
means this feature does not spread load, it survives exhaustion. A site that
genuinely wants its work spread has to say so by writing different lists on
different actions.

**The free channel is one refusal behind.** The ledger learns a pool is spent
by spending a dispatch on it and failing, so the first ticket after a window
closes is the one that pays. Bounding that is the whole reason the verb
exists, and the verb is optional, so a site that binds nothing accepts one
failed start per window. It is a cheap failure — the ticket is written, only
the run refused — and the record makes it the last one until the reset.

**Evidence ephor was given can be stale or wrong, and it is trusted anyway.**
A verb that reports zero for a pool that is fine will pass that pool over
until it says otherwise, and ephor has no way to check the claim; that is what
consuming a report means. The bound on the damage is the rule itself: the
worst a wrong report can do is move a ticket to the next member of a list the
person wrote, or leave it on the first with a note, and never send it to a
hand nobody named.
