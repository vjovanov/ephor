# FS-001-forge-interface: ephor reaches every forge and issue tracker through one provider interface

ephor aggregates work from places that host code review and places that track
issues. Which ones they are is a property of a person's employer, not of ephor.
No forge, tracker, or vendor CLI may therefore be named in ephor's core
([§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)): every one of them is reached through a single interface
with a fixed capability set, and an implementation is selected per project by
configuration.

## 1. Capabilities

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
  ([§FS-004-quick-actions.9](FS-004-quick-actions.md#9-a-gate-is-offered-the-restart-in-two-shapes)).
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
  ([§FS-003-feed-categories.4](FS-003-feed-categories.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).
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
  ([§FS-003-feed-categories.1](FS-003-feed-categories.md#1-the-categories)). Only what is open is asked
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
  [§FS-003-feed-categories.5](FS-003-feed-categories.md#5-one-subject-is-one-row-however-many-sources-reported-it).

## 2. Two transports, one interface

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

## 3. Policy lives above the interface, never in an implementation

An implementation answers questions about a forge. It does not decide what the
answers mean. Whether a citation was answered, whether an item needs a
response, which reported reason puts the user on which side of a pull request,
which of two reports of the same subject the reader sees, how threads and gate
counts roll up, how items match registry branches, and what counts as unread
are ephor's, applied identically to every implementation — so the feed stays
coherent across forges, and an implementation stays small enough to be a shell
script.

## 4. Site-specific implementations ship separately

An implementation for a private forge — such as one reaching an internal
Bitbucket Server, Jira, and Buildbot through a vendor CLI — lives outside the
default build and is neither a build-time nor a run-time dependency of it. Its
vendor CLI name, host names, project keys, and repository names are
configuration it reads, never identifiers in ephor's source.

## 5. No site-specific data in the repository

The registry and feed configuration a person runs ephor with is their own: it
names their employer's repositories, hosts, and accounts. The repository
carries example configuration only, and a published artifact must contain
nothing else.

*Not satisfied today* — see
[§RM-001-forge-interface](../roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface).

## 6. A source that did not answer says so, and says which kind of not

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

## 7. A fetch runs beneath the reading, never in front of it

Asking every source takes as long as the slowest of them, and that is not a
number ephor controls: an out-of-process forge reached over a VPN is allowed a
ceiling of its own (§2) precisely because the shared default is too short for
it. So the fetch is the slowest thing ephor does — minutes, where a source is
entitled to them — and the reader is doing something else while it runs:
scanning, opening a thread, marking work done.

An interactive refresh therefore runs **beneath** the interface. The screen
stays the reader's for the whole of it: every key still answers, and they may
read, act on a matter, and leave while sources are still being asked. A screen
frozen on a fetch is the failure [§FS-010-doctor.3](FS-010-doctor.md#3-two-passes-the-site-and-ephor-itself) names in the diagnostic — a
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
   screen collects every operation in one place ([§FS-005-dispatch.15](FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)), the
   run appears there *additionally* — the header line stays where it is,
   because this point is about progress on the screen being read, and the
   reader entitled to it is the one who never visits the board.
3. **The reader's place is kept.** Rows arriving under a moving cursor must
   not change what the next key would act on: a selection follows the matter
   it was on rather than the position that matter happened to occupy.

None of this changes what a refresh costs the forge ([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)):
moving the waiting off the reader's screen is not licence to ask more sources
at once than were being asked before it.

## 8. A refresh is asked in the cheapest form the forge offers

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
([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).

So the cost of a refresh is bounded by the question being asked, not by the
shape of the loop that asks it:

### 8.1 A role is not a request

Where a forge answers about several roles at once, it is asked once. The
reasons are still reported one by one
([§1](#1-capabilities)) — what collapses is the asking, never the answer. An
implementation that cannot recover the separate reasons from a combined answer
asks the separate questions instead: a reason is a claim about the reader's
involvement, and one that was not established is not reported.

### 8.2 The scarce meter is the last resort

Where the same question can be put to an endpoint the forge meters more
generously, it goes there. Two forms of one answer that differ only in which
meter they draw on are not a tradeoff, and spending the scarce one buys
nothing.

### 8.3 What is already in hand is not asked for again

A field that arrives with the answer is not re-fetched per result. The
per-result follow-up is where a refresh's cost stops being proportional to the
registry and starts being proportional to the reader's own work — the one
direction that punishes exactly the busiest reader.

None of this licenses asking for more than is needed: the cheap form of a
question is still only asked because the answer is read.

