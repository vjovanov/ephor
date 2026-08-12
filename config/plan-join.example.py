#!/usr/bin/env python3
"""Author the ticket that waits for every cause ticket in this plan.

A parent cannot wait for its children in rhei — `**Prior:**` points from a task
to its prerequisites, so the edge runs the wrong way, and a terminal parent may
not hold non-terminal descendants (§FS-rhei-plan-language.3.9). The join is
therefore a *peer* whose `**Prior:**` names every cause ticket, and it becomes
ready exactly when all of them have finished.

Written by a program rather than by the agent that opened the causes: the list
has to be complete and spelled correctly, and that is a job for something that
cannot improvise. It also copies the item's metadata onto the new ticket, since
`{meta.*}` belongs to the ticket ephor dispatched and a ticket the machine
opens for itself inherits none.

    0   a join ticket was appended
    3   no causes to wait for

Environment: RHEI_PLAN_PATH, plus STATE (the state the join starts in) and
PREFIX (which tickets to wait for, default `cause-`).
"""

import os
import re
import sys




def tickets(text):
    """(id, state, start, end) for every top-level ticket, fences skipped."""
    out, fence = [], None
    lines = text.split("\n")
    for index, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped[:3] in ("```", "~~~"):
            marker = stripped[0] * 3
            fence = None if fence else marker
            continue
        if fence:
            continue
        heading = re.match(r"### \w+ ([A-Za-z][\w-]*):", line)
        if heading:
            out.append([heading.group(1), None, index])
        elif out and out[-1][1] is None:
            state = re.match(r"\*\*State:\*\*\s*(\S+)", line.strip())
            if state:
                out[-1][1] = state.group(1)
    return out


def metadata_block(text, ticket):
    """The frontmatter entry for `ticket`, as its lines."""
    match = re.search(r"\n---\n(.*?)\n---\n", text, re.S)
    if not match:
        return []
    block, keep, out = match.group(1).split("\n"), False, []
    for line in block:
        if re.match(r"^    [\w-]+:\s*$", line):
            keep = line.strip().rstrip(":") == ticket
            continue
        if keep and line.startswith("      "):
            out.append(line)
        elif keep and line.strip():
            break
    return out


def main():
    plan = os.environ.get("RHEI_PLAN_PATH", "")
    if not plan or not os.path.isfile(plan):
        print(f"no plan at {plan!r}", file=sys.stderr)
        return 1
    state = os.environ.get("STATE", "land")
    prefix = os.environ.get("PREFIX", "cause-")
    me = os.environ.get("RHEI_TASK_ID_LOCAL", "")

    text = open(plan, encoding="utf-8").read()
    found = tickets(text)
    # Every cause, not only the unfinished ones: "wait for all of them" is the
    # requirement, and a prerequisite already in a successful terminal state
    # satisfies its edge immediately. One that ended `cancelled` does not, and
    # that is the point — nothing is pushed while a cause is unresolved.
    waiting = [id for id, _, _ in found if id.startswith(prefix)]
    if not waiting:
        print("no causes to wait for", file=sys.stderr)
        return 3

    used = {id for id, _, _ in found}
    number = 1
    while f"{state}-{number}" in used:
        number += 1
    new = f"{state}-{number}"

    # The item's own fields, from whichever ticket ephor wrote them onto.
    meta = []
    for id, _, _ in found:
        meta = metadata_block(text, id)
        if meta:
            break

    body = (
        f"\n### Task {new}: land the work — commit, push, and let the gate run again\n"
        f"**State:** {state}\n"
        f"**Prior:** {', '.join('Task ' + id for id in waiting)}\n\n"
        f"Waits for all {len(waiting)} cause ticket(s), then commits what they\n"
        f"left, pushes, and lets the forge re-run the gate on the result.\n\n"
        f"A cause that ends `cancelled` never satisfies a prerequisite, so this\n"
        f"does not run while any of them is unresolved — move that one on\n"
        f"deliberately, or close this plan.\n"
    )
    text = text.rstrip("\n") + "\n" + body

    if meta:
        text = re.sub(
            r"(\n  tasks:\n)",
            lambda m: m.group(1) + f"    {new}:\n" + "\n".join(meta) + "\n",
            text,
            count=1,
        )

    tmp = plan + ".join-tmp"
    with open(tmp, "w", encoding="utf-8") as handle:
        handle.write(text)
    os.replace(tmp, plan)
    print(f"{new} waits for: {', '.join(waiting)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
