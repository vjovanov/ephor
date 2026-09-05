# FS-015-spend-ceiling: what unattended work may spend is the person's number, and the sweep stops at it

Work that starts with nobody present is bounded by how many roots may be live
and how many of those may be working, and by nothing else
([§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)). Neither of those is money. A cheap model at high
concurrency and an expensive one at low concurrency are indistinguishable to
every ceiling ephor has, so the only thing that actually stops an overnight
sweep is the provider's own quota — the vendor's number, arriving as a refusal,
rather than the person's, arriving as a decision. A machine with unlimited
quota still needs a budget, and a machine well under budget still hits a
vendor window; the two are not each other.

Ephor could not have had this ceiling until it had the reading, and now it has
one: `burn` says what the agent tools spent, over four windows, grouped by
project ([§FS-013-burn](FS-013-burn.md#fs-013-burn-what-this-machine-spends-on-agents-is-a-reading-like-any-other)). So there is a **second dimension beside concurrency**,
written at the same three nested scopes, read by the same sweep, and bound to
what `burn` already measured. Ephor computes no prices and ships no price book
here any more than it does there ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)): the ceiling is a
comparison against a reading somebody else took.

What it buys is the other half of the trade [§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself) made. That
section removed the requirement that a person be present when work starts;
this one lets the person who is not present say how much of their money that
may cost, so leaving the loop running is a decision rather than a hope
([§GOAL-004-handover](../goals.md#goal-004-handover-routine-moves-leave-the-persons-hands)).

## 1. Two ceilings, in the three scopes the registry already nests

A budget is written in **site configuration and nowhere else**. It is an
operational binding — the person's choice about their own money — which
[§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order) puts in site configuration, and [§REQ-001-boundary.3](../requirements/REQ-001-boundary.md#3-requirements-on-a-project-are-capabilities-never-artifacts)
forbids a project from being required to carry. The amount somebody will spend
is not a fact about the repository.

Two keys, beside `max_concurrent` and `max_active` and read at the same three
scopes: the site's `work`, `organizations.<org-id>.work`, and
`projects.<id>.work`.

```json
{
  "work": {
    "max_spend":  { "amount": 50, "currency": "USD", "per": "24h" },
    "max_tokens": { "amount": 200000000, "per": "24h" }
  }
}
```

`max_spend` is a ceiling on dollars and `max_tokens` a ceiling on tokens. They
are two ceilings rather than one key with a unit, because a site may want both
at once and because they fail differently — see [§2](#2-a-dollar-can-be-wrong-in-a-way-a-token-cannot).

Which organization a project belongs to is the `organization` field on its
registry row and nothing else, read here and never written, exactly as the
concurrency ceilings read it ([§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself), [§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)).

**What the ceiling reads is the store `burn` publishes, and it reads it
itself.** A sweep runs from a timer with nobody present, so it cannot depend
on somebody having opened a reading first: it refreshes the store inline when
that store is stale, exactly as the reading's own command does
(§FS-013-burn.8). That is a local file read of logs the agent tools were
writing anyway and is not the fetch `refresh` owns — a budget must never make
the sweep reach the network. Ephor keeps no second accounting of its own: the
number the ceiling is compared against is the number `burn` would print for
the same window and project.

**So the site scope counts the whole reading, and the inner scopes count only
their own.** Every record `burn` could attribute to no watched project lands in
`other` ([§FS-013-burn.4](FS-013-burn.md#4-every-record-finds-a-project-or-lands-in-other)), and the site total is the total `burn` prints, `other`
included. That is deliberate rather than incidental: a site budget is a ceiling
on what this machine spends, and a session somebody ran by hand in an unwatched
directory spent that money exactly as a dispatched run would. Excluding it
would make the site ceiling mean *only what ephor started* — the caveat that
reading `burn` rather than ephor's own ledger exists to avoid. An
organization's and a project's ceilings count only the projects the registry
places in them, so a scope narrower than the site is never moved by spend
outside it.

## 2. A dollar can be wrong in a way a token cannot

Both denominations ship, and the reason is not symmetry. Tokens are the
reading; a dollar figure is shown only where the log carried one already
([§FS-013-burn.7](FS-013-burn.md#7-dollars-are-opportunistic-and-unpriced-is-never-000)). That makes a dollar ceiling a comparison against a number
somebody else computed, and it can be wrong in the one direction a ceiling
cannot survive: too low.

**A dollar budget can under-count when the runtime reports a zero it should
have reported as unknown.** [§FS-013-burn.7](FS-013-burn.md#7-dollars-are-opportunistic-and-unpriced-is-never-000) keeps `unpriced` and `$0.00` apart
precisely so a hole is visible, but that distinction only holds where the hole
is reported as a hole. A price book with no entry for a model that reports the
result as a priced zero has produced a fact ephor cannot tell from a genuinely
free call, and a budget bound on it counts real spend as nothing. This is not
hypothetical: a run of this kind read hundreds of millions of tokens as
`$0.00`, and read another vendor's dollars six times too low, over a period in
which the token counts were right both times.

That hole is upstream and is not ephor's to close. Ephor consumes the report
and does not price anything ([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)), and it does not second-guess
a reported price: a zero that arrives priced is taken as a zero, because
overriding it would mean holding the price book this specification refuses to
hold.

**So the token denomination is the answer for a site that does not want to
depend on the pricing being right**, and that is what it is for. A site that
trusts its runtime's prices writes `max_spend`; a site that would rather bound
the thing that is always measured writes `max_tokens`; a site that wants both
guarantees writes both, and the first of them to be full is what refuses.

## 3. What `currency` and `per` accept, and why they accept so little

**`currency` is required on `max_spend` and accepts `USD`.** It is present
from the start so that the shape never has to grow a key later, and nothing
else is accepted, because accepting a currency ephor cannot convert to what
the logs carry would be a ceiling that silently compares two different
numbers. Anything else is refused, and the refusal names what is accepted.
`max_tokens` carries no `currency`: tokens are not denominated in anything.

**`per` accepts exactly the windows the store can answer** — `1h`, `6h`,
`24h`, `7d` — with `day` and `week` accepted as spellings of the last two
([§FS-013-burn.6](FS-013-burn.md#6-windows-groupings-and-the-current-rate)). A window the store cannot answer is a budget that cannot
bind, so it is refused rather than approximated, and the refusal names every
spelling it accepts — the two alternative spellings beside the four windows,
because a reader who wrote one of those two is owed the same list as anybody
else. The window is the trailing one the reading uses, not a calendar period:
`24h` is the last twenty-four hours, not since midnight.

**`amount` is a number and is not negative.** All three keys are required on
the block that carries them; a `max_spend` without `per`, or a `max_tokens`
with a `currency`, is a configuration that means something its author did not
write, and is refused with the same clarity.

Every one of these refusals is the configuration failing to load, which is
where a site's own typos already land, and which is the only outcome that
cannot be mistaken for a ceiling that is quietly doing nothing.

## 4. Omission is unlimited, and `0` is a pause somebody wrote

Read exactly as the concurrency ceilings are read ([§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)).
Leaving a budget out at a scope leaves that scope unbounded in that
denomination, so **a configuration that names no budget behaves exactly as it
did before this existed** — no new refusal, no new output, nothing to migrate.
Writing `0` admits no new autorun starts under that scope.

A `0` budget is a pause its author meant, not a window that will pass, so it
names no resume instant ([§8](#8-the-resume-instant-is-the-earliest-one-the-measurement-allows)) and is not
read as a ceiling that some other ceiling could be written above.

## 5. Every ceiling is evaluated, and the outermost full one is the reason

The new dimension composes with what is there; it replaces nothing and clamps
nothing. A start is refused by whichever ceiling is full first, asked
outermost scope inward — site, then organization, then project — and within
one scope in a fixed order: roots in flight, then working roots, then money,
then tokens. So the reason a reader is given names the widest thing that was
actually full, `max_concurrent` and `max_active` answer exactly as they did
before there was a budget, and a scope that declared no budget is not asked
about one.

Nothing about a budget changes what the concurrency ceilings count, and
nothing about a concurrency ceiling changes what a budget measures. They are
four questions asked of the same start, and the answer is the first *no*.

**A budget written the wrong way round is not warned about.** A project
ceiling above its organization's is named for concurrency because both count
the same live roots and the inner number can only be a mistake
([§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)). Spend does not work that way: an organization's
measured total includes every project's, so a project ceiling above its
organization's is simply a project that will be stopped by the outer number
first, which is the ordinary case rather than a contradiction. Saying so every
sweep would be a warning about correct configuration.

## 6. Only the sweep is bound, and the person's key never is

[§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself) already decides this and its sentence governs here
unchanged: *these limits apply only to this sweep: a run a reader explicitly
starts keeps being the reader's move.*

**The budget binds every sweep that starts work with nobody present.** That is
`ephor work run --due`, and it is also the same sweep as it runs at the end of
`ephor work sync` — both of which a timer invokes, and neither of which anybody
typed. The unit ephor ships runs the two in that order every half hour and its
own comment names the first as the trigger and the second as the backstop, so a
budget that bound only `--due` would let `work sync` start the night's work
unbound and leave `--due` finding those roots already held. It would bind
almost nothing. `max_concurrent` and `max_active` already bind both sweeps, and
[§5](#5-every-ceiling-is-evaluated-and-the-outermost-full-one-is-the-reason)
says the four ceilings are asked of the same start.

Nothing else is refused for budget. `ephor work dispatch`, `ephor work lay`, a
`work run` on a named plan or item, and the key in the interface all **warn
and proceed**: the warning names the ceiling that would have refused, on the
error stream where a warning belongs ([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)), and the run starts.
`work dispatch` ends in the same sweep `work sync` does and is still not bound
by it, because the difference is not what the sweep does but who asked: a
person is at the terminal for one and nobody is for the other.

The cap is the human's leash on the machine, not on the human. A person who
types the command is present, is deciding, and can see the warning; refusing
them would turn a budget into a lock-out and would make the first thing
anybody did with it be to remove it.

There is no one-sweep command-line override of a budget. `--max-concurrent`
exists because narrowing a single sweep is a thing a person does while
watching it; widening a spend ceiling for one invocation is indistinguishable
from not having written one, and the named run above is already the way to
start the work anyway.

## 7. Measured spend binds; a hole is shown and never blocks

**What binds is what was measured.** A `max_spend` ceiling compares the
dollars the window's records actually carried. A `max_tokens` ceiling compares
the tokens, which every record carries.

**A hole is counted and shown, never read as zero, and never blocking on its
own.** Beside the measured dollar total, a refusal and the reading behind it
carry the tokens in that window that carried no price at all. Those tokens are
real spend nothing could price; presenting the dollar figure without them
would be presenting a number that is wrong by an unknown amount as though it
were the total.

**A window in which nothing was priced blocks nothing under `max_spend`.**
Where no record in the window carried a dollar figure, the dollar ceiling has
measured nothing and does not bind — it is reported as unpriced, which is not
zero and is not a full ceiling either ([§FS-013-burn.7](FS-013-burn.md#7-dollars-are-opportunistic-and-unpriced-is-never-000)). A ceiling that stopped
the machine because accounting was absent would fail over an extractor outage
rather than over money, which is the failure this rule exists to forbid. The
token ceiling is unaffected: it is the one that binds when the pricing is
missing, which is [§2](#2-a-dollar-can-be-wrong-in-a-way-a-token-cannot)'s whole point.

## 8. The resume instant is the earliest one the measurement allows

A refusal that says only *no* leaves the reader to guess whether to wait ten
minutes or rewrite the configuration, so a budget that is full says when it
will not be.

The window is a trailing one, so it empties as spend ages out of it. **The
resume instant is the earliest moment at which the measured total in the
window falls below the ceiling, computed from what is already measured.** It
is an earliest, not a promise: spend recorded between now and then moves it
later, and the instant is recomputed from the reading every time it is asked
for rather than remembered.

The reading's own spans are what age out, so the instant is the moment the
oldest spans that have to leave do leave: the span a record fell in, plus the
window. It is written in RFC 3339, to the second, in UTC, wherever it is
written — a time somebody has to compare against `date` is not a place for a
second format.

A ceiling of `0` names no instant. Nothing ages out into room under a ceiling
of zero, and calling it *paused until* would describe a wait that never ends.
It says instead that it admits no new autorun starts.

## 9. A refusal names the scope, the ceiling, the total, the hole, and the instant

A root the sweep passed over for budget is a `passed-over` outcome exactly as
a root passed over for concurrency is ([§FS-005-dispatch.24](FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)) — not a failed
launch, not an error, and reported in prose and in the machine form alike. Its
reason carries five things, and the configuration key names the first two:

Each is one line:

```text
global work.max_spend 50 USD per 24h is full (52.40 USD measured, 12000000 token(s) unpriced); resumes 2026-09-06T10:05:00Z
projects.demo.work.max_tokens 1000000 per 24h is full (1200000 token(s) measured); resumes 2026-09-06T10:05:00Z
organizations.guild.work.max_spend 0 USD per 24h admits no new autorun starts
```

The scope is spelled as the key that carries it — `global work.<key>`,
`organizations.<id>.work.<key>`, `projects.<id>.work.<key>` — which is the
spelling the concurrency ceilings already refuse in, so a reader who has read
one reason can read this one. Dollars are written to two decimal places, and
the unpriced clause is present only where the window actually holds tokens
nothing priced: a hole that is not there is not reported as an empty one.

## 10. A pause is visible where people actually look

`burn` is the spend reading and stays the spend reading: totals, groupings and
coverage are asked for there, deliberately, and none of them is duplicated
onto `status`. But `status` is the habitual command — the one that runs from a
prompt many times a day — and if the only places a pause is said are the
sweep's own output and a reading nobody thought to open, then **a paused loop
and an idle loop look identical**. Work would stop silently, and the next
person to notice would be whoever wondered why nothing shipped overnight.

So, and only while a ceiling is **currently binding**, `ephor status` says so
in **one line naming the scope and the resume instant**:

```text
Autorun paused: organizations.guild.work.max_spend, resumes 2026-09-06T10:05:00Z
```

Not the totals and not the coverage — those stay in `burn`, and a `status`
that grew a spend report would be the duplication this section is refusing.
Nothing is printed when no ceiling binds, so **the line's presence is the
signal**.

**Currently binding is a fact about the ceiling, not about the queue.** A
ceiling is binding when it is full, whether or not anything is due right now:
the reader this line is for is the one who notices that nothing shipped, and
telling them only once there also happens to be a due root would leave the
quietest nights unexplained. And `--cached` does not suppress it — reading the
store is a local file read rather than a fetch, so what `--cached` refuses is
untouched by it ([§FS-013-burn.8](FS-013-burn.md#8-the-page-and-its-command)).

The machine form carries the same fact, because a fact only a person at a
terminal can learn is a fact nothing can alert on ([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)). In
`ephor status --json`, every project the paused scope holds carries an
`autorun_paused` object with the `scope` that is full and the `resumes_at`
instant, absent under a ceiling of `0` as it is absent from the line. A
project under no full ceiling carries no such object, which is the same
absence the missing line is.

## 11. What this version leaves out

Written down rather than left as an absence ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)):

- **A budget in any currency but dollars.** Not because the key could not
  take one, but because ephor would have to convert, and converting is
  pricing.
- **A budget on a pool, a model, or a hand.** The three scopes are the ones
  the registry nests and the ones the reading already groups by. A ceiling per
  model would need a grouping the sweep cannot resolve to a root.
- **A spend reading on `status`.** [§10](#10-a-pause-is-visible-where-people-actually-look) is a pause, not a report, and the
  report is `burn`.
- **Any fix for a price book that reports an unknown as a zero.** It is
  upstream of ephor and this specification says so in
  [§2](#2-a-dollar-can-be-wrong-in-a-way-a-token-cannot) rather than working around it.
