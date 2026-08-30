# Requirements

Declare each behavior or requirement inline as an H2: `## FS-NNN-slug: …`.

## FS-001-forge-interface: ephor reaches every forge and issue tracker through one provider interface

ephor aggregates work from places that host code review and places that track
issues. Which ones they are is a property of a person's employer, not of ephor.
No forge, tracker, or vendor CLI may therefore be named in ephor's core
(§REQ-001-boundary.5): every one of them is reached through a single interface
with a fixed capability set, and an implementation is selected per project by
configuration.

### 1. Capabilities

An implementation answers some or all of the following; it declares which, and
ephor degrades to what is answered rather than failing.

- **Pull requests by role** — the ones the user authored, and the ones they are
  on as a reviewer. Each carries a stable id, title, url, state, head branch,
  last-updated time, and the reasons it is theirs. A reason is one of: they
  opened it, they are engaged in a thread on it, they are cited in it, a review
  was asked of them, or it is assigned to them. Being *asked* and being
  *engaged* are not the same thing, and the quiet one is the one that matters:
  a review request nobody has answered leaves no trace in the conversation, so
  an implementation that reports only the pull requests the user has already
  spoken in reports the ones they have handled and drops the ones they have
  not. Where a forge can be searched as a whole, an implementation reports the
  user's pull requests wherever they live rather than only in a configured set
  of repositories, on the same terms as issues below. State is reported
  whatever it is, closed and merged included: a question asked of the user does
  not stop being asked when the branch lands.
- **Own review** — on a pull request the user is on as a reviewer, the review
  *they* gave: approved, changes requested, or reviewed with no verdict either
  way. Nothing else reported here implies it. A reviewer list says who was
  asked, a conversation says who spoke, and neither says who answered: an
  approval leaves no message behind, and a reviewer who commented at length has
  still not approved — so a reviewing row that shows only `open` cannot tell a
  change the reader has dealt with from one they have not, which is the same
  gap the reasons above exist to close, seen from the other side. Absent where
  the user has not reviewed, and absent for their own pull requests, since an
  author does not review their own change. A verdict this vocabulary has no
  word for — an approval the forge dismissed, a review still in draft — is
  reported as no review, which is what it means to the reader. What a verdict
  *retires* is policy's
  ([§3](#3-policy-lives-above-the-interface-never-in-an-implementation)): an
  implementation says what the user did, never what is left of it. It is what a
  reviewing row leads with, except where a review is being asked for again —
  a re-request is the forge saying the old verdict is no longer the answer.
- **Conversation** — a PR's threads as ordered messages (author, text, time),
  each with its reactions, and enough identity for a reaction to be posted back
  to it. A message the forge tracks as a task carries that too: its state, and
  enough identity to transition it.
- **Reactions** — post a reaction on a message, where the implementation
  supports writing; read-only implementations say so and their messages are
  display-only.
- **Tasks** — resolve a task the forge tracks on a message. Forges spell it
  differently — a checklist item, a blocker comment, a review task — but they
  agree on the shape: a message that is done or not done, and a person who
  says which. An implementation that can only read task state declares no
  tasks capability; its tasks still render with their state, since knowing a
  box is unticked is most of the value even where ephor cannot tick it.
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
- **Restart** — run a pull request's gate again, at a stated scope: everything
  it covers, or only what is not green
  ([§FS-004-quick-actions.9](#9-a-gate-is-offered-the-restart-in-two-shapes)).
  The one capability here that spends somebody else's machines, which is why
  the scope is the caller's word and never the implementation's guess: the
  cheap ask and the expensive one are different asks and a forge that widened
  the first into the second would be answering a question nobody put. It
  answers what it actually asked for — how many jobs where it counts them, its
  own sentence where it does not, and what it could not restart — because a
  gate is minutes away from saying anything itself, and a restart that reported
  only *done* is indistinguishable from one that found nothing to do. Not
  counting is an ordinary answer and not a broken one: a whole-gate start is
  accepted and executed elsewhere, and how much it scheduled is knowable only
  from the gate. What must not happen is that answer arriving as *nothing*. An
  implementation whose forge cannot re-run a check does not declare it, and
  ephor offers nothing rather than a key that fails.
- **Issues by role** — the ones the user opened, and the ones they are engaged
  in without having opened them. Each is a ticket by key, with title, url,
  state as the forge spells it, last-updated time, and its comments in the same
  message shape as a PR conversation. Where a forge can be searched as a whole,
  an implementation reports the user's issues wherever they live rather than
  only in a configured set of repositories: an issue filed against someone
  else's project is theirs to follow just as much as one on their own. State is
  reported whatever it is, closed included — an issue's closing is often the
  activity worth seeing. Each also reports **whether anyone has taken it**,
  where the forge tracks that: an issue nobody has picked up is the plainest
  case of work with no owner, and it is a fact only the forge holds — a
  conversation cannot say it, and an issue filed and never answered looks
  exactly like one that is finished
  ([§FS-003-feed-categories.4](#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).
  An implementation with no notion of assignment omits it, and nothing it
  reports is ever counted as unclaimed.

  Where a source is configured to **follow a label**, an implementation also
  reports the open issues carrying it, whoever is in them. A label is the
  project's own word for what an issue is — a priority, a class of work — and
  on a project the reader answers for, an issue so labelled is theirs to
  follow whether or not they have ever spoken in it; the searches by role
  cannot reach it, since being nobody in an issue is precisely what they
  filter out. Each such issue is reported under the role the reader actually
  holds on it — author where they opened it, participant otherwise — so it
  lands where its kind lands
  ([§FS-003-feed-categories.1](#1-the-categories)). Only what is open is asked
  for: following a label is following work, and a search that took the closed
  too would spend its bound on history rather than on the queue. And a label
  search that comes back as full as it was allowed to be has not answered
  ([§6](#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)) —
  it delivered a prefix nobody can size — so an implementation fails there
  rather than showing a fraction of a queue as if it were the queue.
- **Notices** — what the forge itself says is directed at the user: one entry
  per thing it decided to tell them, carrying the reason it gives, the subject
  it concerns, when it arrived, and whether the forge considers it read. This
  is the completeness capability, and the only one whose job is to be
  exhaustive rather than exact. Every other capability answers a question ephor
  composed, and so can only return what ephor knew to ask about: the
  repositories it was configured with, the roles it thought to search, the
  kinds of thing it models. A forge that keeps a notification list of its own —
  GitHub's notifications, GitLab's todos — has already made that judgement
  across everything it hosts, including the kinds ephor has no capability for
  at all. Without it, [§6](#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)
  is a promise ephor cannot keep: an empty feed would mean "nothing is waiting
  in what I was told to look at", which is a different and much weaker claim
  than the one an empty feed makes.

  A notice about something another capability already reported is the same work
  seen twice, not two pieces of work; reconciling them is policy's job, under
  [§3](#3-policy-lives-above-the-interface-never-in-an-implementation) and
  [§FS-003-feed-categories.5](#5-one-subject-is-one-row-however-many-sources-reported-it).

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
response, which reported reason puts the user on which side of a pull request,
which of two reports of the same subject the reader sees, how threads and gate
counts roll up, how items match registry branches, and what counts as unread
are ephor's, applied identically to every implementation — so the feed stays
coherent across forges, and an implementation stays small enough to be a shell
script.

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

### 7. A fetch runs beneath the reading, never in front of it

Asking every source takes as long as the slowest of them, and that is not a
number ephor controls: an out-of-process forge reached over a VPN is allowed a
ceiling of its own (§2) precisely because the shared default is too short for
it. So the fetch is the slowest thing ephor does — minutes, where a source is
entitled to them — and the reader is doing something else while it runs:
scanning, opening a thread, marking work done.

An interactive refresh therefore runs **beneath** the interface. The screen
stays the reader's for the whole of it: every key still answers, and they may
read, act on a matter, and leave while sources are still being asked. A screen
frozen on a fetch is the failure §FS-010-doctor.3 names in the diagnostic — a
tool that shows nothing until it is finished is one a person kills half way
through and reports as hung — except that here they cannot kill it either,
since the key that would quit is the one not being read.

Three things follow:

1. **Answers land as they arrive.** A project whose sources have answered
   takes its place in the feed then, not when the last project is done. One
   slow forge otherwise holds back every fast one's news, and the reader waits
   on the worst source for all of them.
2. **A run in flight says so, and says where it has got to.** A screen that
   stays live is also a screen that looks finished, and a reader who cannot
   tell a running refresh from a completed one reads a half-filled feed as the
   whole answer — §6's failure arriving by another road, an empty section that
   means "not asked yet". So the header names the run and its progress while
   it is in flight, and the reader is told what it lost when it ends. Where a
   screen collects every operation in one place (§FS-005-dispatch.15), the
   run appears there *additionally* — the header line stays where it is,
   because this point is about progress on the screen being read, and the
   reader entitled to it is the one who never visits the board.
3. **The reader's place is kept.** Rows arriving under a moving cursor must
   not change what the next key would act on: a selection follows the matter
   it was on rather than the position that matter happened to occupy.

None of this changes what a refresh costs the forge (§GOAL-005-costless):
moving the waiting off the reader's screen is not licence to ask more sources
at once than were being asked before it.

### 8. A refresh is asked in the cheapest form the forge offers

A forge meters what it will answer, and not evenly: the endpoint that searches
is usually the scarcest thing it has, metered per minute where the ordinary API
is metered per hour. A refresh that spends one search per role, per repository,
per project therefore scales its cost by three numbers the reader never chose,
and crosses that ceiling long before a registry looks large.

What crossing it does is §6's failure arriving where §6 cannot see it. The forge
does not refuse the refresh, it refuses the tail of it: the sources asked first
answer, the ones asked last come back refused, and which is which depends only
on the order the run happened to take. Every refused source does say so, and the
reader is still misled — the same watch reports a different set of projects each
run, for a reason that is nowhere in the feed and nothing to do with their work.
A watch that is quietly short is worse than one that is plainly down
(§GOAL-003-nothing-lost).

So the cost of a refresh is bounded by the question being asked, not by the
shape of the loop that asks it:

#### 8.1 A role is not a request

Where a forge answers about several roles at once, it is asked once. The
reasons are still reported one by one
([§1](#1-capabilities)) — what collapses is the asking, never the answer. An
implementation that cannot recover the separate reasons from a combined answer
asks the separate questions instead: a reason is a claim about the reader's
involvement, and one that was not established is not reported.

#### 8.2 The scarce meter is the last resort

Where the same question can be put to an endpoint the forge meters more
generously, it goes there. Two forms of one answer that differ only in which
meter they draw on are not a tradeoff, and spending the scarce one buys
nothing.

#### 8.3 What is already in hand is not asked for again

A field that arrives with the answer is not re-fetched per result. The
per-result follow-up is where a refresh's cost stops being proportional to the
registry and starts being proportional to the reader's own work — the one
direction that punishes exactly the busiest reader.

None of this licenses asking for more than is needed: the cheap form of a
question is still only asked because the answer is read.

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

**Self-checked means the binary did its job, not that it started.** What every
artifact is held to is `doctor`'s self pass (§FS-010-doctor.3): it builds a
project of its own and walks the seams against it, so a binary that links,
prints its version and cannot refresh a source is caught here rather than by
the first person to install it. A check that only asks for `--version` tests
the argument parser.

**And it is what the profile is trained on.** A profile-guided build is only
as good as the workload it profiled, so the training run is the same self
pass rather than a list of commands assembled for the occasion — one workload,
exercising the paths a reader actually waits on. It must also be hermetic: a
training run that reads the registry of whoever is building reads a private
site on one machine and finds nothing on a build runner
(§FS-001-forge-interface.5), and a profile gathered from commands that all
exited early is no profile at all.

### 4. Publication is gated on carrying nothing site-specific

No artifact is published while the tree still violates
[§FS-001-forge-interface.5](#5-no-site-specific-data-in-the-repository) or the
literal confinement of §REQ-001-boundary.5. The checks are mechanical and run
before anything is uploaded.

## FS-003-feed-categories: the feed sorts itself into categories, and finished work lands in Recent

A feed is read by scanning, not by searching, so items arrive already sorted
into the categories a person works in. The categories are ephor's, never a
provider's — a provider reports items and ephor places them
([§FS-001-forge-interface.3](#3-policy-lives-above-the-interface-never-in-an-implementation)),
policy staying on ephor's side of the seam (§REQ-001-boundary) — so every
forge lands in the same categories and a new implementation inherits them
without asking.

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
| Participating | issues the user is in but did not open, or follows by a label the source names (§FS-001-forge-interface.1) |
| Tasks | the project's own tasks, from a store in its checkout (§FS-006-project-interface.7) |
| Messages | anything addressed to the user that is not a pull request or an issue |
| Recent | finished work that still leaves something to do — see [§2](#2-recent) |

Exactly one, so that the size of a category is the size of that pile of work
and not a double count.

### 2. Recent

Work does not stop mattering the moment it is finished, but most of it stops
asking for anything. An item whose state is terminal — closed, merged, done,
resolved, declined, however its forge spells it — leaves its category, and it
appears under **Recent** only while it still leaves the reader something to do.
There are three such things, and they are the whole list:

- **An answer is missing** — whatever would have made the subject await one
  while it was still open
  ([§4](#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)):
  somebody else had the last word, a task box on it is unticked, a notice named
  the reader. The comment that lands on the way out, or after it, is the case
  this exists for.
- **The gate is red** — the run that went the other way after the merge. A red
  gate is a thing to look at whatever the state beside it says, and the action
  that reads its log is offered on the row it is on
  ([§FS-004-quick-actions.4](#4-failing-ci-answers-what-failed-and-why)).
- **Work is still open on it** — the runtime holds a ticket about this matter.
  Work stands on rows beneath the matter (§FS-005-dispatch.23), so a matter
  that leaves the feed takes the work rows with it, and a run nobody can see is
  a run nobody can take back.

A finished item with none of the three leaves the feed the moment it finishes,
whatever the recency window would still allow. Nothing is lost by that: it
merged, which is what it was for, and a merge that went as asked is not news
anybody has to clear. Listing it anyway makes Recent a list of things nobody
will do anything about — read every day, at the cost of the rows that need
doing (§GOAL-002-glance).

The recency window bounds the two the report knows: a missing answer and a red
gate are worth showing only while the item's own last activity falls inside it
([§3](#3-the-recency-window-is-configured)). Open work is not bounded by it. A
run in flight is not history, however long ago the matter it was asked about
last moved, and the ledger — not the forge — is what says it is still going.

**A loose end is not a response owed.** Finished work never awaits one:
whatever its conversation looks like — someone else had the last word, the user
was named and never answered — a finished item is news and not a task, and
nothing that counts work left to do counts it. What the loose end decides is
whether the news is worth putting in front of anybody, never that somebody owes
an answer.

### 3. The recency window is configured

How long finished work stays interesting is a property of a person, not of
ephor: the window is `defaults.recent_days` in the feed configuration, in
days, defaulting to 7. Zero drops an item from the feed the moment it is
finished, which is the behavior for someone who never looks back.

### 4. A conversation is answered in whatever form the forge recorded it

A thread awaits the user while the answer it is waiting for is missing, and an
answer takes more than one form. Having the last word is one. A reaction on the
last message is another — often it is the honest answer, and demanding prose
where a forge offers a button would leave the thread pending forever.

A task the forge tracks is a third, and it outranks the other two: a thread
holding an unresolved task awaits the user however the conversation ended, and
a thread whose last word is a resolved task is answered even though a robot
spoke it. Task state is the forge's own record of whether the thing was done,
which is exactly the question, and reading who spoke last instead leaves every
bot checklist counted as work forever — the reader's own inbox tells them the
box is unticked long after they ticked it.

**An issue nobody has taken is a fourth, and it is not a conversation at all.**
Where a source is configured to say so, an unclaimed issue
([§FS-001-forge-interface.1](#1-capabilities)) awaits somebody however the
conversation ended — including when it never started. The three forms above
all read the talk, and the talk is silent here in the most misleading way
available: an issue somebody filed and nobody picked up has its author's word
last, so the rule that serves every other case reports it as answered. It is
the same shape as a review asked for and never given
([§FS-001-forge-interface.1](#1-capabilities)) — being waited on leaves no
message behind.

Whether it applies is the source's to say, because *unclaimed* only means
*yours* where the reader is answerable for the backlog. On a project they run
it is the whole point; among issues they merely commented on somewhere it
would turn every stranger's open bug into their work. So it is configuration
on the source rather than a rule everywhere, and off unless asked for.

### 5. One subject is one row, however many sources reported it

Sources overlap on purpose. A source that searches by role and a source that
reads the forge's own notice list
([§FS-001-forge-interface.1](#1-capabilities)) are asking different questions
that land on the same pull request, and the overlap is the point: it is how
ephor can be exhaustive without being told in advance where to look. What the
reader must never see is the consequence — the same pull request twice, in two
rows, counted twice in the size of the pile that [§1](#1-the-categories) exists
to make readable.

So a subject reported by several sources is one item. Which report survives is
decided by how much it says, not by which source was configured first: the one
carrying the conversation, the gate, and the role outranks the one carrying a
title and a reason, because the reader can act on the first and can only click
through on the second. What the losing report knew and the winner did not — a
reason the forge gave for telling the user about it — is carried over rather
than discarded, since that reason is often the only thing that explains why the
row is there at all.

Merging is by subject identity as the forge states it, never by title text: two
pull requests may share a title, and a subject whose identity cannot be
established is left alone rather than guessed at. A duplicate shown is a small
insult to the reader; a distinct piece of work silently swallowed is the
failure [§FS-001-forge-interface.6](#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)
is about.

## FS-004-quick-actions: a problem ephor recognizes arrives with the action for it

The action menu starts empty: every entry in it is something the reader wrote
down first. Yet what a person wants on a problem is nearly always the same
thing — the failing log, the diff, the conversation — and making them
configure that is ephor knowing what is wrong and sending them to fetch it
anyway. The reader would have to leave, find the repository, remember the
tool's flags, and come back with an answer ephor could have handed them.

So ephor offers those itself. A **quick action** is a menu entry ephor has
without being told, on an item where it already knows what the problem is —
the most frequent response made the cheapest one (§GOAL-001-fewest-moves).

### 1. A quick action belongs to the source that found the problem

The source that produced an item is the one place that knows which forge it
came from and which tool reaches it — and naming a forge or a vendor CLI
anywhere above that is what
[§FS-001-forge-interface](#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
forbids. A quick action is therefore offered by the source; ephor's core only
merges it into the menu and runs it, exactly as it runs a configured action —
the same checkout resolution, the same `EPHOR_*` environment, the same
handover of the terminal while it runs, one crossing in the seam's materials
(§REQ-001-boundary.1). A quick action is an ordinary menu
entry that nobody had to write, and a source that offers none is complete.

### 2. Offered only where it would work

A quick action appears only when running it would do something: the item has
the problem the action addresses, the identifiers the command needs are
known, and the tool it runs is installed. A menu that lists an action which
cannot work is worse than one that lists nothing, because the reader believes
it and spends a keystroke and a screen of errors finding out.

A key a screen advertises is under the same rule, and it is measured against
what the key would act on rather than against the screen. Message keys are
offered on the message the reader has selected: the key to react appears where
the forge would accept a reaction, the key to tick a task where there is an
unresolved task to tick. A footer that offers the same keys everywhere teaches
the reader a key that does nothing on most of what they select, and the answer
they get for pressing it — a refusal in one line at the bottom of a full
screen — is the one place they were not looking.

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

### 5. A task is ticked where it is read

A task renders as what it is — a box, ticked or not — beside the message that
carries it, and the reader ticks it from there. The whole of that interaction
is reading the sentence and agreeing with it, so sending the reader to a
browser to click the same box is the trip this section exists to save.

Ticking goes back through the source that reported the task, like every other
write ([§1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)) —
ephor knows a task has a state and a way to transition it, and nothing about
how that forge spells either. A forge that reports task state without offering
to write it renders its boxes and offers no key, which is
[§2](#2-offered-only-where-it-would-work) and not a degraded mode.

A ticked box is an answer ([§FS-003-feed-categories.4](#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)),
so the thread stops awaiting the reader as soon as the forge accepts the
transition, without waiting for the next refresh to say so.

### 6. A branch that trails its main branch is offered the rebase

ephor already measures how far every checked-out branch has fallen behind its
project's main branch, and says so on the branch row — `3 behind`. Then it
leaves the reader to go elsewhere and do something about it. Knowing what is
wrong and not offering the move is the whole of what this section exists to
stop, and here ephor is not even relying on a forge to tell it: the fact is on
disk, in the reader's own checkout.

The fact on disk is only as fresh as the last fetch. Nothing under the watch
fetches — reading a row costs a handful of local reads and runs nothing — so
every distance stated here is measured against the copy of the base this
machine last pulled down, and that copy can be weeks old. Two things follow,
and together they are the shape of the offer.

So **a checkout that trails its project's main branch is offered the rebase
onto that branch — and so is one that measured level**. The reading that says
*level* is exactly the reading the replay would refresh: replaying onto main
begins by fetching ([§FS-005-dispatch.12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)),
so a branch that went stale without this machine hearing about it is the branch
the offer is needed on most, and withholding the entry there hides the one move
that would correct the reading. A branch genuinely level replays onto nothing
at no cost, and is told so in the register a current repository is always told
it in ([§2](#2-offered-only-where-it-would-work)) — a cheaper thing to be wrong
about than a branch nineteen days behind being shown as current. What the offer
needs is a base to name, not a distance to it.

Which base this is about has to be said now that there are two: this one
replays onto the branch the project declares as its main, and it is offered
only where the project declares one — an entry has to name what it is about to
replay onto, and where nothing names a main branch there is no answer to put in
it. The other rebase
([§8](#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it))
resolves its ref inside each repository and so needs no base named anywhere:
the two are gated apart, and a project that declares no main branch is still
offered the replay onto its own published copy.

What carries the offer is a branch on disk, not the kind of row that mentions
it. **Any item that resolves to a branch workspace** is offered it — a pull request, an issue, a status a source filed about the same
change — because the fact being acted on belongs to the checkout, and the item
is only how the reader arrived at it. Restricting it to pull requests said that
a branch is stale only when a forge has an opinion about it, which is the
inverse of this section's whole argument: the fact is on disk.

**And the branch rows carry it themselves**, with no item behind them at all.
The row saying `13 behind` is where a reader looking at a stale branch is
actually standing, and an offer reachable only by first finding something a
source filed about that branch is an offer most readers never reach — including
on every branch nothing has been filed about yet.

**And every statement of the distance says how fresh it is.** The row and the
entry alike read `13 behind as of Jul 28` — the count, and the day the local
copy of the base last moved — because `13 behind` on its own is a claim about
now that nothing here is in a position to make. A branch that measured level
says the same thing the same way: `level as of Jul 28`. *Up to date* is retired
wherever it stood, on rows and in entries both: it announced that the branch was
current when all that was measured was that it matched a copy fetched at some
unstated time, and it was the wording that made a nineteen-day-old reading look
like news.

The day is the base's own: the last time the local copy of the base moved on
this disk, which is the last fetch that brought anything down for it — read
from that copy's ref, never from a record of fetches having been attempted. So
it never over-claims. A fetch that found nothing new leaves the copy, and the
day, where they were; a fetch that failed to connect leaves both untouched
rather than stamping today onto a comparison it never refreshed. It is measured
per repository, so a checkout of several reports the oldest of them — a
comparison is only as fresh as its stalest half. And where a measured
repository has no such day at all, which is what a fresh clone that has never
fetched looks like, there is no day to report and the qualifier is left off
entirely: the row reads `13 behind`, and nothing is invented to fill the gap.

Offered only where it would work ([§2](#2-offered-only-where-it-would-work)):
something that resolves to no branch has nowhere to rebase, and a workspace
that is not there is a checkout question
([§7](#7-a-workspace-that-is-not-there-is-offered-the-checkout)) rather than a
rebase one. A branch measured level is no longer one of these refusals, but a
branch nothing could be measured on — no base named, or no ref to compare with
— still is: the entry would have nothing to name. Where a branch cannot be
resolved to a checkout on disk the offer is withheld rather than made and left
to fail on the keystroke.

It is git and nothing else. Fetch, replay the branch on the base, say what
moved — no forge, no vendor CLI, and no knowledge of what the project is built
with. A poly-repo workspace is several repositories sharing one branch name, so
every repository under the checkout is rebased and the answer is given per
repository, one already current being reported as current rather than silently
skipped.

Two things it will not do. It does not stash: a rebase that quietly pockets
uncommitted work and replays it is a good trick right up until it conflicts,
and the reader is then holding a conflict in a change they had not finished
writing — a repository with uncommitted work is reported and left alone. And it
does not decide. A rebase that stops in a conflict has arrived at a question
about the code, which is
[§FS-005-dispatch.12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model).

### 7. A workspace that is not there is offered the checkout

A branch ephor watches, on a project whose checkouts are one per branch, is
either on disk or it is not — and ephor is the thing that knows which, because
it computes the directory from the project's own template and looks. Where it
is not there, everything else stops: every action that needs a checkout is
refused, and no work can be dispatched at all, since a ticket about a change
has to run in the change.

Sending the reader to a configuration file for the command that fixes that is
the same mistake as sending them for the failing log. It is one operation, the
same on every project ephor watches, and ephor is already holding every input
it takes: which repositories the project has, where each goes under the
checkout, which branch, and what that branch is grown from. So **a missing
workspace is offered the checkout, and ephor supplies the command**. A project
that wants its own — a bare mirror, a filesystem snapshot, a `gh pr checkout` —
configures one and that wins ([§3](#3-quick-actions-come-first-and-configuration-adds-to-them)),
but nothing has to be configured for the offer to exist.

What it does is git and nothing else, and it has the rebase's shape. A
poly-repo workspace is several repositories sharing one branch name, so each
gets a working tree under the new directory: the branch itself where the forge
has it, a new branch of that name grown from the main branch where it does
not, and — where the repository already has the branch but the forge does
not — the branch as it stands, checked out and reported as published nowhere,
naming what its tracking configuration records, if anything (a tracking
configuration naming the base is where the branch was cut, not where it is
published, §DA-003-upstream-is-the-published-copy). That third case changes no
tracking configuration and pushes nothing: the reader is told the fact rather
than handed a claim. The answer is per repository, and one that was already
there is reported as already there rather than silently skipped.

Two things it will not do. It will not move a branch another working tree is
holding — git refuses that, and it is right to; the repository is reported and
left alone rather than worked around. And it will not decide where to put the
workspace: the directory is the project's template applied to the branch, the
same one every other part of ephor resolves, because a checkout that landed
somewhere else would be a checkout nothing else could find.

Like the rebase, it is one implementation for both callers
([§FS-005-dispatch.12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)):
the key the reader presses and the command a state machine runs are the same
operation, since two of them would eventually disagree about what a checked-out
workspace is.

#### 7.1 A workspace that is there is still owed its store

A directory that is there stops the offer: ephor reports the workspace as
already checked out and does nothing more. But *already checked out* answers
the question about repositories, not the one about work. A workspace made
before ephor made stores at all, or made by a project's own checkout command
([§3](#3-quick-actions-come-first-and-configuration-adds-to-them)), holds every
repository it should and still has nowhere for a plan to land — and the reader
who asks for the checkout again is asking exactly the question that would fix
it.

So the checkout makes whatever is missing and stops only where nothing is: the
repositories where those are absent, and the store
([§FS-006-project-interface.7](#7-the-projects-own-tasks-are-read-where-they-live))
in either case, since a store is part of what makes a directory a workspace
rather than a pile of repositories. Asking twice is how a half-made workspace
is repaired, not a no-op the reader has to work around.

#### 7.2 The offer is a key on the row that says the workspace is missing

The row already says *not checked out* — the matter's own row, the branch's
row, and the row of a matter nothing could place, which is where a matter whose
branch has no workspace usually is, since a branch nothing checked out is a
branch the reader has no row for. Where a fact is shown is where the move
about it belongs
([§6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)), so the
checkout is a key on that row and not only an entry in a menu opened over it.
One key, on every row that carries a branch, running the one operation above.

The menu keeps its entry — the key is a second way to the same move, not a
replacement for the first. But a menu is where a reader goes to choose among
things, and there is nothing to choose among here: a workspace that is not
there has exactly one thing that can be done about it, and the row already
says so.

### 8. A branch that trails its own published copy is offered the rebase onto it

Main moving under a branch is one thing that happens to it. The branch moving
under the reader is another: a teammate pushes to it, a second machine of their
own does, the forge writes something onto it — and the checkout on this disk is
behind the copy everybody else can see. ephor measures that distance too, per
repository, and shows it on the branch row beside the first one. Then, again, it
leaves the reader to go elsewhere and do something about it.

So **a branch whose published copy carries commits its checkout does not is
offered the rebase onto that copy**, beside the rebase onto main
([§6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)). Two
facts, two entries, two operations: one replays what the reader has onto where
the project went, the other onto where their own branch already is.

It follows [§6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)
in both of that section's consequences, for the same reason: this distance is
measured against what was last fetched too. So the entry is offered wherever
there is a copy to replay onto, whether the checkout measured behind it or
level, because the replay is what would refresh the reading — and it says how
fresh the reading is, `2 behind as of Jul 28` or `level as of Jul 28`, from the
copy's own ref rather than the base's. The two rarely last moved on the same
day: main moves when the project lands something, a branch's copy when somebody
pushes to it, and a fetch only dates the refs it actually brought down.

A branch's published copy is what was last pushed of it, read per repository
from that repository's own `HEAD` rather than from the name of the directory the
workspace sits in — a repository need not be on the branch its workspace is
named for. Where git records where the branch is published, that is the copy;
where it does not, the remote's branch of the same name is; and tracking
configuration naming the repository's base records where the branch was *cut*,
not where it is published, so it publishes nothing
(§DA-003-upstream-is-the-published-copy) — and a base nobody could resolve
cannot clear the record of naming it, so there too only a pushed copy of the
branch's own name counts. The last rule is what keeps the two
entries from being one entry twice, and the middle one is what makes the offer
worth having at all: a branch that was pushed and has no tracking configuration
is exactly what `git worktree add -b` leaves behind, and in such a checkout bare
`git rebase` refuses to run.

Offered only where it would do something
([§2](#2-offered-only-where-it-would-work)), which here is four refusals. An
item linked to no branch has nowhere to rebase, and a workspace that is not on
disk is a checkout question
([§7](#7-a-workspace-that-is-not-there-is-offered-the-checkout)). A branch
never pushed has no copy at all — nothing to name in the entry, nothing to
replay onto, and no reading a fetch would correct — and *nothing published* is
an answer, given in the same register as a repository already current and never
as a failure: the reader is told what was found, not what went wrong. And a
repository whose published copy **is** the base carries nothing of its own here
— a branch parked on the main branch and tracking it
has one distance wearing two names, and two menu entries counting one distance
is the duplication the resolution above exists to prevent — so its distance
belongs to the rebase onto main alone, and a checkout of nothing but such
repositories is offered only that. The offer stands where some repository
actually trails a copy that is not its base — one on a change's branch while
another sits parked on the base — counting those repositories alone, and the
answer says what happened to each, because a forest is not one branch.

That per-repository answer is the difference this makes to the fold. The rebase
onto main is one branch name for the whole checkout; the rebase onto the
published copy is a different ref in every repository, resolved from each
repository's own `HEAD`, and a repository that has published nothing is reported
as such while the rest replay. Everything else is the rebase onto main's and
unchanged: git and nothing else, an answer per repository, uncommitted work
reported and left alone, a conflict handed over rather than decided.

One property has to be said plainly, because it is why this is not simply
*pull*. Replaying a branch onto its own published copy rewrites commits that are
already published, so that copy can no longer be fast-forwarded and landing the
result means a force push under a lease. The rebase onto main has exactly the
same property, and the same answer: the replay itself never pushes, and the push
is a decision belonging to whoever makes it.

### 9. A gate is offered the restart, in two shapes

Reading what failed ([§4](#4-failing-ci-answers-what-failed-and-why)) answers
the question a red gate asks. It does not answer the one a reader most often
has about it, which is *was that even me*. A runner died, a mirror was
unreachable, a dependency shipped something broken, the same flake landed on
the same job for the third day: nothing about the change is wrong, and what
the change needs is another run. Knowing that and sending the reader to a
browser tab to click it is the same failure this section exists to stop, and
it is worse here than elsewhere, because the browser tab is where a restart is
usually one click away and where finding *which* thing to click takes five
minutes.

So a gate is offered the restart. It is two entries, not one, and the
difference between them is the reader's own diagnosis:

**Restart what failed.** One job died on infrastructure while everything
around it passed, and what that needs is the work back rather than a whole
gate re-run — an hour of a shared machine pool to answer a question that was
about one build. This is the ordinary case and the cheaper one.

How much cheaper is the forge's to say, not this section's. What is asked for
is *what is not green, and everything downstream of it*
(§FS-006-project-interface.6): on a forge that re-runs job by job that is the
failed jobs and nothing else, and on one that starts a gate as a whole it is
the failing part of the tree and what hangs off it, which can be most of the
tree. **The entry says which it is**, because a row promising one job over a
hundred rebuilt ones is the label lying about the cost — and a reader who
cannot tell the two apart cannot choose between the two entries at all, which
is the whole point of there being two.

**Restart everything.** The merge commit itself is suspect: the base moved
under the change, a cache was poisoned, the green results are as untrustworthy
as the red ones. Then re-running only what failed proves nothing, because the
part that would have caught it is the part that is being kept. This is the
expensive one, and it is a separate keystroke precisely so it is a decision
rather than a default.

**Where they are offered follows from what each of them means.** A red gate
gets both. A gate that is not red — green, still running, blocked on an
approval — gets *restart everything*, which is exactly the entry that still
has something to do there, and not *restart what failed*, which would be a key
that runs and reports that there was nothing to restart
([§2](#2-offered-only-where-it-would-work)). An item carrying no gate at all
gets neither: there is nothing to restart, and the fact is the item's, not the
project's.

**Restarting is the gate's verb, not ephor's idea of one.** Which command
re-runs a gate is project truth exactly as reading it is
(§FS-006-project-interface.6), and the scope — everything, or only what is not
green — crosses that seam as part of the ask. A forge that hosts its own gate
answers it as the shipped default and needs no manifest, so restarting works
on the common case with nothing configured
(§FS-001-forge-interface.1). Where the forge cannot re-run
something — an external status somebody else's system wrote — the entry says
so rather than reporting a restart that never happened.

**It runs beneath the screen** (§FS-005-dispatch.17). A restart asks nothing,
decides nothing, and is answered by the gate minutes later; taking the
interface for it would be paying the screen for a command that never needed
it, and the log is where what it asked for and what came back are kept.

**It commits nothing, and it is the same move the loop makes**
(§FS-005-dispatch.11). A reader pressing this key and a state machine deciding
the failure was never the change's are asking for the same thing, so they ask
for it the same way — one verb, one scope, one answer — and cannot drift into
two ideas of what restarting means.

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
whether the item has moved since. Watching and working are one loop, ephor is
the half that remembers, and the routine moves leave the reader's hands
(§GOAL-004-handover).

The runtime is a binding with [rhei](https://github.com/vjovanov/rhei) as the
shipped default (§REQ-001-boundary.1, decided with its tradeoff recorded in
§DA-001-runtime-bound-default). What ephor writes is a plan file in a
documented plain-text language, and that language — together with the runner
command configured to execute it and the verdict read back from its results —
is the entire coupling: a contract in files, never a linked process. Choosing
a runtime remains a property of how a person works, which is why one ships
wired and ready; requiring it would be something else. Nothing in ephor's
core names the default runner, and with no runner installed every part of
dispatch except the running still holds — tickets are written, read, and
reopened, staying readable, diffable, and hand-editable on disk — while
running refuses with the configured runner named.

### 1. A recipe decides which items deserve work, and what to ask for

A **recipe** is a named piece of configuration with a selector and a brief: the
selector says which items it applies to — kind, role, whether the gate is red,
whether a response is owed, which source reported it — and the brief is what
the ticket asks for, in the reader's own words.

Recipes are how the same watch serves different projects: what to do about a
red gate in one repository is not what to do about it in another, and neither
is ephor's to decide. A recipe is therefore configuration first. ephor ships
the few that are true everywhere — the red gate, the unanswered conversation,
the review, the issue, the branch that has fallen behind — for the same reason
it ships quick actions
([§FS-004-quick-actions](#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)):
a problem ephor already recognizes should not need to be described to it
before anything can be done about it. Configuration adds recipes, and a
configured recipe that reuses a shipped one's name replaces it.

**A recipe is an action.** The recipes and the quick actions are one menu, not
two lists behind two keys: *what can I do about this row* has one answer, and
which half of it the reader sees does not depend on which key they happened to
learn. A recipe stands among the entries a source, a project and the reader
wrote
([§FS-006-project-interface.9](#9-offers-the-projects-actions)),
selected by the same language, ordered by the same provenance, and refused in
the same sentence — marked as work to hand over and saying who would get it
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project))
before the key is pressed, because that is the difference the reader is
choosing between: an entry that runs something here, and an entry that opens a
ticket asking somebody else.

It runs both ways. An entry may carry a **brief instead of a command** — the
same selector, the same brief, the same hand — which is how a project offers
agent work of its own without writing a separate list; such an entry is a
recipe under another name, and is dispatched as one. And an entry is offered
only where it would work
([§FS-004-quick-actions.2](#2-offered-only-where-it-would-work)):
work about a change is offered where the change is on the machine, and never
about an item that is finished
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)).

Where an entry already in the menu carries a recipe's name, that recipe is
what the entry hands over when it cannot finish, not a second thing to do
about the row: the key that replays a branch and the ticket about the conflict
it stops at are one operation under one name
([§FS-004-quick-actions.6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase),
[§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)), and a
menu offering both would be asking the reader to tell two spellings of one
thing apart. Because they are one name, they are gated as one: what a recipe
applies to and what the entry that dispatches it is offered on cannot be
different sets, or the entry hands over work its own recipe says does not
apply here.

Handing work over from the menu is the same handing-over the work screen does
— one plan, one ticket, one ledger entry
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch),
[§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)) —
because where the reader pressed is not a fact about the work. And with no
runner bound the entries are still there: a ticket is written whether or not
anything can run it, and where the entry would say who gets it, it says
instead that nobody can be asked
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)).

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

A project that keeps a single checkout for every branch is not exempt from
that. Its root is the branch's working tree only while it is standing on the
branch; a root standing on another one is a checkout of different code, and a
directory existing is not the same fact as the change being in it. Dispatch
refuses there too, naming the branch the root is actually on — and it offers no
checkout, because there is none to make: the remedies are to put the branch in
that root or to give the project branch workspaces of its own, and both are the
reader's to choose between.

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
after it — after the last ticket that was not cancelled
([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)),
since a cancelled prior is one nothing waits out. Not by opening a second plan: the point of the record is that one
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

### 8. The ticket carries the item as data, not only as prose

The dossier is written for a reader — a person or an agent — and a program
cannot read it. Yet the useful thing to put in front of an agent working a
failing gate is usually what a *script* can fetch: the log, the failing job,
the forge's analysis. A state machine can run that script before the agent, but
only if the script is told which item it is about, and prose is not an input.

So every ticket also carries the item's identifiers as **structured metadata**,
under the same names its context takes in a shell action
([§FS-004-quick-actions.1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)):
project and source, kind and item id, repository and number, branch and ticket,
url and state, and the checkout the work belongs to. One vocabulary, whether
the thing reading it is a shell command in a menu or a program in a state
machine.

Two consequences. A ticket that is appended to a plan **adds** its metadata
rather than replacing what is there, because the runtime keeps its own
bookkeeping in the same place and a ticket writing over it would break the
plan. And what is written is identifiers only — the prose stays in the dossier,
which is where a reader is looking.

### 9. Work that stops for a person says so where the person is looking

Work handed to a runtime is autonomous until it meets a question that is not
its to answer: a product decision, a trade-off between two things it cannot
weigh, an instruction it cannot read. The honest move then is to stop and ask —
and a machine that can only finish or fail will instead guess, because guessing
is the only move it has.

A runtime that can park work pending a person is therefore something ephor
reads, not something it invents: where the state a ticket sits in is one the
runtime will not leave on its own, that ticket is **waiting on the reader**, and
it is shown that way — ahead of anything else its work is doing, since it is
the one part of it nobody else will move.

The question and its answer stay in the plan. A ticket that asked something
carries the question in the artifact it wrote, and the answer belongs beside it
rather than in a chat window, a comment, or somebody's memory: the plan is the
record of what was decided about this item, and a decision taken anywhere else
is one the next round cannot read.

### 10. What ephor offers is not a limit on what can be asked

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

### 11. A failure that is not the change's fault is restarted, not fixed

Not every red gate is a broken change. A runner dies, a mirror is unreachable,
a dependency ships a bad artifact, the same flake lands on the same job for the
third day running — and what the item needs then is not a fix but another run.
A loop that cannot tell the two apart pays for the difference twice: it spends
a model on diagnosing something that was never wrong, and then it lands a
commit whose only purpose was to make the gate start again.

So **restarting is a move the loop has**, and it is a different move from
fixing. Four things follow from its being different.

It is **decided by a program, not by prose**. Recognizing infrastructure is a
judgement, and a judgement nobody acts on is the same as no judgement at all:
what the model concluded has to reach a transition. So the verdict is a marker
on a line of its own, the way a question for a person is
([§FS-005-dispatch.9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)),
and a program reads that line and picks the state. An agent that is asked to
notice something, and given nowhere to put the answer, has been asked for
nothing.

It **opens a ticket of its own**. A restart is work done to the item, and the
plan is the record of that work — what was hit, when, and whether the round
after it came back green. A restart that happened as a side effect of some
other state would leave the next round unable to see that this failure has been
retried already, which is the one fact that separates a flake from a gate that
is simply broken.

It **restarts the gate and every gate downstream of it**. A gate that spans
several repositories fails downward: the repository whose job died takes the
rest of the tree with it, and the gates below never ran at all. Re-running only
what failed leaves every one of them exactly as red, so what is restarted is
the failing gate and everything under it that is not green. Nothing is
committed — the change was never the problem.

And it is **bounded**. An unhealthy runner pool answers a restart with the same
failure, and a loop that restarts every round is one that never stops and never
says why. Past a small number of restarts on one item the work stops for a
person, because at that point the infrastructure is the thing that is wrong and
no amount of retrying is going to be the fix.

### 12. Work an algorithm can finish does not start with a model

Not everything the watch turns up is a judgment call. Replaying a branch onto
its main branch is a fetch and a rebase: it either applies or it stops at a
conflict, and it does the same thing every time. Handing that to a model is
paying a pass to have two commands typed, slower and less predictably than the
commands would have run themselves — and the pass that matters is the one after
it, on the part no algorithm can do.

So the deterministic move runs first, and the work starts where it stopped.
Where it finished, nothing is dispatched at all: a clean rebase is a done
thing, not a ticket. Where it stopped, that is the ticket, and what is handed
over is the situation rather than the request to reproduce it — the repository
is left where the algorithm left it, mid-rebase with the conflict in the
working tree, because that is the state resolving it needs, and the ticket says
which repository, which files, and which two sides
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)).

A move that costs no model costs no screen either: the replay runs beneath
the interface as a job, and what the reader would have watched is in its log
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)).

The rebase is the first of these, not the shape of the only one. Any recipe
whose opening move is deterministic makes that move before it costs a model,
and dispatches what is left over — which is also why the move has to be
runnable on its own ([§FS-004-quick-actions.6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)):
the same rebase the reader presses a key for is the one a state machine runs,
and two implementations of it would eventually disagree about what a clean
rebase is.

### 13. A communication is work too, and its answer comes back as a proposal

Not every matter's next move is a change in a checkout; often it is a reply.
A matter owing a response — a question in a review thread, a mail asking for
a decision, a mention carrying a request
(§FS-003-feed-categories.4) — is dispatched like any other: the ticket
carries the discussions as its dossier, and asks the runtime for a
**proposed answer**, drafted in the matter's context. The shipped answer
recipe is this shape, and an ask in the reader's own words (§10) may request
one for anything.

Three things distinguish it. **It needs no checkout**: the work is about the
conversation, so the plan is written where the matter resolves — the branch
workspace where one exists, because sitting in the change makes a better
answer, and the project root otherwise — and the checkout-able rung is not
required (§FS-006-project-interface.10). **The proposal is a file, never a
post**: §7 holds — the runtime writes the proposed reply into the plan's
results, ephor reads it back and surfaces it beside the discussion it
answers, and nothing reaches the channel by itself. **Posting is one
deliberate move, where the channel can carry it**: on a channel that
declares reply (§FS-007-matters.4) the surfaced proposal is offered for
posting, edited or as it stands, exactly as a reaction is posted today; on
a channel that does not, the proposal is what the person copies — a stated
degrade (§REQ-001-boundary.1), not a failure.

### 14. Who does the work is chosen, and defaulted per project

Work is handed to an agent carrying a model at an effort, and two of those
three follow from the first: which models an agent can carry and which
efforts it declares are facts about the agent, not free choices beside it.
A chooser built as `agent × model × effort` would be mostly cells nobody can
run, and ephor has no way to know which — the runtime does. So the choice has
one axis the reader picks and one dependent on it, and the set of choices is
the **roster**: the runtime binding's own enumeration of who can be asked,
read from the binding at the moment of asking rather than kept as a list in
ephor's configuration, because a copy is wrong the first time an agent or a
model is added on the other side
(§DA-004-roster-is-asked-not-configured). Every id on the roster is unique —
the runtime's model and agent namespaces are separate, and a model profile
may claim an agent's very name, in which case the profile holds it and the
agent stands alone under a marked spelling of its own — because one name
over two rows can address only one of them.

One entry is a **hand**: a name, the agent it summons and the model that
agent will carry — shown together, because a reader choosing the name is
choosing both — the **efforts** the entry declares, and whether it is
available. An entry may declare no efforts at all, which is an answer rather
than a gap: such a hand is simply asked plainly, in either spelling, because
it has no effort an ask could drop. A hand that does declare efforts is
always asked at one of them. A choice that names none is **completed** where
the hand declares exactly one — a single declared effort is a fact about the
hand, not a choice left open, and the completion is said in a note — and
**refused** where it declares several, with the efforts listed, before
anything is written: the runtime's two spellings do not agree on what an
effort-less ask would mean — one drops the effort silently, the other lets
the state machine's own choice fall in and fails outright where the hand
does not declare it — and neither answer is the reader's choice.
Configuration names a hand by its id and nothing else; the binding's own
spelling of agent, mode, provider and model is the binding's, so a
configuration written under one runtime reads unchanged under another
(§REQ-001-boundary.1).

An unavailable hand is **shown with its reason, never hidden**. This is the
opposite of what a menu does (§FS-004-quick-actions.2), and deliberately so:
a menu entry is an offer, and an offer that cannot work costs a keystroke —
but the roster is the answer to "who could I ask", and a hand that silently
vanished because its agent left `PATH` looks exactly like a hand that never
existed, which is the one confusion a reader debugging a dispatch cannot
resolve from the screen. The reason is computed where the roster is read —
the agent's command is looked for, never spawned to fail — and it is one
sentence beside the entry, the same sentence everywhere that hand appears.
The roster is reportable before anything depends on it: `ephor doctor` and
`ephor capabilities` print each hand, what it resolves to, and why an
unavailable one is unavailable (§FS-010-doctor.2).

**The hand for a piece of work resolves in seven steps**, each displacing
the ones after it: what the reader picked for this dispatch alone; the pin
the action or recipe itself carries; the project's hand for this action id;
the project's default for everything; the site's hand for this action id;
the site's default; and, where nobody chose at all, whatever the binding
would pick unasked. The order mirrors the
binding's own resolution deliberately, so ephor's answer and the runtime's
cannot come to disagree about what one configuration means. A project may
also narrow the roster — say which hands may be used on it at all, which is
what a repository under a policy about which models may see its code needs —
and a hand outside the narrowing is refused with that reason, never silently
dropped.

The first step is **made at the moment of dispatch and spent by it**. In the
interface it is a picker over the menu's entry: the roster's hands in one
column and, beside a hand that declares efforts, those efforts in a second —
absent where it declares none, which is every hand on a machine whose
runtime settings declare no model profiles, and a dead column would teach an
axis that is not there. On the command line it is `--hand
<hand-id>[:<effort>]` on the command that dispatches, the same grammar the
tables write (§FS-006-project-interface.9), so the key and the flag are one
operation (§FS-005-dispatch.12). The pick lives exactly as long as the one
dispatch it was made for: nothing records it, and the next dispatch of the
same action resolves from the second step down — a pick that outlived its
dispatch would be a configuration layer nothing wrote down. The picker never
assembles a choice the resolution would refuse — a hand that declares
efforts is picked at one of them — and it shows an unavailable hand with its
reason without letting it be chosen. A hand the project's narrowing excludes
does not appear in it at all: the narrowing is the project's policy rather
than a state of the hand, and what is refused loudly is a *named* choice —
offering the name only to refuse it would teach the policy one wasted
keystroke at a time. With an empty roster there is no picker, and the entry
dispatches exactly as if nothing had been picked.

**A chosen hand binds in one of two spellings, never both.** A hand that
carries a model is written onto its ticket, at dispatch, in the runtime's
own per-ticket execution line (§REQ-001-boundary.1) — each ticket carrying
its own choice, so two tickets in one plan can go to two hands and the
choice survives every later run. A hand that names an agent and no model of
its own — which is every hand on a machine whose runtime settings declare
no model profiles — has no line in the plan language, so it rides the run
instead: the run invocation carries the choice as the runtime's own per-run
agent flags, the agent and the effort where one was settled, resolved again
at the moment the run is invoked — the same moment the runtime reads its
own configuration, so the two answers cannot drift apart. The two per-ticket
lines rank differently against a run's flags, and ephor follows the
runtime's own ranking exactly. A ticket carrying the full execution line
cannot be re-aimed: the runtime resolves such a ticket from its line alone,
and the run's agent flags are invisible to it — only a per-run model choice
reaches past that line. A ticket carrying a model alone can be: the run's
agent flags supply its carrier, and one run advances several tickets,
including one a person pinned by hand. So the flags ride a run only where
they can re-aim nothing — every ticket the run would advance and that has
no line of its own resolves to the same spelling, and none pins a bare
model. Where one plan's open tickets do not agree, that plan runs with no
flags and the reader is told the hand went unbound for that run; plans that
agree differently are run separately, each under its own spelling. A ticket
somebody has claimed is not the run's to advance at all — a claim makes the
runtime skip it (§FS-005-dispatch.15) — and enters none of this. The
cheaper spelling is always available to the reader: a
model profile declared in the runtime's own settings turns an agent-only
hand into a model hand, and the ticket line then carries it everywhere,
with no flags involved at all.

**A run started from the interface is the same run.** The key that hands
one item's plan to the runtime resolves the hand exactly as the command
line does and carries the same flags — which surface a reader started a
run from is not a fact about who did the work, and two resolutions of it
would eventually disagree (§FS-005-dispatch.12). Such a run names one
plan and advances no other, so that plan's own open tickets settle its
flags and no other plan's can contradict them. What the resolution has to
say — a hand nothing resolves, an effort completed, a hand left unbound —
is said where the reader can still read it when the run returns: a surface
that cedes the terminal keeps the note for its own message rather than
printing it into a screen the run is about to take, and a refusal is
answered before the terminal is ceded at all, never after
(§FS-004-quick-actions.2).

With no runtime bound, or a bound one not on `PATH`, the roster is empty and
says so in the *workable* rung's own words (§FS-006-project-interface.10):
who can be asked is the runtime's knowledge, and where nobody can run work
there is nobody to ask. A runtime settings file that exists and does not
parse empties the roster too, in a sentence of its own that names the file:
a roster read around it would be a list missing whatever the person just
added, which is worse than no list. Nothing else changes — every other rung
resolves, and tickets are still written and read on disk
(§REQ-001-boundary.1).

### 15. Every operation is visible in one place

The watch can say what is being done about any one item — the lines its work
stands on beneath its row
([§23](#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)), the
work screen behind `w` — but "what is ephor doing right now" should
not require visiting every row that might hold a piece of the answer. So
there is an **operations board**: one screen, reachable from anywhere in the
interface, holding every operation beneath the reading — each live run, each
claim somebody holds, each ticket waiting on a person, and the refresh
itself, which already reports in the header of the screen being read and
appears here *additionally* (§FS-001-forge-interface.7). It is where
§FS-005-dispatch.9 pays off at the scale of the whole watch: work that
stopped for a person is one glance away wherever the person happens to be
looking — and within one operation, what asks something of the reader is
listed ahead of anything else its work is doing — a parked question first,
then what a dead run left holding — then what runs, then claims, then the
queue.

**The rows are found by looking, never by remembering.** What ephor
dispatched is in the ledger, but "every operation" is a claim about the
world, not about the ledger: the work roots themselves are enumerated —
the place a project's work is configured to live, resolved at the
project's own checkout and again in each branch workspace on disk, since
the work root is per branch workspace and each one is its own execution
root — and every plan found in one is watched, whoever wrote it. A plan
written by hand, a project's own planning tickets, and a run somebody
started in another terminal on a root ephor never dispatched into are
operations exactly as dispatched work is, judged row-worthy by the same
artifacts; the ledger still says which matter a plan is about
(§FS-005-dispatch.4), it just no longer decides what exists. And an
operation ephor never dispatched has no matter behind it by construction —
that is the common case for a foreign plan, not an edge — so `Enter`,
which goes to the matter where the feed still carries one, opens the plan
itself there: the same reading `e` offers wherever work is shown, and no
row on the board leads nowhere. Enumerating is a reading of the plan files
on disk and asks nothing of the bound runner: with no runner installed the
plans are still found and still readable — it is only operations that
cannot exist then.

**Liveness is read from the runtime's artifacts, never from a process
table.** The same reasoning that keeps work state out of the ledger
(§FS-005-dispatch.4) applies to whether work is running at all: the runtime
leaves the truth on disk. It holds a lock on an execution root for exactly
as long as a run is live there, and the operating system lets go of that
lock when a run dies, however it dies. The board probes the lock without
ever waiting on it — the runtime acquires it blockingly, so a probe that
queued would park the watch behind the very run it is asking about. Which
tickets a live run holds is read from what that run itself writes as it
works ([§15.2](#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)),
and where a runner writes no such stream, from the journal and the logs it
leaves behind.

**A row is an execution root, not a ticket.** The runtime schedules one run
per root, and ephor's work root is per branch workspace — so two items whose
work lives in one workspace are one operation, and a ticket written into a
root a run already holds is shown as **queued**, never as running: a second
run there would wait for the first. And a ticket is a ticket at whatever
depth the runtime nests it — a subtask parked three headings deep is as much
an operation as the ticket it was split from, run or no run.

**A claim is not a run.** An assignee on a ticket is written when somebody
takes it, and its effect is that the runtime skips it: it says *claimed and
unschedulable*, never *live*. A ticket with an assignee on a root where no
run holds the lock is its own flavour of row — **claimed, not scheduled** —
shown with the bound runner's own command for releasing the claim. The board
reports it; it does not act on it. And under a live run a claim stays a
claim: the run skips it, so *queued* would promise a turn that never comes.

**Parked work outlives the run that parked it.** The usual end of a run
that parks a ticket is the run exiting: nothing else was schedulable, so
the lock goes free — and the runtime wrote no claim on the way out, since
parking is a transition, not a taking. The ticket is waiting on the reader
all the same (§FS-005-dispatch.9), and it keeps its row — **waiting**,
ahead of anything else its operation is doing — whether the run that parked
it is still live, exited, or died. A root holding such a ticket is an
operation with nothing running in it, exactly as a root holding a claim is.

**Silence is a badge, not a verdict.** A long tool call is legitimately
quiet, and the lock — not the last write — is the liveness signal. A live
run that has written nothing for a while carries a **quiet** badge and
nothing more; a run that died has released its lock, and its root simply
stops being a live row — though not always a row: a ticket a dead run was
still holding mid-slot is read out of the journal that run left behind and
keeps a row of its own, **dropped by a run that died** — beside a parked
ticket, deliberately not as one: nobody else will move either, but a parked
ticket asks a question about the work, and a dropped one asks for the run
back. The artifacts tell the two apart without guessing — parked is the
machine's gating word on the ticket's own state, dropped is the journal's
unreleased slot under a lock nobody holds. The journal
outlives every run, so what it says is held is believed only while the
world still agrees: an assignment no run ever released stops counting the
moment the ticket's own state says it moved on, and is never read as
running under a run that came later.

**Watching only, and deliberately so.** The board starts nothing, stops
nothing, and intervenes in nothing — and it stays that way now that something
*can* be started beneath the screen
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)): what starts a
job is the menu entry the reader pressed, and the board is only where it is
then seen. Interfering with a live run remains out — it needs a channel to
the run that exists only while a run serves one, and a board that hinted at
it would be promising what it cannot do.

**With no runtime bound, the board holds the refresh row and ephor's own
jobs** — and that is the board being right, not broken (§REQ-001-boundary.1):
the operations a *runtime* has are runs, and where nothing can run there are
none, said in the workable rung's own words (§FS-006-project-interface.10). A
job needs no runtime to exist
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)) — it is ephor
running a command, which is the one thing that never depended on a binding —
so it keeps its row while the runtime's half of the board says why it is
empty. The tickets themselves stay readable
exactly as everywhere else in dispatch: work state is read from the plan
files on disk, that reading is the floor and is never removed, and where the
bound runner itself can be asked for a sharper listing, its answer may
refine what the files said — it never replaces them. And a work root whose
own state machine cannot be read is reported at the same altitude: liveness,
running, claims, and what a dead run dropped are facts the lock, the
journal, and the plans carry on their own, and the board still says them —
but nothing there is called queued or waiting and nothing is counted
finished, because those are the machine's words and the machine is not
there to say them. The row itself says the machine could not be read, in so
many words: a count left silently at zero would read as nothing done, which
is exactly the guess the withholding exists to avoid.

#### 15.1 The board keeps itself current

Nothing here is something the reader has to ask for twice: work that moves
on disk — a ticket advanced, a question parked, a verdict written, a run
starting or dying — surfaces on its own within moments, not at the next
refresh. This is not the refresh (§FS-001-forge-interface.7) wearing a new
name: a refresh asks the world's forges and costs what they cost, while this
watches files ephor already knows by name and asks nothing of any forge
(§GOAL-005-costless). It is cheap by construction — nothing is re-read while
nothing has changed, a timestamp answers that, and the timestamps asked
are a fixed handful per root: each plan file the last enumeration found,
the root's own directory — a plan appearing or vanishing is a directory
event — and the artifacts the runtime moves as it works: the one it writes
on every slot it takes or releases, and the live run's own stream
([§15.2](#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)), which
is one more name in the same fixed handful and never a sweep of everything
the runtime ever wrote; the bound runner is
asked to list its plans only about a root that holds an operation, and
nothing is ever read while a frame is being put on screen — and it holds
everywhere work is shown, not only on the board: a ticket the runtime
parks resurfaces on the reader's rows when it parks (§FS-005-dispatch.9),
instead of waiting for a refresh that was never going to be about it.

Finding the roots (§FS-005-dispatch.15) is the one walk in the design, and
it is not the tick's: the work roots are enumerated when the rows are
built — the board opened over the reading, rebuilt because the glance saw
something move, or a refresh landing — and never merely because time
passed. The walk is bounded by where work is configured to live, not by
what the disk holds: it visits the project checkouts and the branch
workspaces ephor already resolves, to the fixed depth a branch name can
nest, and costs one directory listing per candidate work root — it never
descends into a repository, and it never reads a plan to find it.

#### 15.2 What a run is doing is read from the run's own stream

The lock says a run is live and the plan says what state a ticket is in.
Between those two facts sits everything the reader actually wants from a
live row — which ticket the run has in hand right now, what it just
finished, whether it is still moving — and it was reconstructed from a
journal that outlives every run. That journal is the wrong witness for a
question about *this* run: it is append-only across all of them, so a run
that died mid-slot leaves an assignment nobody ever released, and every
later reading has to argue that entry down from evidence elsewhere — the
ticket's own state, the age of a log against the birth of the lock. The
answer is right and the reasoning is a chain of inferences about a file
that was never about one run.

So where the runtime keeps a **record of the run itself** — what that run
took up and let go, in order, from its own beginning — that record is what
a live row is read from. Its properties are the ones the inference chain
was standing in for: it belongs to one run, so nothing in it can be another
run's leavings; it is ordered and numbered, so a reader can say exactly how
much of it it has seen; it says how each slot ended rather than leaving the
end to be deduced; and it says when the run ended, so a reader knows the
difference between a run that finished and a run that stopped existing.

**The journal stays the floor.** A runner that writes no such record is
read exactly as before, with the same inferences and the same caution
(§FS-005-dispatch.15): this sharpens the reading where the artifact is
there and is never a thing a runtime has to provide to be bound at all
(§REQ-001-boundary.1). Where both exist the run's own record answers,
because a witness to one run beats a witness to all of them.

**Liveness is still the lock.** A record that ends saying the run finished
agrees with a lock that is free; a record that simply stops does not mean a
run is gone, only that it has written nothing lately, which is the quiet
badge's business and not a verdict (§FS-005-dispatch.15). Nothing here is
consulted to decide *whether* a run is live — only what the live one is
doing, and what the last one did.

**And it is read where work is shown, not only on the board.** The reader
looking at a matter's rows sees the same word the board would show, because
this is one reading narrowed to one matter rather than a second one of its
own ([§23](#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)).

### 16. Work that should not go on is cancelled, and the plan says so

Not every ask should run to its end. The same recipe was pressed twice; a
ticket was asked for on the wrong item; the item moved past the question
before anyone picked it up. What is wanted then is not a fix and not a
reopen but a **cancel** — and a watch that can hand work over and cannot
take it back leaves the reader with two runs of the same fix in one
checkout, or with an editor open on the plan, guessing at what the runtime
would have written.

So a ticket can be cancelled: from the work screen, on the ticket, and from
the command line. Cancelling moves the ticket into the machine's
**abandonment state** — spelled `cancelled` in the plan language, the one
final state that satisfies no `**Prior:**` — carrying the reader's reason
as the ticket's result. Two things follow from its being a state.

**It is the runtime's move, made in the runtime's own words.** The plan
language reserves a ticket's state, after it is written, to the runtime's
own verbs — the compare-and-swap, the artifact checks, the callbacks, the
audit trail — and a state line ephor rewrote by hand would be a plan the
runtime can no longer vouch for
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work),
§DA-005-cancel-is-the-runtimes-move). So ephor asks the runtime for the
transition, captured rather than watched, and what comes back is what the
reader is told: the ticket cancelled, or the runtime's own refusal in its
own sentence. It follows that with no runner bound cancelling is refused in
the workable rung's words like running is
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)): the
plan is still readable and hand-editable, and nobody is there to make the
move.

**The record keeps it.** A cancelled ticket stays in the plan, in its place
in the order, marked as cancelled with its reason beneath it — the same
reading a finished ticket gets. Nothing is deleted: the plan is the record
of what was decided about this item
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)), and taking an ask
back is a decision too.

Cancelling refuses where the move would be wrong, and says why in one
sentence before anything is asked of the runtime. A ticket a **live run
holds** is that run's to finish: pulling the state out from under an agent
is interfering with a run, which is the later section
([§15](#15-every-operation-is-visible-in-one-place)), and the refusal names
the run. A **finished** ticket has nothing left to cancel. A machine that
declares **no abandonment state** is refused with the machine and its file
named, exactly as a recipe naming a state the machine lacks is
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)):
a ticket moved into a state the machine does not have leaves a plan the
runtime refuses to run at all. The machine ephor ships and the examples
beside it declare the state and a transition into it from anywhere, so a
work root ephor made can cancel from the start; a machine of the reader's
own says whether it can, and the refusal tells them what to add. Everything
else is fair: a ticket that is queued, parked on a question, dropped by a
run that died, or claimed and unscheduled is somebody's to cancel, and the
runtime's own checks are the last word.

Order follows from the state's name. A ticket **ordered after** one now
cancelled will not start — the abandonment state satisfies no `**Prior:**`,
which is what its spelling is for
([§11](#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed))
— so cancelling says which open tickets those are, and cancelling them too
is one more keystroke; ephor does not decide for them. And a ticket ephor
appends afterwards — a reopen, a second ask — is ordered after the last
ticket that is **not** cancelled
([§5](#5-an-item-that-moved-reopens-its-work)), so ephor's own chain never
hangs off abandoned work.

### 17. A move that needs nobody runs beneath the screen

Pressing a key for a deterministic move costs the whole interface for as long
as the move takes. Replaying a forest onto its main branch is a fetch and a
rebase per repository, and on a checkout weeks behind that is minutes of
output — output that asks nothing, decides nothing, and is read afterwards if
it is read at all. Meanwhile the watch is gone: the reader cannot look at the
next item, cannot start the second move, cannot even see that the first one is
still going, because the screen that would say so has been given away. Then it
asks for a keypress to hand the screen back. The reader paid the interface for
a command that never needed it
([§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).

So a menu entry that does not need the reader runs as a **job**: started
beneath the screen, with the interface staying exactly where it was, and the
job taking a row of its own among the operations
([§15](#15-every-operation-is-visible-in-one-place)). What was a takeover
becomes a line saying the work started and a row saying it is going.

**Which entries these are, the entry says.** ephor's own deterministic moves
are jobs by construction — the rebase is the whole argument of
[§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model), and a
move that costs no model has no reader to cost either. Everything a person or
a project wrote keeps the terminal unless it asks otherwise, because a menu
entry has always been allowed to *be* the reader's session — `lazygit`, an
editor, a pager (§FS-006-project-interface.9) — and starting one of those
beneath the screen would leave a program nobody can type into, waiting on a
terminal it does not have. Which way an entry runs is therefore the entry's
own word, and its default is the one that was always safe.

**A job outlives the interface that started it.** It is its own process in its
own process group, so quitting ephor does not take it down, and neither does
closing the terminal: a move that needed nobody watching does not suddenly
need somebody staying. This is the same property from the other side — work
handed to the runtime already survives the screen (§FS-005-dispatch.4), and a
move ephor runs itself should not be the fragile one.

**Everything the reader would have watched is kept.** The job's output —
what it wrote and what it complained about, whole and in order — goes to a
log, and the log is the inspection: from the job's row on the board, from the
work screen of the item it was started about, and from the command line. A
job that is going says what it is doing right now, because "still running" and
"stuck" are the same three words on a row, and the log is the difference.

**Liveness is the lock, exactly as it is for a run.** A job holds one for as
long as it runs, and the operating system releases it when the job dies,
however it dies ([§15](#15-every-operation-is-visible-in-one-place)); the
board probes it and never waits on it. Nothing consults a process table, and
nothing believes a record that says a job started: a job ephor started and
then crashed alongside is not running, and the lock says so without being
asked.

**The rows are found by looking, here too.** Jobs are read from where they are
written, not from anything that remembers having written them, so a job
started from another ephor — a second terminal, an earlier session that has
since exited — is a row like any other, and a record with no job under it is
history rather than a claim.

**The chain travels with the job.** An entry that needs the item's branch
workspace still gets the checkout first (§FS-004-quick-actions.7), inside the
job and against the same verification: the directory is checked for rather
than trusted (§FS-006-project-interface.8), and a checkout that did not make
it ends the job there with what it said. A job is a sequence because the move
was a sequence; it is not a way to run two unrelated things.

**The outcome comes back to the row it was about.** A job ending is news
exactly as a ticket parking is (§FS-005-dispatch.9), and news read at the top
of the screen is news about nothing in particular: a header line saying a
replay went through names no branch, so a reader with three of them going has
to guess which row just moved. So the line lands **under the subject the job
ran on** — the branch it replayed (§FS-004-quick-actions.6), or the matter it
was started about — beside the facts it changed, and it stays there until the
reader opens that row or a later job about the same subject replaces it. Where
that row is not on the screen at all, the project's own row carries it: news
with nowhere to land is news that is lost.

**Only what has ended lands there.** A job still going is already marked
running where it could be started again
([§21](#21-what-is-already-going-is-shown-where-it-could-be-started-again))
and holds a row among the operations
([§15](#15-every-operation-is-visible-in-one-place)), and a third live mark
on the tree would be one fact said three times. A job that has ended is no
longer an operation and leaves the board — an inbox that accumulated every
finished thing would be the pile this section exists to avoid — while its
record stays with the item it was about, log and all, until it is old enough
to be swept.

**What the move hands over, it still hands over.** A rebase that stops in a
conflict dispatches its ticket exactly as it did when the reader was watching
([§12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)), and
that ticket is a run with a row of its own. The job ends; the work does not.

### 18. The work screen says when, and folds away what is over

What was asked for and what ephor ran are both lists of things that already
happened, and a list of things that happened without times answers half of
what the reader brought to it. "Did I already press this?", "is that the job I
started a minute ago, or the one from yesterday?" — those are questions about
time, and a screen that will not answer them sends the reader to the ledger
and the job directory to read timestamps that were on disk the whole while
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work),
[§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)).

So **every row that already happened says when**, in the age the watch
already spells on its rows rather than a clock time the reader has to
subtract from: a screen read at a glance is read in "12m ago", not in
"14:22". A ticket says when it was **asked for** — the ledger's record of the
dispatch, not anything the plan holds, since the plan is the runtime's and
tracks what the work reached rather than when it was handed over
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)). A
job says when it **ran**, and how long it took; a job still going says how
long it has been going, which is the same question asked of a thing that has
not finished, and the only answer there is.

**An age nothing can source is left off.** A ticket somebody wrote into a
plan by hand was never dispatched, so nothing knows when it was asked for,
and that row simply carries no age. The plan file's own modification time
would be ephor inventing a fact about work it did not start, which is the
same refusal the board makes about rows it did not write
([§15](#15-every-operation-is-visible-in-one-place)).

**What is over folds away.** Tickets accumulate, and are meant to: a reopen,
a second ask, a cancel, months of work about one long-lived change, all of it
kept ([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)).
But a reader opening this screen is looking at what is still going, and the
finished and the cancelled push it down the screen a row at a time until it is
off it. So the screen leads with the tickets that are still open and collects
the ones that are over behind a single line saying how many there are and how
to see them; one key unfolds them in place, in their order in the plan, ages
and all. Nothing is hidden that a keystroke does not show, and nothing is
deleted: this is a reading of the record, and the record is unchanged
(§GOAL-003-nothing-lost).

**A plan with nothing open is not folded to nothing.** Where every ticket is
over, they are all shown: folding the whole list away would leave a heading
above an empty space and a reader wondering where the work went. The fold is
for what is over *beside* what is not.

### 19. A workflow the runtime offers is an action, and its inputs are answered here

A binding brings more than a place to put tickets. It carries **workflows** —
named, parameterized plans that lay down tasks of their own, under a machine of
their own, with fan-out and gates ephor never wrote — and a reader who has one
is in exactly the position
[§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)
describes: the thing worth doing about this row is already written down, and
doing it means leaving, remembering a vocabulary, and coming back. So a
workflow is an entry in the same menu, selected by the same language, ordered
by the same provenance, refused in the same sentence.

Above the binding a workflow is an id, a description, and its **inputs**: each
one a name, a type, whether it is required, and what it stands at when nobody
says. Everything else about it — where workflows live, what a plan rendered
from one looks like, how it is rendered at all — is the binding's, and is
spelled there (§REQ-001-boundary.5). Which workflows there are, and what each
one takes, is the binding's own knowledge too: it is asked of the binding at
the moment of asking rather than kept as a list of ephor's, for the reason the
roster is (§DA-004-roster-is-asked-not-configured) — a copy is wrong the first
time a workflow is added on the other side. So with no runtime bound there are
no workflows, in the *workable* rung's own words
([§FS-006-project-interface.10](#10-capability-rung-by-rung)), and nothing else
changes: the plans a workflow already laid down go on being found by looking
and read from disk, like every other plan there
([§15](#15-every-operation-is-visible-in-one-place)).

**An entry names a workflow, and that is what makes it an action.** The entry
is the one the menu already has
([§FS-006-project-interface.9](#9-offers-the-projects-actions)) — an id, an
icon, a description, a `when`, its capability requirements — and where it would
carry a command or a brief it carries a workflow's name and the answers to its
inputs instead. It is written in any of three places: beside the workflow
itself, where the binding keeps one somewhere a reader can put a file — there
it travels and versions with the workflow; in the project's manifest; in the
person's own configuration. Narrow beats broad, as everywhere else. A
workflow the binding ships is ranked with what ephor ships, one the person
keeps with the person's own, one the project keeps with the project's offers —
the provenance the menu already orders by, with nothing new to learn, and the
same rule settling two entries that share an id.

**A workflow no entry names is still asked for.** Requiring configuration
before a workflow can be used once is the cost
[§10](#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked) exists to
refuse — but a menu carrying every workflow the machine can find, on every row,
is [§FS-004-quick-actions.2](#2-offered-only-where-it-would-work) broken at
scale: most of them have nothing to do with the item, and a reader who has to
read past twenty of them to reach the two that do has lost the menu. So the
named ones stand in the menu where they apply, and every other workflow is
behind one entry that opens the list of them, where it is picked, answered, and
run once. What was answered there can be kept as an entry, which is how a
workflow becomes an action without anybody opening a configuration file first.

**The inputs are answered in five steps**, each displacing the ones after it:
what the reader answered for this instantiation alone; what the entry says;
what ephor answers for an input about who does the work; the workflow's own
default; and, where an input is required and still unanswered, the reader —
asked, or refused by name where nobody is there to ask. The order is
[§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project)'s, deliberately,
so that one resolution order covers everything a dispatch has to settle.

What the entry says is data, not prose: a string is rendered with the item's
fields where it names them, exactly as a brief is
([§1](#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)), and
anything that is not a string is passed on as it stands, because an input that
wants a number, a flag, or a list of them is not served by a sentence. Where a
list or a record holds strings, those are rendered too — the fields an item
carries are as useful inside a structure as beside one.

**What ephor knows reaches a workflow as files, not only as words.** The
dossier and the identifiers are already written for exactly this
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it),
[§8](#8-the-ticket-carries-the-item-as-data-not-only-as-prose)), and a workflow
has no place in its plan for either — so an entry may answer an input with the
dossier or with the item's identifiers, and what the workflow is given is a
path to a file ephor has already written, or that file's contents where the
input wants the text itself. It is written before the workflow is, so an input
that insists its file exists is answered truthfully, and it is written where a
work root's own reading will not mistake it for a plan.

**Every input is answered on one screen, with its answer already standing in
it.** Answering a workflow is not filling in the blanks. The answers that were
resolved are as likely to be the wrong ones as the missing ones — the
workflow's own defaults most of all, which are a stranger's habits about which
model reviews and how many passes it takes — so what the reader is given is
every input the workflow declares, each carrying the answer the five steps
reached and the name of the step it came from, and each one changeable there.
A screen that showed only the holes would be asking the reader to accept
everything else unseen, which is the opposite of what showing an account is
for.

**What has a known set is chosen from it; the rest is typed.** An input is
edited in the shape it actually has. Where the values it can take are known,
one is picked from them rather than spelled: a flag has two, an input that
names who does the work has the roster, already narrowed and already saying
who is unavailable and why
([§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project),
§DA-006-hands-fill-a-workflows-targets),
and an input whose own check is a plain set of words has those words. An input
wanting several of them is answered several at a time, from the same set.
Everything else is one line typed on its own row, which is
[§10](#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)'s ask with
the input's name already on it. What ephor reads out of a check is a
convenience and never a second authority: a value the offered set does not
hold can still be typed, and the binding remains the one thing that validates
its own inputs. What no row can carry — a record, or a list of them — is the
reader's editor, on that input alone or over the whole set at once, which is
the same handover an offer already takes.

**What is still unanswered blocks the laying and says so, or refuses by name.**
A required input nobody has answered leaves the screen open, naming what is
missing, rather than laying a workflow down with a hole in it. Where nobody is
there to ask — a dispatch of every matching item at once — the entry refuses
and names the inputs it could not answer, because a workflow written with a
hole in it is a piece of work that looks scheduled and never happens
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)).

**Who does the work is ephor's answer, not the workflow's.** A workflow's
inputs are mostly its agents: which one reviews, which one adjudicates, which
one writes — each defaulted to a model its author happened to have. Left alone,
those defaults are a hole in everything
[§14](#14-who-does-the-work-is-chosen-and-defaulted-per-project) and
[§FS-006-project-interface.9](#9-offers-the-projects-actions) settle: the
project's table stops applying, the reader's pick stops applying, and a project
that narrowed which hands may see its code is narrowed right up to the moment a
workflow is instantiated, which is the moment it mattered. So an input that
names who does the work resolves through the seven steps like any other hand,
and is rendered into the binding's words in the one place that renders them
(§DA-006-hands-fill-a-workflows-targets); an
input wanting several is answered with several. A hand a narrowing does not
permit is refused with that reason wherever it was named — the workflow's own
default included, since a default is a naming — and never quietly replaced.

**What it writes is a plan beside the item's, not a ticket inside it.** A
workflow lays down a plan of its own; it cannot be appended to the item's
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)), and pretending otherwise
would mean rewriting somebody else's workflow into ephor's shape and losing
what made it worth having. It is written into the item's own work root all the
same, and everything that follows from that is the point: the operations board
finds it by looking, like every other plan there
([§15](#15-every-operation-is-visible-in-one-place)); it shares the root's one
run, so a workflow and a ticket about the same change queue rather than edit
the same tree at once; and the ledger records the dispatch against the plan it
made, which is the fact that was missing — the record says the item, the entry,
the plan, and what the item looked like
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)), and
gains a plan of its own beside a ticket of its own, which is an addition and
costs nothing to what is already written
([§FS-006-project-interface.11](#11-the-interface-is-versioned)). An item that
moved is offered the workflow again rather than a ticket appended to it
([§5](#5-an-item-that-moved-reopens-its-work)), ordered after what came before
and named apart from it, because two runs of one workflow about one item are
two records and not a correction of the first.

**Instantiating writes files; running is the move after it.** Everything
[§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)
guarantees holds here and holds for the same reason: what a key press does is
write a plan, and what runs it is the reader, from the board where every other
operation is run. A workflow lays down more than a ticket does — a directory,
a machine, sometimes settings — so what is about to be written is shown before
it is, in the binding's own account of it beside ephor's own account of every
input it answered and where the answer came from. A workflow that fails to be
written leaves nothing behind, so the board is never given half of one to
report on, and the refusal read back is the binding's own.

**What comes back is what the plan says, and no more.** A ticket ephor wrote
carries the shape a verdict and a proposed answer are read out of
([§13](#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal));
a workflow answers to its author, not to ephor, and reading a verdict out of
one would be ephor inventing a fact about work it did not shape. So workflow
work is read at the altitude every plan is read at — its states, what is
waiting, what is finished — and nothing is attached to the matter that the
matter did not get. In the same spirit, where a workflow brings settings of its
own into a work root, those settings are the root's from then on and govern
every plan in it, ephor's tickets included: what changed is reported when it is
written, because a workflow quietly re-answering how another workflow's agents
run is exactly the kind of fact a watch exists to say out loud.

### 20. A run of the runtime starts beneath the screen, and is watched by attaching

Pressing the key that runs the runtime hands it the whole interface for as
long as the run takes — and a run takes as long as the work does. Everything
[§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen) says about a
replay holds here with more force: the work was handed over precisely so that
nobody had to stay
([§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)),
ephor is the half that remembers, and a screen given away to one run cannot
watch the other items, cannot start the next move, and cannot even say that
the first one is still going. A run that the reader *wants* to watch is the
exception that was made the rule.

So **a run starts detached**: where the binding can, the runtime is started in
a session of its own, outliving the screen and the terminal that started it,
and what the reader gets is one line saying the run began and what it is
called. The root turns live on the board
([§15](#15-every-operation-is-visible-in-one-place)) from the lock, as every
run does — nothing new is watched, because nothing about "is a run live here"
changed. A move that needs nobody does not suddenly need somebody staying,
and a run is the longest such move ephor makes.

**A run has an identity, and it is the binding's.** A live run names itself —
an id, and while it serves one, the address of its control — and both are
read from the artifacts the binding leaves beside its lock, never from
anything ephor remembers having started: a run somebody started in another
terminal, on a root ephor never dispatched into, has the same identity and is
reached the same way
([§15](#15-every-operation-is-visible-in-one-place)). An id is how the reader
and the runtime agree on which run they mean, so the board says it on the
row, the work screen says it on the operation, and the command line prints it
with the rest (§FS-011-command-line.8).

**Watching is attaching.** The binding's own surface for a run it did not
start — a reader of the run's files and a client of its control — is opened
on the run, and leaving that surface detaches and never stops the run: the
reflex that ends a foreground command must not end a run another screen may
also be watching. The surface is something the reader types into, so by
[§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)'s own rule it
takes the reader's terminal — or a window of the reader's own, where one is
bound ([§22](#22-a-window-of-the-readers-own-where-one-is-bound)). What the
surface can do — answer a question the run parked, release a gate, intervene
— is the binding's, unchanged, and ephor adds nothing to it and takes nothing
from it.

**Stopping stays out of the screen.** The board starts nothing and stops
nothing ([§15](#15-every-operation-is-visible-in-one-place)), and a detached
run does not change that: where a run can be stopped, the row carries the
runner's own command for stopping it, in the runner's own words, exactly as a
claim carries the command that releases it
([§10](#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)). A key that
stopped a run would be a channel to the run ephor promised never to hold.

**A question a detached run asks still reaches the reader.** A run with
nobody at its terminal waits at a human gate rather than exiting — that is
the binding's own contract, and it is the right one, because the person who
releases the gate is expected to arrive later. ephor reads the wait exactly
as it reads a parked ticket
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)):
the ticket says *waiting on you* wherever the reader is looking, and the way
to answer is to attach.

**Where the binding cannot detach, the run is watched as it was.** A runner
with no detached shape, or a platform that has none, runs attached — the
terminal handed over, the reader watching — and the one line says so rather
than pretending. With no runtime bound there is no run to start, in the
workable rung's own words (§FS-006-project-interface.10).

### 21. What is already going is shown where it could be started again

The menu says what can be done about a row; the board says what is being
done. Kept apart, they forget each other: a reader who opens the menu on an
item whose rebase is already replaying is shown the rebase as something to
start, presses it, and is either refused by a lock or has started a second
one — and has to visit the board to learn which. That is the watch knowing a
fact and not saying it where the reader is looking (§GOAL-002-glance).

So **every entry that has work going about its subject is marked running,
and set apart**: the running entries stand first, under a line that says so,
indented a step further than the rest, in one colour reserved for what is
going and used for nothing else on that screen. Each says how long it has
been going and what it is at right now — the job's own last line, the ticket
a run holds and the state it is in, *waiting on you* where the ticket it
opened is parked, with a run still on the root or without one, *queued* where
the root's run will reach it — in the words the board already uses
([§15](#15-every-operation-is-visible-in-one-place),
[§18](#18-the-work-screen-says-when-and-folds-away-what-is-over)), because
this is the board's reading narrowed to one row, not a second reading.

**What counts as going is found by looking, here too.** A command entry is
running where a job started from that entry, about this subject, still holds
its lock — so a job records which entry it came from and, on a branch row,
which branch, or nothing could ever match it back
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)); the checkout
row is running where the job that is making the workspace is. An entry that
hands work over is running where the ticket it would open, or the plan it
would lay, is open and its root is live or will reach it — the very facts the
work's own lines beneath the row are made of
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking),
[§23](#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)) —
and a ticket the run parked counts, live root or not, because a question
standing on this subject is exactly what a second dispatch must not be laid
beside.
An entry whose program runs in a window of the reader's own is running while
that window holds it
([§22](#22-a-window-of-the-readers-own-where-one-is-bound)). Nothing here is
remembered from the keypress: a second ephor opening the same menu sees the
same rows, and a job that died is not running, whatever started it.

**Pressing a running entry opens it; it never starts it again.** The key on a
row that says *running* goes to the thing that is running: a job's log,
followed as it writes ([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen));
a run of the runtime, attached
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching));
a program in its own window, that window brought forward
([§22](#22-a-window-of-the-readers-own-where-one-is-bound)). A second copy is
not what a reader pressing a row that says *running* meant, and where
somebody does mean it the command line starts it and the refusal is the
lock's own sentence. The footer says *open* on such a row, not *run*, because
it is built from the row and not from the key (§FS-004-quick-actions.2).

**Both surfaces say it.** The list `ephor actions` prints carries the same
mark with the same facts — what is running, since when, and the way in
(§FS-011-command-line.8) — so that a program reading the menu cannot start
what a person reading it would have opened (§REQ-002-parity.2).

### 22. A window of the reader's own, where one is bound

Two kinds of thing above want a terminal: the surface that attaches to a run
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)),
and the program an entry *is* — an editor, a pager, a coding agent's own
session (§FS-006-project-interface.9). ephor has one terminal and is sitting
in it. Handing it over works everywhere, and stays the floor: ephor leaves,
the program runs, the reader comes back. But a reader inside a multiplexer,
or in a terminal that opens windows on request, has a better move available
— the program in a window of its own, ephor still on screen, and "open" from
then on meaning *bring that window forward* — and a tool that cannot make
that move sends them to make it by hand, which is the sweep this project
exists to retire (§GOAL-003-nothing-lost).

So **the window is a seam** (§REQ-001-boundary.1, decided with its tradeoff
recorded in §DA-007-window-is-a-bound-opener), with the anatomy every seam
has. The contract is in materials: one command that opens a window
running a given command and prints a handle for the window it made, and one
that brings a handle forward. The binding is configured — `window` in site
configuration names which — and ephor ships bindings for the common shapes
(a terminal multiplexer, and terminals that take remote commands), chosen
unasked when the environment ephor is running in says which one the reader
is sitting inside, and never by spawning one to find out. The degrade rule is
the floor: no binding and no recognized environment means no window, and
the terminal is handed over as it always was. A window is the reader's: ephor
opens it and brings it forward, and never closes it or ends what is in it
([§15](#15-every-operation-is-visible-in-one-place)).

**An entry may ask for a window.** An offer or a configured action says
`window` as it already says `background`
(§FS-006-project-interface.9): its program runs in a window of its own
instead of taking the terminal, and ephor stays where it was. Such a program
is an operation while it runs — it holds a lock as a job does
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)), its record
keeps the window's handle, and the window is its inspection where a log
would have been, because what it writes is on that screen and nowhere else.
That is what makes a coding agent started from the menu a row that says
*running* and opens to the agent
([§21](#21-what-is-already-going-is-shown-where-it-could-be-started-again)),
rather than a program ephor handed the terminal to and forgot. Where no
window can be opened, the entry takes the terminal as it always did, and
says so.

**Attaching goes to a window where one is bound.** The surface on a run
opens in a window when there is one to open, and in the terminal otherwise;
either way leaving it detaches and the run goes on
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)).

### 23. Work stands on rows of its own, beneath the row it is about

A matter's work used to ride on the end of the matter's own line — one
phrase, after the title, the state and the gate: *⚙ fix-gate · fix*. It said
the true thing and left the reader nowhere to go with it. The line is not a
row, so the cursor cannot reach it; the keys on the row it rides belong to
the matter, so none of them are the work's; and the phrase is cut to what is
left of the width after everything else on the line has taken its share. A
reader who reads *⚙ fix-gate · fix* and wants to take it back has to open a
second screen to find the thing they were already looking at, which is the
sweep this project exists to retire (§GOAL-003-nothing-lost).

So **the work comes off the matter's line and stands on lines of its own,
beneath it** — indented a step, selectable like any other row, one per ticket
the plan holds open. The matter's line goes back to being about the matter,
and the work is where a key can reach it.

**What each line says** is what the board and the work screen already say, in
their words, because this is that reading narrowed to one matter and not a
third one of its own
([§15](#15-every-operation-is-visible-in-one-place),
[§18](#18-the-work-screen-says-when-and-folds-away-what-is-over)): the ticket's
recipe and the state it is in, and how long since it was asked for, where the
ledger knows — a ticket nobody dispatched carries no age rather than a guessed
one. A ticket the runtime parked says *waiting on you* and stands first among
them, since it is the one part nobody else will move
([§9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).

**A ticket a run has in hand says so, and says it at a glance.** *Open* and
*being worked on right now* are different facts, and a row that spelled them
the same way left the reader to guess which of the two they were looking at:
the ticket an agent is inside of and the ticket nothing has picked up wore one
marker and one colour, and the only way to tell them apart was to leave for
another screen — which is the sweep this project exists to retire
(§GOAL-003-nothing-lost). So a ticket a live run holds is marked apart from one
merely open, read from the run's own record of itself
([§15.2](#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)) and
phrased as the board phrases it, because this is that reading narrowed to one
matter and not a third one of its own
([§15](#15-every-operation-is-visible-in-one-place)). A ticket on a root whose
run is live but busy elsewhere is *queued*, for the same reason the board says
so: it will get its turn without anyone doing anything. And a live run that has
gone quiet carries the badge it carries there — a long tool call is legitimately
quiet, so it is a badge and never a verdict.

Nothing here is a new question asked of the world: it is the liveness the watch
already probes and the record the run already writes, said on the row the
reader is already looking at.

**What is over is one line, not many.** Tickets accumulate and are all kept
([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)), and
a tree that grew a line per finished ticket would bury the matters between
them. So where a plan holds nothing open, its work is one line for what the
last ticket decided — the verdict, or *cancelled* — and where it holds
something open, what is over is not on the tree at all: the work screen is
where the whole record is read
([§18](#18-the-work-screen-says-when-and-folds-away-what-is-over)). An item
that has moved under its work says so on a line of its own, in the words §5
gives it, because that is a fact about the work and not about the matter
([§5](#5-an-item-that-moved-reopens-its-work)).

**The keys are the work's, on the row the work is on.** On such a line,
cancel takes *that* ticket back — named, with no second screen to choose it on
([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)) —
attach watches the run holding it
([§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)),
and the plan opens for reading. What the line is *about* is still the matter,
so the keys that go to the matter — its thread, its gate is not among them,
its work screen, its menu — go there from here too. A key means one thing at
a time and the footer says which, measured against the row the cursor is on
rather than the screen it is in (§FS-004-quick-actions.2): where the cursor
stands on work, the footer offers the work's keys and not the ones they
displaced.

**A line is offered only where the move behind it would work.** Cancelling is
the runtime's move and is refused in the runtime rung's words with nothing
bound ([§16](#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so));
attaching needs a run actually holding the root, read at the keypress from the
lock and never remembered
([§15](#15-every-operation-is-visible-in-one-place)); a line for work that is
over has no ticket to take back. Each says so in one sentence rather than
appearing to act.

### 24. Work nobody has to start starts itself

A ticket that needs a person to press a key before anything happens to it is
a ticket waiting on a person, whatever its state says. For the work a reader
has already decided about — *the gate is red, collect what failed and fix
it* — that key is a formality standing between the dispatch and the run, and
the interval it costs is not the reader's attention being spent well: it is
work sitting in a plan on disk while the person who asked for it does
something else, and the row saying *⚙ fix-gate · collect* the whole time,
which is true and reads as *going* when nothing is going at all.

So **a recipe may say that the work it asks for needs nobody to start it**,
and a ticket written from such a recipe gets its run without anyone pressing
anything. The reader's deliberate act moves one step earlier and is made
once: adopting the recipe, rather than starting each of its tickets
([§7](#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)).
Everything a recipe already decides — which items deserve work, what to ask
for, whose hand does it — is the same decision, and *and do not wait for me*
belongs beside it.

**Nothing autoruns unasked.** Silence means the key, exactly as before: a
recipe that says nothing about this is started by the reader, and so is
every menu entry, every workflow, and every plan somebody wrote by hand. The
setting is per recipe and nowhere else, because the reader who trusts one
kind of work to start itself has said nothing about the rest.

**Starting is a sweep, and the sweep reads the world.** What starts a run is
not a memory of having dispatched something — that would be the ledger
deciding what exists, which is the one thing it never does
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work)) —
but the same looking every other reading here does
([§15](#15-every-operation-is-visible-in-one-place)): a root is **due** when
a plan in it holds a ticket that is open, unclaimed, not parked on a
question, and from a recipe that asked to run itself — and no run is live on
that root. A ticket a hand wrote into such a plan is due exactly as a
dispatched one is; the recipe is a fact about the ticket, not about who
appended it.

**One run per root, because that is what the runtime schedules.** A root a
run already holds is left alone: the live run reaches a ticket written
beneath it, and a second run there would only wait for the first
([§15](#15-every-operation-is-visible-in-one-place)). This is why the sweep
can be run as often as anything cares to run it and asks nothing about what
it did last time — a due root gets a run, a live root gets nothing, and
running the sweep twice in a second is the same as running it once.

**The sweep is safe where the key was.** Everything dispatch refuses before
writing a ticket, starting refuses before running one
([§6](#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not)):
a branch that is not in the working tree the plan is about is not run in,
whatever the directory holds
([§3](#3-one-rhei-per-item-one-ticket-per-dispatch)). And what it starts is
the run [§20](#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)
already describes — detached, identified by the binding, watched by
attaching. Nothing about how a run is seen, stopped, or answered changes
because nobody pressed the key that began it.

**A start that fails is not tried again immediately.** A root that cannot
start a run — a runner that refuses, a workspace that has gone wrong — would
otherwise be retried by every sweep for as long as the ticket stays open,
which is a loop nobody asked for and the most expensive kind of quiet. So a
failed start is remembered as ephor's own record of what ephor did (never as
work state — that is still the plan's
([§4](#4-the-ledger-is-ephors-record-and-never-the-truth-about-the-work))),
and that root is left alone for a while, longer each time it fails. The
failure itself is not swallowed: it lands under the row it was about, the
way a job's outcome does
([§17](#17-a-move-that-needs-nobody-runs-beneath-the-screen)), so a reader
who never pressed anything still learns that the thing they did not press
did not happen.

**The reader keeps the key.** Starting a run by hand is unchanged and is
never refused on the grounds that a sweep would have got there — a reader
who wants it now says so, and a recipe that autoruns is still work whose run
can be watched, attached to, and stopped in the runner's own words. The
sweep only removes the requirement that somebody be present, and the board
stays what it is: it starts nothing itself, and what it shows is the run,
whoever asked for it
([§15](#15-every-operation-is-visible-in-one-place)).

## FS-006-project-interface: a project and ephor meet over one interface, in three homes

ephor requires capabilities of a project, never artifacts in it
(§REQ-001-boundary.3): a project is fully watchable exactly as it stands, and
everything beyond watching is something the project *can do* or *chooses to
offer*. This section is the whole of how a project and ephor speak — where
each fact lives, how a command is invoked, what it may answer, and what its
absence degrades to — so that tracking a new project costs minutes and
touches nothing in it (§GOAL-005-costless). Whatever crosses this interface
in structure is validated against a published schema; nothing crosses it as
linked code.

### 1. The three homes

Every fact the interface carries lives in exactly one of three places.
**Description and identity** live in the registry row: where the forest is,
how a branch becomes a workspace, and the signals by which the project's
matters are recognized (§FS-008-attribution.1). **Operational bindings**
live in site configuration: which command fills which verb, which runtime
runs work, the person's own actions and recipes. **Conventions** are probed
in the checkout: well-known names a project carries for its own sake. One
precedence resolves them all — *site configuration over manifest over
probe* (§REQ-001-boundary.2) — so a probe is a default, a manifest is a
declaration, and the person always has the last word.

### 2. The manifest is offered, never required

A project that chooses to speak places one file, `ephor.json`, at its forest
root (decided in §DF-001-manifest-offered). It may declare identity hints,
the forest's own layout, check verbs, gate verbs, task stores, and
offers — every field optional, an empty
manifest valid, and nothing in it able to gate a capability that probing or
site configuration could not establish alone. Identity fields are hints the
registry row adopts unless it overrides: **the row is authoritative, because
attribution keys must not be forgeable by a checkout.** Manifest commands
run with exactly the trust a person extends to running the project's own
build, and the row can narrow it — honoring the manifest fully, reading only
its descriptions, or ignoring it — for checkouts trusted less. Offers are
menu entries a person invokes; what spends agent time on its own match (a
recipe) is site configuration only, because a repository does not get to
spend it.

### 3. A summons: environment in, exit code and answer out

Every command the interface names — a check verb, a gate verb, a checkout, a
task store's CLI, an offer — is invoked one way. It runs in the resolved
place: the item's branch workspace where one resolves, the forest root
otherwise, a manifest-designated repository of the forest where the entry
says so. It receives the dossier as `EPHOR_*` environment — one vocabulary,
identical to what a shell action and a state-machine script receive
(§FS-005-dispatch.8). It answers first with its exit code — `0` for done,
non-zero for failed, `75` for *parked*: not applicable now, ask again later —
and optionally in structure, written to the file named by `$EPHOR_ANSWER`.
Standard output and error remain the command's own, streamed to the person
or the log; a contract that parsed them would make every honest build log a
protocol violation.

**Paths in that environment are spelled so the shell can read them.** The
command is invoked through a shell, so `$EPHOR_ANSWER` and the rest are
strings that shell parses before anything opens them — and where a platform's
native spelling separates directories with the shell's own escape character, a
path handed over verbatim stops being a path: the redirect lands somewhere
else, or nowhere, and the answer comes back empty with nothing saying why,
which is the silence §REQ-001-boundary.1 refuses. Path-valued variables use
`/` between their segments on every platform. This costs nothing where the two
spellings already agree, and a command that only passes a path on to another
program sees no difference either way; the place the command runs in is set by
ephor rather than spelled to the shell, so it is not involved.

### 4. The answer envelope

Structured answers share one envelope, speaking the model's nouns
(§FS-007-matters): `matters`, `discussions`, and `events` for sources with
something to report; `summary`, `url`, and `needs_response` for the common
one-line cases; `failures` and `features` as verb-level conveniences that
ephor normalizes into events and facts; `data` as free passthrough that
returns wherever the dossier's metadata goes. Each verb's contract names the
fields it reads and ignores the rest, and unknown fields are ignored
everywhere — the envelope evolves by addition, and an incompatible change is
a version bump with a changelog entry (§11). Paths in an answer resolve
against the summons's working directory.

### 5. Checks are verbs, and every script is self-contained

A project's checks are three well-known names probed at the forest root —
`./check.sh` the aggregate, `./check-style.sh` the fast style pass,
`./smoke-test.sh` the smoke — or the same three declared in the manifest
under whatever paths the project prefers. Each is self-contained: a smoke
test that needs a build performs its build, because how a project builds is
the project's knowledge and stays there. Smoke may enumerate **features** —
`--list` printing one id per line, or a static list in the manifest — and a
feature id given as an argument runs that feature's smoke alone; without
enumeration, smoke is one opaque verb and that is a complete
implementation. Which verbs run, and in what order, is policy above the
interface: a verify step sequences them from site configuration, one
summons each.

### 6. The gate is the project's, in three verbs

How to ask a project's CI what it is doing is project truth — the same for
every person who works on it — so its home is the manifest, with site
configuration overriding where credentials or variants demand. Three verbs:
**status** answers the gate's counts per repository of the forest;
**failures** answers what actually failed, as the failing job, its log, and
the error where it can be had — the expensive question, asked on demand
(§FS-001-forge-interface.1); **restart** re-runs the gate,
committing nothing, at the scope it is asked for — everything the gate covers,
or only what is not green
([§FS-004-quick-actions.9](#9-a-gate-is-offered-the-restart-in-two-shapes)) —
and, where only what is not green is asked for, that means the failing gate
and every gate downstream of it, under the semantics of §FS-005-dispatch.11.
The scope crosses the seam as part of the ask, because the cheap re-run and
the expensive one are different questions and only the caller knows which was
meant. A forge-hosted gate needs no manifest at all: the
provider's own gate capability is the shipped default binding. A project
with an internal gate binds three commands, and nothing above the seam can
tell the difference.

### 7. The project's own tasks are read where they live

A project may keep its own work in its checkout — a plan directory, a
git-backed issue store — and a **task store** ephor recognizes is read
through the store's own files and CLI, as matters with their discussions
(§FS-007-matters), into the same feed under the same rules as anything a
forge reported. Recognition is by probed convention or manifest
declaration; attribution is the checkout's own project; and the stores are
project-native things that exist without ephor — a store's presence is a
capability rung, never an obligation.

The word is **task**, and it is one name for one thing
(§FS-001-forge-interface.3): a *ticket* is what a remote tracker keys, an
*issue* is what a forge files, and these are neither — they are the
project's own work, written down in the project's own checkout. So the row
they land on is **Tasks** (§FS-003-feed-categories.1), the rung is *tasks*
(§10), and the manifest key is `tasks`. What is not one of these keeps its
own name: the ticket ephor writes to dispatch work (§FS-005-dispatch.3) and
the ticket keys a forge is asked for (§FS-001-forge-interface.1) are other
things and are called what they are.

**A task in a final state is not read.** Final is the store's own word: what
its state machine declares final, or — where the store declares no machine —
what the runtime's built-in default machine declares, since that is what the
store's own tasks actually run under. Such a task is history the store keeps,
not news the feed carries: it has no activity time of its own beyond its
file's, so under §FS-003-feed-categories.2 every finished task in a plan would
resurface each time the plan was touched, and a store that is the record of a
project's work would drown the feed in its record. The store keeps the finished
work and answers for it; the feed shows what is open. A store whose machine
cannot be read is a store that did not answer, exactly like a plan that cannot
be read (§FS-001-forge-interface.6).

**Where they live is per branch, where a project has branch workspaces.**
Work about a change belongs in that change's working tree
(§FS-005-dispatch.3), so a branch-addressable project keeps a store per
workspace rather than one at the forest root. Both places are read: the root,
for a project whose checkout is its root, and every branch workspace on disk
that holds one, for a project whose branches have trees of their own. Reading
only the root leaves such a project writing its work into a place it never
looks again — work dispatched, plans on disk, and a feed that shows none of
it.

Verified on disk rather than derived from the registry row. The row names the
branches somebody wrote down; the stores are wherever branches were actually
checked out, and the two are not the same list.

**A workspace ephor makes gets a store.** Where ephor creates a branch
workspace (§FS-004-quick-actions.7) it initializes one there, so the first
dispatch into that branch has somewhere to land and what is under way is
visible from the moment the tree exists. This is not an artifact required of
the project and does not bend §REQ-001-boundary.3: the store ignores itself,
so what it holds is ephor's own planning state that happens to live in a
checkout, never content the project carries. A project cloned without ephor
is byte-for-byte what it was.

**The runtime makes its own project; ephor says where.** What a runtime
project consists of is the runtime's answer, so ephor asks the runner for one
rather than writing the runner's files out of a copy of that answer compiled
into ephor — two answers to one question drift the first time the runner
changes its mind, and the reader who then runs the runner by hand is the one
who finds out. What ephor supplies is the directory: the work root it already
resolves for this workspace, so a person who moved the work root gets the
project there rather than wherever the runner would have put it left to
itself. Ephor's own state machine (§FS-005-dispatch.6) is installed beside
what the runner wrote, and the self-ignore above stays ephor's whatever the
runner's project would say about version control — the paragraph above is a
promise about the checkout, and the runner has made none. For the same reason
the runner is asked not to leave its own discovery note beside the project: a
note in the checkout's `AGENTS.md` is a change to the branch, tracked or
untracked, and ephor said there would be none. Where the runner is not on the
machine, ephor writes the store it can and says what it could not do: a
checkout that failed because a convenience did is a checkout that did not need
to fail.

### 8. The checkout contract

A project may bind one **checkout** command; its contract is to make
`$EPHOR_WORKSPACE` exist, and ephor verifies the directory afterwards
rather than trusting the exit code alone. Where none is bound, ephor
supplies the git checkout itself (§FS-004-quick-actions.7). Everything that
needs a workspace — offers marked as requiring one, work whose recipe edits
the change — is gated on this contract and degrades by naming it.

### 9. Offers: the project's actions

A manifest may offer actions: entries for the same menu configured actions
occupy, in the same shape, selected by the same `when` language recipes use,
and gated by the same capability requirements. Provenance orders the menu —
what ephor itself recognized first (§FS-004-quick-actions.3), then the
project's offers, then the person's own — and where two entries share an id,
the person's beats the project's beats the shipped one. An offer is invoked
by a person, runs as a summons (§3), and is refused with its reason where
its requirements do not hold (§FS-004-quick-actions.2). It takes the reader's
terminal while it runs, which is what lets an offer be a pager or an editor —
and an offer that needs none of that says so, and runs beneath the screen as a
job instead (§FS-005-dispatch.17). An offer that is such a program and should
not take the terminal says `window` instead, and runs in a window of the
reader's own where one is bound — still something that takes a person, now
beside ephor rather than in its place (§FS-005-dispatch.22).

**An offer may name a workflow instead.** Where the runtime carries whole
workflows of its own — parameterized plans that lay down tasks of their own —
an entry naming one is the same entry, in the same menu, under the same
provenance, and what it answers that workflow's inputs with is written beside
the rest of it
([§FS-005-dispatch.19](#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)).
The manifest is one of the three places such an entry may be written, for the
reason it holds the others: which of the runtime's workflows are worth offering
here, and about what, is the project's to say.

**Who does an action is the project's to default.** Work that needs judgment
goes to a hand (§FS-005-dispatch.14), and `work.hands` maps an action's id to
the one that does it — `{ "default": "sonnet", "rebase": "luna:high",
"fix-gate": "gpt-5:high" }` — with `default` answering for every id the table
does not name. The id is the menu's own, so an offer, a configured action and
a recipe are all named the same way and a project learns no second vocabulary.
The table exists per project and at site level, because the alternative — one
hand for everything — is either the deep hand on every trivial replay or the
cheap one on the conflict that actually needed judgment, and that choice is
being made today anyway, silently, by whatever the runtime would pick.

A hand is named `<hand-id>[:<effort>]`, both the roster's own words
(§FS-005-dispatch.14). The long form `{ "agent", "model", "effort" }` stays
legal for a pair the runtime's registry never enumerated — a proxy serving a
model it does not list — and is **accepted with a note, never refused**: ephor
cannot prove such a pair invalid, and refusing it would make ephor's
configuration a smaller world than the runtime's, which sends the person back
to configuring the runtime directly and defeats the table. A name the roster
does list is checked against it: an effort the hand does not declare is
refused with the ones it does, and a name carrying no effort is settled by
what the hand declares — completed, refused, or asked plainly
(§FS-005-dispatch.14).

The project's table is read before the site's, each narrow before broad: this
action's id, then `default`, then the site's the same way. That is the middle
of the seven steps §FS-005-dispatch.14 sets, and neither end moves — what the
reader picked for this dispatch alone still displaces every table, and what
nobody chose at all is still the runtime's to pick unasked.

**A project may narrow the roster.** `work.permitted_hands` lists the hands
that may be used on it at all, which is what a repository under a policy about
which models may see its code needs. A hand outside the list is refused with
that reason wherever it was named — the project's own table, the site's, the
pin an action carries, the reader's choice at the moment of asking — and never
silently dropped or silently replaced, because a policy that fails quietly is
indistinguishable from a typo and the person would learn which it was from the
other side. The check is against the name, not against the roster, so it holds
with no runtime bound too. A hand spelled out in full is refused under a
narrowing for the same reason a name outside it is: nothing in the list
authorized it. What a narrowing cannot bind is the runtime's own unasked pick,
so a project that narrows and names no `default` is told that much.

Absence is the ordinary case: with no table anywhere nobody is named and the
runtime picks exactly as it does now. With no runtime bound there is no roster
to name a hand from, so a configured hand resolves to nothing and says so in
the *workable* rung's own words (§10) rather than failing the dispatch — the
ticket is written as it would have been, because who does the work is not what
makes a ticket (§FS-005-dispatch.4).

### 10. Capability, rung by rung

What a project can do is resolved into a ladder, and every feature names
the rungs it needs: *observable* (a registry row and at least one source
answering) buys the watch; *placed* (the forest root on disk) buys actions
and update; *branch-addressable* (a workspace template) buys resolution of
matters to workspaces; *checkout-able* (§8) buys work that edits;
*checkable* (§5) buys verification that means something; *gated* (§6) buys
failure dossiers and the restart; *tasks* (§7) buys the project's
own tasks as matters — a `requires` naming either of its older spellings,
*ticketed* or *local-issues*, goes on meaning it;
*workable* (a bound runtime, §FS-005-dispatch) buys the loop. A missing
rung degrades exactly the features that named it, with the reason stated
where the feature would have appeared — never an error, never silence
(§REQ-001-boundary.1).

### 11. The interface is versioned

The manifest, the envelope, and the registry schema are published schemas,
embedded in the binary and printable on demand, so a project can validate
what it says without ephor present. They evolve by addition: an optional
field costs nothing, unknown fields are ignored, and any incompatible
change bumps the schema version with a changelog entry per
§FS-002-release.1. The schemas are the interface's stability surface — what
a release may change is answerable by diffing them.

## FS-007-matters: the feed is made of matters, and a matter knows why it is there

The unit of the watch is the **matter**: the subject under discussion or
observation — a pull request, an issue, a task in the project's own
store, a periodic build, a status subject, or a bare topic. What the spec has so far
called an item is a matter seen through one source's report. A matter is
the feed's row, the unit of attribution, of state, of fingerprinting, and
of dispatch — the dossier is the dossier of a matter — and the reason for
the noun is that the same matter is discussed in more than one place: the
pull request's review threads, a mail thread about it, a chat fragment
naming it. One subject, several venues, one row (§GOAL-002-glance).

### 1. A matter is a subject with a stated identity

A matter's identity is the subject key its source stated — the pull request
the forge names, the ticket by its key, the store's own id — or, where a
conversation matched a project but no known subject, an identity synthesized
for it as a topic. Identity is never guessed from resemblance: two pull
requests may share a title, and a subject whose identity cannot be
established is left alone (§FS-003-feed-categories.5).

### 2. Same subject, one matter; related subjects, linked matters

Reports of the same subject key merge into one matter, however many sources
made them, under the survival rules of §FS-003-feed-categories.5. Matters
that *reference* each other — the pull request implementing a ticket, the
project's own task tracking a gate — stay distinct and are **linked**, presented
together under the branch that relates them. Merging what is one thing and
linking what is related is the difference between a readable pile and a
lossy one.

### 3. A discussion is messages grouped in a channel

A matter's conversation arrives as **discussions**: ordered messages with
authors, times, reactions, and task boxes, grouped within one channel.
Whether a discussion awaits the reader is decided per discussion, by the
calculus of §FS-003-feed-categories.4, identically in every channel. A
matter awaits its reader while any of its discussions does.

### 4. A channel says what it can do

The venue a discussion lives in — review threads, an issue's comments, a
mail thread, a chat thread — declares its capabilities in the pattern of
§FS-001-forge-interface.1: whether a reaction can be posted, a task ticked,
a reply sent. What grouping means is the channel's own policy; what the
reader can do about a message is offered only where the channel declared it
(§FS-004-quick-actions.2) — an undeclared capability narrows the offer by
the degrade rule of §REQ-001-boundary.1, never silently.

### 5. An event moves state, and resurfacing names its reason

Everything about a matter that is not conversation arrives as **events**:
the gate's counts changed, the state closed, a check finished, a ticket
resolved. Events fold into the matter's state, and the matter's fingerprint
digests state, discussions, and the event tail — so when a done matter
moves and resurfaces (§FS-005-dispatch.5), the row can say *what* moved:
resurfacing is always accompanied by its reason, because a row that
reappears without one sends the reader to re-read everything, which is the
sweep this tool exists to retire (§GOAL-003-nothing-lost).

## FS-008-attribution: every conversation finds its project, or says that it could not

Conversations arrive from places that know nothing of the registry: a
mailbox serves every project a person has, a discussion sits on an adjacent
repository, a notice names a subject nobody configured. Attribution is
ephor's own move — deciding whose business a conversation is — and it is
data matching, never code: evidence the conversation carries against
identity the registry declares (§GOAL-003-nothing-lost).

### 1. Identity is declared, and the row has the last word

A project's identity is the set of signals by which its matters are
recognized: ticket patterns, the forest's repositories, the wider
**territory** the project claims — repositories and organizations that are
its business without being in its forest — names and aliases, addresses. It
lives in the registry row; a manifest may hint it
(§FS-006-project-interface.2), and the row adopts or overrides — a checkout
must not be able to claim another project's conversations. Territory is what
places the general case: a mention of the person on some repository of the
project's ecosystem, an issue filed there, a discussion opened there —
none of it in any forest, all of it the project's business
(§GOAL-003-nothing-lost).

### 2. Two stages, one engine

Attribution runs discussion → matter, then matter → project and branch. It
is one matching engine at two scopes: the branch matching that already
places items under a project's branches is this engine confined to one
project, and it is promoted, not duplicated.

**A project's branches are the row's and the disk's together.** The row names
the branches somebody wrote down; the workspaces are wherever branches were
actually checked out, and the two are not the same list — the same gap task
stores are read across (§FS-006-project-interface.7). A branch whose
workspace is on disk is one ephor can place work on, so it is one items are
placed under, whether or not the row names it. Anything else is ephor
contradicting itself about a fact it measured: a row reading `✓ checked out`
under a heading that says the item is linked to no branch, and — worse — a
checkout ephor made itself (§FS-004-quick-actions.7) staying invisible to the
grouping the moment after it was made.

A branch found this way is named by the directory it was found in, never by
what its checkout has at `HEAD`, so the directory a branch resolves to and
the directory it was found in are always the same one. The row keeps the last
word on everything else about a branch — its ticket, whether it is active,
whether it is a release branch — and on identity, which no checkout may widen
([§1](#1-identity-is-declared-and-the-row-has-the-last-word)).

### 3. Venue beats reference beats resemblance

A discussion *on* a subject belongs to that subject's matter. A discussion
*naming* a subject — a ticket key in a mail's text, a pull request's URL in
a chat message — belongs to the named matter, linked onward
(§FS-007-matters.2). Only where neither holds may declared aliases place a
conversation, and then as a topic matter, never onto an existing subject:
resemblance may start a new row, it may not amend one. At the second stage
the venue itself is the explicit signal: a matter whose subject sits on a
repository of a project's forest or declared territory
(§FS-008-attribution.1) is that project's before any reference or alias is
consulted.

### 4. Unattributed is a place, not a fate

A conversation that matched nothing lands in a visible unattributed bucket,
in the interactive view and on demand — never dropped. The bucket is the
attribution seam's degrade rule (§REQ-001-boundary.1): mapping failures are
seen where they can be fixed, by adding the signal the identity was
missing.

## FS-009-shipped-actions: what ephor ships for CI runs from the repository alone

ephor ships continuous-integration entry points — workflow steps a project
wires into its own CI — and every one of them obeys the rule that selects
them: **a shipped step runs from repository-committed material and workflow
inputs alone**, never from a personal site. A step that would need a
registry, credentials for a person's sources, or a person's bindings does
not ship as CI; the watch-and-work loop stays on machines that have a site,
and shipping it hosted would mean shipping someone's configuration
(§REQ-001-boundary.2).

### 1. The set

Three steps ship. **Validate** checks a repository's `ephor.json` — and a
committed registry, where a repository carries one — against the published
schemas (§FS-006-project-interface.11). **Check** reads the manifest's
declared check verbs and runs them, per-feature where features are
enumerated (§FS-006-project-interface.5) — the project's own gate derived
from the project's own declaration, with nothing project-specific in the
workflow. **Setup** installs a pinned ephor release, and is the building
block the other two and anyone's own composition stand on.

### 2. Versioned and released with ephor

The steps live in ephor's repository and version with it: a release that
changes a schema or a verb ships the steps that understand the change, per
§FS-002-release. A repository pins the version it consumes, as it pins any
dependency (§GOAL-005-costless).

## FS-010-doctor: ephor can be asked whether it still works, and answers in one screen

The watch is only worth having if it is believed when it says there is
nothing to do (§GOAL-003-nothing-lost). Everything that makes that claim
false is quiet: a credential that expired, a forge whose extension left
`PATH`, a checkout somebody deleted, a runner that a system upgrade removed.
None of them announces itself — each one simply makes a section of the feed
empty, which is the one thing an empty section must never mean
(§FS-001-forge-interface.6).

So ephor can be **asked**. `doctor` answers "is this still working" for the
whole site in one run, and it is built to be run on a timer by whoever wants
one: it needs no argument, it prints what is wrong and what would fix it,
and it says so in its exit code.

### 1. It reports what is already judged, and judges nothing itself

What a project can do is the ladder of §FS-006-project-interface.10, and why
a rung is missing is a sentence that already exists. What a source did is
§FS-001-forge-interface.6's answer, with its own split between a
configuration to go and fix and a network to wait out. `doctor` composes
those two and adds no opinion of its own.

This is the whole of the design rule. A second opinion about whether a
project is checkout-able would be a diagnosis that drifts from the one the
menu refuses with, and a reader holding two answers has none: the sentence
`doctor` prints and the sentence a greyed entry shows are the same sentence.

### 2. The ladder is answerable on its own

The same table is worth having without a sweep. **`capabilities`** prints one
project's rungs — held, and missing with their reasons — so that "why is this
action not offered here" is a question with a cheap answer. `doctor`'s first
pass is this for every configured project.

### 3. Two passes: the site, and ephor itself

**The site pass** asks the world: the registry parses, every project's row
resolves, each ladder is computed, and every configured source is asked —
refreshed rather than read from cache, since a cached answer cannot say
whether a source still answers. What it reports is per project, and a
project that is entirely well says so in one line.

**The self pass** asks the binary. It builds a throwaway project — its own
state directory, registry, configuration and checkout, in a temporary place —
and walks the seams end to end against it: a forge reached out of process
(§FS-001-forge-interface.2), a refresh that categorizes what came back, a
summons answering by exit code and by envelope (§FS-006-project-interface.3),
a check verb bound by manifest and by probe (§FS-006-project-interface.5),
the git operations both a key and a state machine run
(§FS-004-quick-actions.6, §FS-004-quick-actions.7), a dispatch that writes a
plan and reads its ledger back (§FS-005-dispatch.4), and a task store
read where it lives (§FS-006-project-interface.7).

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

### 4. Nothing of the reader's is written

The site pass refreshes and reads; the self pass writes only inside the
temporary place it made and removes it afterwards. `doctor` posts nothing,
dispatches nothing, and changes no checkout of the reader's — a diagnostic
that repaired things as it went would be a second thing to debug, and one
that could not be run while unsure is one nobody runs.

### 5. The answer is in the exit code

A timer reads exit codes, not screens. `0` is well. `4` is degraded — a rung
missing or a source lost, which is the condition a person acts on. `3` is
nothing on the site reachable at all. `1` is the self pass failing, which is
ephor itself being wrong rather than the world being away, and is the one
answer that does not improve by waiting.

## FS-011-command-line: every ability is a command, and every answer has a JSON form

The interface is one way to reach the watch; the command line is the other,
and neither is a subset of the other (§REQ-002-parity). What follows is the
command-line half of abilities that were reachable only by keystroke, and
the rule that governs the machine form of every answer.

### 1. What may be done here, listed and run

`ephor actions` prints the menu a matter carries: the source's own offers,
ephor's, the project's, the person's, the work that can be handed over, and
the workflows laid down beside it — one list, in provenance order, exactly
the list the interface shows (§FS-006-project-interface.9,
§FS-005-dispatch.1). Each row says its id, what it is, whether it can run,
and where it cannot, the sentence that says why — the ladder's own, never a
second opinion (§FS-006-project-interface.10). A
branch is asked the same question and answers with ephor's own offers about
it (§FS-004-quick-actions.6).

`ephor actions run <id>` runs one. It resolves the same working directory,
exports the same context as `EPHOR_*` (§FS-004-quick-actions.1), honours the
same `confirm` (§FS-006-project-interface.9) —
which on the command line is `--yes`, since a second keystroke has no meaning
where there is no first one — and checks out the branch workspace first where
the entry needs one and it is not there (§FS-004-quick-actions.7). An entry
that hands work over dispatches it, with `--hand` picking who for this
dispatch alone (§FS-005-dispatch.14); one that lays a workflow down lays it,
with `--set` answering its inputs (§FS-005-dispatch.19). `--command` is the
freehand row: whatever the reader wants to run once, in the resolved place,
with the dossier already exported (§FS-005-dispatch.10).

### 2. Branches, and where each one stands

`ephor branches` prints what the registry knows about a project's branches
and what the disk says about them: whether the workspace is there, how far
each trails its main branch and how far it stands from its own published
copy (§DA-003-upstream-is-the-published-copy), how fresh that comparison is,
and how many of the project's matters are on it. This is the reading behind
every branch row, and it is what makes `rebase` and `checkout` usable without
first opening a screen to learn which branch to name.

### 3. The board is answerable without the screen

`ephor operations` prints the operations board (§FS-005-dispatch.15): every
execution root found by looking, each live run, each claim, each queued
ticket, each parked question, and the jobs ephor is running itself
(§FS-005-dispatch.17). `ephor job list` remains the jobs alone; this is both
halves in the order the board puts them, because "what is going on" has one
answer and a reader should not have to know which half of the machine is
doing it.

### 4. A conversation, and the moves inside it

`ephor thread <item>` prints a matter's recorded conversation — every
message, its author, when it arrived, what is on it, and the reply a run
drafted where one is waiting (§FS-005-dispatch.13).

The moves inside a conversation are commands as well as keys: `ephor react`
posts a reaction, `ephor tick` resolves a task the source reported
(§FS-004-quick-actions.5), and `ephor reply` sends the drafted reply, or a
reply given in words, where the channel declares that it can carry one
(§FS-007-matters.4). Each refuses by name where the source cannot carry the
move, which is the same sentence the key answers with.

### 5. What can be done about a matter, before anything has been

`ephor work offers` prints what could be handed over about one matter and
what already has been: the recipes that match it, the workflows that could
be laid beside it, the tickets that exist with their states and verdicts, and
what ephor has run about it itself. `ephor work list` remains the ledger
across everything; this is one matter's work screen (§FS-005-dispatch.4).

### 6. The gate, spelled out

`ephor failures` answers about a matter by its feed id as well as by the four
coordinates a quick action passes it, and prints the forge's own reasons for
refusing the merge beside what failed (§FS-001-forge-interface.1). `ephor
restart` takes the same id.

### 7. `--json` is the same answer, not a second one

Every command that prints a reading takes `--json` (§REQ-002-parity.3).
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
(§GRUND-001-overseer.2).

Each shape is declared, and `ephor schema views` prints the document holding
every one of them (§FS-006-project-interface.11) — every reading, and every
outcome a move reports. A field is added freely; renaming or removing one is a
release note (§REQ-002-parity.4). The declaration is held to what the commands
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
run under, which is the runtime's own language (§FS-005-dispatch.11). It is not
declared by reference, because ephor knows something about it the document does
not say: *which* machine is in force, the one ephor ships or one this site
configured. So the machine rides in a field of a shape of ephor's own, beside
where it came from. A reader who got only the document could not tell the two
apart, and they are different answers to "what states can a ticket be in".


### 8. What is going is said, and the way in is printed

`ephor actions` marks every entry that has work going about its subject
(§FS-005-dispatch.21) with the same facts the screen sets apart: what is
running — the job, the run, or the window — since when, and what it is at. And
it prints **the way in**, because the way in is the ability and spawning the
reader's own program on it is not (§REQ-002-parity.1): a job's log path, a
run's id and the runner's own attach command in the runner's own words, its
control address while it serves one, a window's handle. `ephor operations`
prints the same for every live root, beside the runner's own command for
stopping it (§FS-005-dispatch.20) — shown, never run.

`ephor actions open <id>` is the key on a running row
(§FS-005-dispatch.21) as a command: it follows the log, attaches to the run,
or brings the window forward, by the same binding the key uses
(§FS-005-dispatch.22), and refuses by name where the entry has nothing going.
`ephor operations attach <run>` is the same move from the board's side, where
the row is an execution root and not an entry (§FS-005-dispatch.15): every key
the board holds is a command too (§REQ-002-parity.2), and a run somebody
started in another terminal is reachable by its id alone.

`ephor work run` starts the runtime detached and prints the run's id
(§FS-005-dispatch.20); `--watch` keeps the terminal and watches the run as
before, which is also what a runner that cannot detach does unasked, saying so.
