# Requirements

Declare each behavior or requirement inline as an H2: `## FS-NNN-slug: …`.

## FS-001-forge-interface: ephor reaches every forge and issue tracker through one provider interface

ephor aggregates work from places that host code review and places that track
issues. Which ones they are is a property of a person's employer, not of ephor.
No forge, tracker, or vendor CLI may therefore be named in ephor's core: every
one of them is reached through a single interface with a fixed capability set,
and an implementation is selected per project by configuration.

### 1. Capabilities

An implementation answers some or all of the following; it declares which, and
ephor degrades to what is answered rather than failing.

- **Pull requests by role** — the ones the user authored, and the ones they are
  engaged in as a reviewer. Each carries a stable id, title, url, state, head
  branch, and last-updated time.
- **Conversation** — a PR's threads as ordered messages (author, text, time),
  each with its reactions, and enough identity for a reaction to be posted back
  to it.
- **Reactions** — post a reaction on a message, where the implementation
  supports writing; read-only implementations say so and their messages are
  display-only.
- **Gate status** — the job counts (passed, failed, running) for a pull
  request, per repository the gate covers, since one change may gate across
  several repositories at once; and, where the forge reaches a verdict of its
  own, whether the gate blocks the merge and the reasons it gives for that.
  Counts alone cannot say it: a gate whose jobs are all green may still be
  blocked on an approval, on a downstream repository, or on jobs it has not
  started, and a row showing only what passed reads as finished work.
- **Failures** — for a pull request whose gate is red, what actually failed:
  each failure as the job that produced it, a link to its log, and the error
  text itself where the forge can extract it. Asked on demand rather than
  during a refresh — it is the expensive question, and nobody asks it of a
  green gate.
- **Issues by role** — the ones the user opened, and the ones they are engaged
  in without having opened them. Each is a ticket by key, with title, url,
  state as the forge spells it, last-updated time, and its comments in the same
  message shape as a PR conversation. Where a forge can be searched as a whole,
  an implementation reports the user's issues wherever they live rather than
  only in a configured set of repositories: an issue filed against someone
  else's project is theirs to follow just as much as one on their own. State is
  reported whatever it is, closed included — an issue's closing is often the
  activity worth seeing.

### 2. Two transports, one interface

An implementation is reached either **in process**, as Rust implementing the
interface, or **out of process**, as an executable ephor runs. The two are the
same interface: the capability set of §1 defines a set of data types, the
in-process form passes them directly and the out-of-process form passes their
JSON serialization, and both are produced from one definition so they cannot
drift. Which transport an implementation uses is invisible above the interface —
the policy that turns forge data into feed items runs identically over both.

Out of process is the low-ceremony path and needs no Rust: an executable named
for the forge, resolved on `PATH`, answering a fixed set of subcommands with
JSON on stdout and receiving its configuration on stdin. A shell script with
`jq` is a complete implementation. In process is for implementations that want
Rust's typing; because Rust has no stable plugin ABI, an in-process
implementation outside this repository means depending on ephor as a library
and building a binary that registers it — not a dynamically loaded object.

### 3. Policy lives above the interface, never in an implementation

An implementation answers questions about a forge. It does not decide what the
answers mean. Whether a citation was answered, whether an item needs a
response, how threads and gate counts roll up, how items match registry
branches, and what counts as unread are ephor's, applied identically to every
implementation — so the feed stays coherent across forges, and an
implementation stays small enough to be a shell script.

### 4. Site-specific implementations ship separately

An implementation for a private forge — such as one reaching an internal
Bitbucket Server, Jira, and Buildbot through a vendor CLI — lives outside the
default build and is neither a build-time nor a run-time dependency of it. Its
vendor CLI name, host names, project keys, and repository names are
configuration it reads, never identifiers in ephor's source.

### 5. No site-specific data in the repository

The registry and feed configuration a person runs ephor with is their own: it
names their employer's repositories, hosts, and accounts. The repository
carries example configuration only, and a published artifact must contain
nothing else.

*Not satisfied today* — see
[§RM-001-forge-interface](docs/roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface).

### 6. A source that did not answer says so, and says which kind of not

ephor's whole purpose is to be believed when it says there is nothing to do.
An empty section therefore has to mean "nothing is waiting", never "this
source could not be read" — the two are indistinguishable on screen, and the
second one silently costs exactly the work the reader opened ephor to find.

So a provider that cannot deliver **fails explicitly**, and never substitutes
an empty or partial answer for a failure:

1. **No silent degradation, in either transport.** An implementation that
   cannot complete an answer — a fetch that failed, output it cannot parse, a
   shape it does not recognise — reports the failure. Returning the part it
   managed is not allowed where the missing part changes meaning: a pull
   request whose conversation was dropped reads as one that needs no reply.
2. **A failed capability probe is a failure, not an empty declaration.** §1
   lets an implementation decline a capability, and ephor degrades to what is
   declared. That applies to an implementation that *answered*; one that could
   not be asked has declared nothing, and is reported as broken rather than as
   a forge that does very little.
3. **Failures are visible without being looked for.** Every failed provider is
   named — with its project — on stderr and in the interactive header, and a
   run that lost any provider exits non-zero. A partial refresh reported as
   success is how a source stays dark indefinitely: whatever runs the refresh
   on a timer sees exit 0 and no one is told.
4. **An unreachable destination is its own condition.** A host that could not
   be reached at all — DNS, refused connection, no route, a VPN that is down —
   is reported as unreachable rather than as a generic failure. It asks
   nothing of the reader but a working network, where every other failure asks
   them to go and change something, and the distinction also says whether the
   items still on screen are last-good values waiting out an outage.
5. **Last-good items are labelled, never passed off as current.** Keeping the
   previous answer when a provider fails is right — one flaky source must not
   blank the feed — but those items are marked stale wherever they appear, and
   the provider that failed is reported alongside them.
6. **What is quoted is the diagnosis, not the narration.** An out-of-process
   implementation writes its diagnostics to stderr, and the tools it wraps
   narrate their progress there too — "Requesting …", "Waiting for …". ephor
   reports one line of that stream, so taking the first line by position
   reports the narration and drops the error underneath it: a message that
   says only that something was attempted, and that no reader can act on.
   The line reported is the one that reads as a diagnosis.

## FS-002-release: ephor releases from a tag, with a changelog entry per change

Versions are semver, and a version exists exactly when a `vX.Y.Z` tag does. The
version in `Cargo.toml` and the tag agree or the release refuses to run.

### 1. Changelog

[docs/changelog.md](docs/changelog.md) holds `## Unreleased` and the most
recent release inline; older releases move one-per-file under
`docs/changelog/` with a one-line pointer, so the common question — what
changed lately — is one file deep. Sections per release are the
Keep-a-Changelog set. Every pull request adds a bullet under `## Unreleased`
naming its own number, checked in CI.

### 2. Cutting a release

Promoting `## Unreleased` into a numbered release, bumping the manifest, and
tagging is done by workflow, not by hand: a patch release on a schedule when
main has shipped observable changes and its CI is green, and a minor release on
demand. Each first runs the whole release on a candidate branch with publishing
disabled, and only fast-forwards main if that dry run passed.

### 3. Artifacts

A release publishes the crate and a self-checked binary per supported target,
each built profile-guided, archived with its `sha256`, and attached to a GitHub
release whose notes are the changelog section for that version. Re-running a
partially-failed release skips what already exists rather than failing.

### 4. Publication is gated on carrying nothing site-specific

No artifact is published while the tree still violates
[§FS-001-forge-interface.5](#5-no-site-specific-data-in-the-repository). The
check is mechanical and runs before anything is uploaded.

## FS-003-feed-categories: the feed sorts itself into categories, and finished work lands in Recent

A feed is read by scanning, not by searching, so items arrive already sorted
into the categories a person works in. The categories are ephor's, never a
provider's — a provider reports items and ephor places them
([§FS-001-forge-interface.3](#3-policy-lives-above-the-interface-never-in-an-implementation))
— so every forge lands in the same categories and a new implementation
inherits them without asking.

### 1. The categories

An item belongs to exactly one category, chosen by its kind and by the user's
role on it:

| Category | Holds |
| --- | --- |
| Status | project status lines |
| My Pull Requests | pull requests the user authored |
| Reviewing | pull requests the user is on as a reviewer |
| CI | gate and build results |
| My Issues | issues the user opened |
| Participating | issues the user is in but did not open |
| Messages | conversations that are not attached to a pull request or issue |
| Recent | finished items — see [§2](#2-recent) |

Exactly one, so that the size of a category is the size of that pile of work
and not a double count.

### 2. Recent

Work does not stop mattering the moment it is finished. An item whose state is
terminal — closed, merged, done, resolved, declined, however its forge spells
it — leaves its category and appears under **Recent** for as long as its last
activity falls inside the recency window; past that it leaves the feed
entirely. Being closed is itself activity: an issue closed with no reply shows
up under Recent precisely because closing it was the answer.

Finished work never awaits a response. Whatever its conversation looks like —
someone else had the last word, the user was named and never answered — a
finished item is news and not a task, and nothing that counts work left to do
counts it.

### 3. The recency window is configured

How long finished work stays interesting is a property of a person, not of
ephor: the window is `defaults.recent_days` in the feed configuration, in
days, defaulting to 7. Zero drops an item from the feed the moment it is
finished, which is the behavior for someone who never looks back.

## FS-004-quick-actions: a problem ephor recognizes arrives with the action for it

The action menu starts empty: every entry in it is something the reader wrote
down first. Yet what a person wants on a problem is nearly always the same
thing — the failing log, the diff, the conversation — and making them
configure that is ephor knowing what is wrong and sending them to fetch it
anyway. The reader would have to leave, find the repository, remember the
tool's flags, and come back with an answer ephor could have handed them.

So ephor offers those itself. A **quick action** is a menu entry ephor has
without being told, on an item where it already knows what the problem is.

### 1. A quick action belongs to the source that found the problem

The source that produced an item is the one place that knows which forge it
came from and which tool reaches it — and naming a forge or a vendor CLI
anywhere above that is what
[§FS-001-forge-interface](#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
forbids. A quick action is therefore offered by the source; ephor's core only
merges it into the menu and runs it, exactly as it runs a configured action —
the same checkout resolution, the same `EPHOR_*` environment, the same
handover of the terminal while it runs. A quick action is an ordinary menu
entry that nobody had to write, and a source that offers none is complete.

### 2. Offered only where it would work

A quick action appears only when running it would do something: the item has
the problem the action addresses, the identifiers the command needs are
known, and the tool it runs is installed. A menu that lists an action which
cannot work is worse than one that lists nothing, because the reader believes
it and spends a keystroke and a screen of errors finding out.

### 3. Quick actions come first, and configuration adds to them

They are listed above the configured actions, so that the obvious thing is
the first key. Configuration never replaces them: a reader whose own action
does the same job gets both, because ephor cannot tell that two commands mean
the same thing and silently dropping either one is the failure that matters.

### 4. Failing CI answers what failed and why

The quick action on a pull request whose gate is red shows what failed: the
check list as the forge reports it — which failed, which passed, which are
still running — and then the failures themselves, each with its log, paged.
That is the whole question a red gate asks, and reaching it by hand is several
commands and a browser tab.

The condition is the red gate, not the source that reported it. Every item
carrying a failing gate is offered the action, whichever source produced it,
and a source that cannot say what failed offers nothing. Hanging the action off
one kind of item instead leaves the reader looking at a number with nothing
behind it on every forge that reports its gate on the pull request itself —
which is most of them, since a gate is a property of the change, not a separate
piece of work.

Where ephor renders the failures itself rather than handing over a log,
identical ones are reported once, with the number of jobs that hit them. A gate
fans one error across every job that compiled the same file, and six copies of
one compile error is a worse answer than one copy that says six.

## FS-005-dispatch: what ephor watches, it can hand to an agent runtime

A watch that only watches hands its reader a list. Nearly every row on that
list has an obvious next move — the gate is red so the failures need reading
and fixing, a reviewer asked a question so it needs answering, an issue was
filed so it needs doing — and every one of those moves is the same shape: read
a change in a checkout, do something small, say what was done. That is work an
agent can be asked to do, and asking it is the boring half of the day.

ephor does not do that work. It **dispatches** it: it turns an item into a
ticket in an agent runtime, hands over what it already knows, and then keeps
the ledger — which items have work under way, what that work reached, and
whether the item has moved since. Watching and working are one loop, and ephor
is the half that remembers.

The runtime is [rhei](https://github.com/vjovanov/rhei), named here rather than
hidden behind an interface. That is not the neutrality
[§FS-001-forge-interface](#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
asks for: a forge is a property of a person's employer and may not be named,
where the runtime that executes their work is a property of how they choose to
work. What ephor writes is a plan file in a documented plain-text language, so
the coupling is a file format and not a process — the tickets stay readable,
diffable, and hand-editable if the runtime is never run at all.

### 1. A recipe decides which items deserve work, and what to ask for

A **recipe** is a named piece of configuration with a selector and a brief: the
selector says which items it applies to — kind, role, whether the gate is red,
whether a response is owed, which source reported it — and the brief is what
the ticket asks for, in the reader's own words.

Recipes are how the same watch serves different projects: what to do about a
red gate in one repository is not what to do about it in another, and neither
is ephor's to decide. A recipe is therefore configuration first. ephor ships
the few that are true everywhere — the red gate, the unanswered conversation,
the review, the issue — for the same reason it ships quick actions
([§FS-004-quick-actions](#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)):
a problem ephor already recognizes should not need to be described to it
before anything can be done about it. Configuration adds recipes, and a
configured recipe that reuses a shipped one's name replaces it.

### 2. The ticket carries what ephor knows, not a link to it

A ticket that says "look at pull request 42" has handed back the whole job. The
watch already holds what the work needs: the title and state, the branch and
the checkout it lives in, the gate's counts per repository and the forge's own
reasons for refusing the merge, and the conversation as messages with their
authors and times. All of it was fetched already, and it is on disk.

So the ticket carries it — a **dossier** written into the plan, and the ask
written under it. Two things follow. The work starts from what a person would
have read first, instead of spending its opening move re-fetching what ephor
had. And the dossier is a record: it says what the item looked like when the
work was asked for, which is the only way to read the result of that work
later.

A dossier is bounded. A conversation of two hundred messages is not evidence,
it is a transcript; what is quoted is bounded per thread and in total, and
where anything was dropped the ticket says so and links to the whole.

### 3. One rhei per item, one ticket per dispatch

An item's work lives in one plan named after the item — so its whole history is
one file, and dispatching a second recipe on the same item adds a ticket to it
rather than starting a rival copy of the same work somewhere else.

The plan is created in the project the item belongs to, in the checkout the
item's branch resolves to — the same resolution actions already use
([§FS-004-quick-actions.1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)).
Work about a branch belongs in that branch's working tree: it is where the
change is, where the tools run, and where the runtime will put the agent. Where
the branch is not checked out, dispatch says so and offers the checkout,
because writing a ticket about code that is not on the machine only moves the
problem.

### 4. The ledger is ephor's record, and never the truth about the work

ephor keeps a ledger of what it dispatched: the item, the recipe, the plan, and
what the item looked like at that moment. The ledger is what makes the second
question answerable — has this already been handed over? — and it is written
where ephor's other state lives, not in the reader's repositories.

But the work's state belongs to the runtime and is read from the plan, never
cached in the ledger. A ledger that remembers "running" when the plan says
"done" is worse than no ledger: it is a watch reporting on itself instead of on
the world, which is the one thing this tool must never do. A ledger entry whose
plan has been deleted is reported as missing rather than repaired.

### 5. An item that moved reopens its work

Work asked about a pull request is answered against the pull request as it was.
New comments arrive; the gate turns red again; the state changes. The ticket
that was finished is now finished about something that no longer exists.

So ephor **fingerprints** the item at dispatch — its last activity, its state,
its gate, how much conversation it had — and a change to any of those makes the
work **stale**. Stale work is reopened by appending a ticket to the same plan
that says what changed since the last one and asks for the difference, ordered
after it. Not by opening a second plan: the point of the record is that one
item's work reads in one place, in order.

What is asked for is chosen against the item as it now is, preferring what was
asked last while that still applies. A change moves between categories as it
goes: the pull request whose gate was red is, two hours later, one whose jobs
pass and whose reviewer has asked a question. Reopening it under the recipe it
was first dispatched with would hand the work a ticket about a problem that is
no longer there. Where nothing applies any more — it merged, it closed — the
work is not reopened at all, and the ledger goes on saying that the item moved
past it.

Reopening is a decision, not a reflex. It is offered where it applies and
performed when asked for — by a person or by whatever runs the sync — and never
as a side effect of merely looking at the feed.

### 6. Dispatch is offered where it would work, and refuses where it would not

The rules of [§FS-004-quick-actions.2](#2-offered-only-where-it-would-work)
hold here and cost more when broken, because a ticket that cannot run is not a
wasted keystroke but a piece of work that looks scheduled and never happens. A
recipe is offered only when it matches the item, the item's project has a root,
and the checkout can be resolved — and where the work edits the change rather
than reading it, only when that change is actually on the machine. Where the
runtime's setup in that checkout cannot run what ephor would write — a state
machine already there that does not declare the state a recipe starts in —
dispatch refuses and names both, rather than writing a ticket that will sit
there unrunnable. It refuses the mirror image too: where the reader's own plans
are already in that directory under no declared machine, ephor does not install
one, because a state machine governs every plan in a project and theirs were
there first.

Matching is on what a gate is doing, not on how red it looks. Jobs that failed
are work for a checkout; a forge that refuses to merge an otherwise green
change is usually waiting on a person, and dispatching an agent at it spends a
pass to be told so.

Finished work is never dispatched. An item under Recent
([§FS-003-feed-categories.2](#2-recent)) is news, and asking an agent to fix a
merged pull request is asking it to invent something to do.

### 7. Handing over work is the reader's move, and stays inside the machine

Dispatch writes files and nothing else. It opens no pull request, posts no
comment, and pushes no branch — those are the runtime's to do, if a recipe asks
for them, and a recipe that does asks in the ticket's own words where a reader
can see it. What ships asks for none of them: the shipped recipes end at a
local change, and closing the loop out to the forge is one line of
configuration that a person turns on deliberately.

Bulk dispatch — every matching item in a project, in one command — is the same
guarantee at scale: it writes tickets, reports each one, and can be asked what
it would do without doing it.

### 8. What ephor offers is not a limit on what can be asked

Recipes are for the work that repeats. Most work does not: a reader looks at a
change and knows the one thing they want done to it, and that thing has never
come up before and will not come up again. A tool where every ask must first be
written down as a rule, in a configuration file, in another window, has made
the common case the expensive one — and the reader will do it by hand instead,
which is what they were trying to stop doing.

So an item can be asked for **anything, in the reader's own words, where they
are standing**. What that produces is an ordinary ticket: the same dossier, the
same plan, the same place in the order, the same runtime. Only the brief is
different, in that nobody wrote it in advance.

Two things follow from its being asked for rather than offered:

1. **It is never refused for not matching.** Selectors say what ephor
   *volunteers*; they say nothing about what a person may ask for. Finished
   work, an item no recipe covers, a second ask on work already under way — each
   is somebody's deliberate request, and ephor's job is to write it down
   accurately rather than to have an opinion about it.
2. **The same holds for a command.** The action menu
   ([§FS-004-quick-actions](#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it))
   is configuration plus what a source offers; a reader who wants to run
   something once should not have to add it to a file first. A command typed
   into the menu runs exactly as a configured one does — the same checkout, the
   same `EPHOR_*` environment, the same handover of the terminal — because the
   only difference between the two is whether anyone expects to want it again.
