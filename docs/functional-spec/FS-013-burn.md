# FS-013-burn: what this machine spends on agents is a reading like any other

Everything ephor watches costs tokens, and until now ephor could not say how
many. The agent command-line tools already write down every one of them, and
the runtime already meters its own runs, so the fact exists on disk in two
places and is reachable from neither. A watch that cannot answer "what did the
last hour cost" is answering "what needs me now" with half the picture
([§GOAL-002-glance](../goals.md#goal-002-glance-one-glance-answers-what-needs-me-now)).

So there is a **burn** reading: current rate, the last hour, six hours, day
and week, grouped by project, model, session, plan or matter. It is a screen
and a command, like every other ability
([§REQ-002-parity.1](../requirements/REQ-002-parity.md#1-an-ability-is-a-key-that-reveals-a-fact-or-changes-the-world)), and it costs the watched nothing: it reads logs
those tools were already writing ([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)).

## 1. Two lenses, and they are never added together

The two records answer two different questions and overlap in a way nothing
can subtract exactly, so they are kept apart and labelled.

**The machine lens** is what this machine burned. It is built from the agent
command-line tools' own transcripts, and it is the ground truth for a total:
it covers interactive sessions and runs the runtime started alike, because
the runtime shells out to the same tools. It groups by project, by model and
by session.

**The work lens** is what a piece of work cost. It is built from the
runtime's own accounting records under each work root, which name the plan,
the ticket and the state that spent them, and it reaches a matter through
ephor's own ledger — a plan was dispatched *for* a matter, so a matter's burn
is the burn of the plans dispatched for it ([§FS-005-dispatch.4](FS-005-dispatch.md#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)). It groups
by plan and by matter.

A run measured by both appears in both. **Neither reading ever contains the
other's numbers, and no total sums them.** A surface that added them would
double-count every run the runtime started, and a reader cannot see that
happening. Where the two are shown together they are shown as two tables with
a rule between them.

## 2. The work lens says what it did not measure

The runtime records an invocation whether or not the tool it ran reported any
usage, and some do not. A lens that quietly dropped those would show a plan
costing a third of what it cost, which is worse than showing nothing.

So the work lens carries, beside its numbers, how many invocations it read
that recorded no usage, and which agents did report. That sentence is on the
reading and in the machine form — never inferred by the reader from a number
that looks low.

## 3. What is counted, and the two ways of counting it wrong

Four counters per record: input, output, cache-read and cache-write. They are
kept apart all the way to the screen, because a cache read and an input token
are priced an order of magnitude apart and a single "tokens" number hides
which one moved.

Each log has one shape that reads plausibly and is wrong, so each is named
here rather than left to whoever writes the next reader:

- **The assistant record's own `usage` is the whole of it.** A record also
  carries a per-iteration breakdown of the same call. Summing the outer
  counters *and* the breakdown counts every token twice.
- **The session's token counter is cumulative, not per event.** Each event
  restates the session's running total, so the tokens an event spent are its
  total minus the previous event's. Summing the events themselves inflates a
  session by orders of magnitude.
- **A session changes model mid-way.** Each delta is attributed to the model
  in force at that event, never to one model for the whole session.
- **Sub-agent transcripts are real spend and are counted**, tagged so they
  can be told from the session that spawned them.

Provider stays beside model in every key: not every model a tool runs is
priced or served by the vendor whose tool it is.

## 4. Every record finds a project, or lands in `other`

Each record carries the directory it ran in. That directory is matched
against the registry — each project's root and each branch workspace its
`branch_root_template` names — longest match first, so a branch checkout
inside a project root is attributed to the project rather than to whichever
row happens to be checked first ([§FS-008-attribution.1](FS-008-attribution.md#1-identity-is-declared-and-the-row-has-the-last-word)).

A directory under no registered root is not dropped and is not guessed at: it
lands in a project named `other`. Burn ephor cannot attribute is still burn,
and a total that quietly excluded it would be wrong in the one direction a
reader cannot detect.

## 5. Buckets, not transcripts

Reading the transcripts is a scan of everything the machine has ever run, so
it is done once and then only over what was appended.

**Cursors.** One cursor per transcript file, keyed by path, recording the
byte offset already read, the size the file had then, and whatever the reader
must carry across a scan — the session's last cumulative counter, the model
in force, the last cost total seen. A file whose size and modification time
are unchanged is not opened at all; one that has grown is read from its
cursor to its last complete line; one that has *shrunk* is read from the
start, because it is not the file the cursor was about.

**Buckets.** What a scan reads is aggregated into five-minute buckets keyed by
`(project, source, provider, model, session, sub-agent)` and written to one
file per day. Every window is then a sum over buckets, so changing the window
re-reads nothing. Day files older than thirty days are deleted on the next
scan.

The store lives under ephor's own state directory, beside the feed cache, and
holds no transcript text: counters, and the keys they are counted under.

## 6. Windows, groupings, and the current rate

Four windows — one hour, six hours, twenty-four hours, seven days — and five
groupings: project, model, session, plan, matter. The first three read the
machine lens, the last two the work lens; asking for a grouping selects the
lens that can answer it.

**The current rate** is the last five minutes' tokens over five minutes, and
beside it the sessions that are still going: a transcript written to in the
last few minutes, with what it has spent per minute since. That is the whole
of "live" in this version — it is read from the same buckets and file times
as everything else, and nothing is subscribed to.

## 7. Dollars are opportunistic, and `unpriced` is never `$0.00`

Tokens are the reading. A cost is shown only where the log carried one
already; ephor computes no prices and ships no price book.

**Unknown and zero are different facts and stay different**, in the machine
form and on the screen. A group nothing priced prints `unpriced`, never
`$0.00` — a reader who sees a zero concludes something was free, and being
wrong about that is the whole reason the distinction is written down here.

## 8. The page and its command

The reading is `ephor burn`, with `--window` and `--by`, and `--json`
printing the published shape like every other reading
([§REQ-002-parity.3](../requirements/REQ-002-parity.md#3-every-reading-answers-a-program)). The screen is the Burn page, opened over whatever
the reader was looking at and closing back to it.

**Neither surface scans a transcript while drawing or while a key is held.**
The screen refreshes the store from the same tick that watches the work
artifacts ([§FS-005-dispatch.15.1](FS-005-dispatch.md#151-the-board-keeps-itself-current)), and the command refreshes inline only
when the store is more than thirty seconds stale. Both are local file reads,
so this is not a fetch: `refresh` remains the only verb that asks the world
([§FS-001-forge-interface.7](FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)).

## 9. What this version leaves out

Written down rather than left as an absence
([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)):

- **A price book.** Prices are site data and would belong beside the
  registry, not in the binary. Until there is one, everything the logs do not
  price is `unpriced`.
- **Per-invocation live detail.** The runtime emits a usage event per
  invocation into the stream ephor already tails; the live strip reads file
  times instead, which is enough to say what is going and costs no new
  coupling.
- **Drilling into a row.** A row groups; opening one to see what it groups is
  a second reading, and both surfaces would owe it.
