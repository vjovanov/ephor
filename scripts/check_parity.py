#!/usr/bin/env python3
"""Hold the two surfaces to parity, mechanically (§REQ-002-parity.5).

§REQ-002-parity says every ability the interactive interface offers is also a
command, and that every reading a command gives back is also available as JSON.
The list of abilities and the command each one carries lives in
``src/api/parity.rs``; a test there holds every command it names to the actual
command tree. This script holds the other end, in three passes:

1. Every key the interface binds is either an ability on that list or an
   exemption on its presentation list. Every ``.rs`` file under the interface
   is read, however deep it sits, and a key counts whether it is spelled out
   as ``KeyCode::Delete`` or written bare as ``Delete`` under a ``use`` that
   brought that name into scope — the prefix is punctuation, the binding is
   the same.
2. Every ability's command actually takes ``--json``, asked of the built
   binary rather than of anybody's memory (§REQ-002-parity.3). A move added to
   both surfaces with no machine form is half an ability, and it is exactly the
   half a runtime needs.
3. A binding this script cannot read is reported rather than skipped. A check
   that silently ignores what it does not understand is a check that grows
   blind spots, and a blind spot in a parity gate is a key with nothing behind
   it shipping green. A ``use …KeyCode::*`` that names every variant without
   writing one down, a key decided by a constant, and a chord whose modifier
   is written in front of its code are all said out loud here rather than
   guessed at.

A key added to a screen with nothing behind it on the command line therefore
fails the build, rather than being noticed by a reader who went looking for
the command and did not find one. That is the same reasoning that makes
§REQ-001-boundary.5 a build failure instead of a review comment.

Run it directly (``python3 scripts/check_parity.py``) or through ``just
check``; CI runs it too, beside the boundary and grund checks. It needs the
binary, which both of those build first; run alone in a fresh tree it builds
one itself.

Adding a key: give it an entry in ``ABILITIES`` with the command that carries
it, or — if it only moves a cursor, changes a mode, or hands the reader's own
pager, editor or browser something the command line already names
(§REQ-002-parity.1) — an entry in ``PRESENTATION`` saying so. Either edit is
one a reviewer should argue with, which is the point of writing it down.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PARITY = ROOT / "src" / "api" / "parity.rs"
SURFACE = ROOT / "src" / "feed" / "tui"

# Keys that are structural rather than bound: leaving a screen, going back,
# and the digits that pick a numbered row are the same gesture as Enter on it,
# which the list already covers.
STRUCTURAL = {"esc", "enter", "tab", "backspace", "space", "1-9"}

# The digits `1-9` names, one by one, for the screen that binds `Char('1')`
# outright rather than through a guard. Written as the nine of them rather
# than as "a digit", because `0` picks no numbered row: a binding on it is a
# key like any other and is owed a command, and waving it through with its
# neighbours was the exemption reaching a key it was never written for.
NUMBERED_ROWS = {str(row) for row in range(1, 10)}

# A guard naming one of these says the arm is the numbered rows, which
# STRUCTURAL already covers as `1-9`. Anything else guarding a bound character
# is a key this script cannot read, and it says so rather than passing it.
DIGIT_GUARDS = ("is_ascii_digit", "is_numeric", "is_digit")


def strip_noise(text: str, keep_chars: bool = True) -> str:
    """`text` with comments and string literals blanked, offsets preserved.

    The key scan runs over this: a `{` inside a doc comment or a message must
    not close a module, and the word `KeyCode` in a string is prose about a
    binding rather than one. Character literals are kept — they are what a
    binding *is*.

    `keep_chars=False` blanks those as well, and the brace matcher counts over
    that instead. One text cannot serve both: a `KeyCode::Char('{')` is a
    binding to the scan and an unbalanced brace to the matcher, and reading
    both out of the same characters let one key literal blank a file.
    """
    out = list(text)
    at, end = 0, len(text)
    while at < end:
        rest = text[at:]
        if rest.startswith("//"):
            stop = text.find("\n", at)
            stop = end if stop == -1 else stop
            for index in range(at, stop):
                out[index] = " "
            at = stop
            continue
        if rest.startswith("/*"):
            stop = text.find("*/", at + 2)
            stop = end if stop == -1 else stop + 2
            for index in range(at, stop):
                if out[index] != "\n":
                    out[index] = " "
            at = stop
            continue
        raw = re.match(r'r(#*)"', rest)
        if raw:
            fence = '"' + raw.group(1)
            stop = text.find(fence, at + raw.end())
            stop = end if stop == -1 else stop + len(fence)
            for index in range(at, stop):
                if out[index] != "\n":
                    out[index] = " "
            at = stop
            continue
        if rest.startswith('"'):
            index = at + 1
            while index < end and text[index] != '"':
                index += 2 if text[index] == "\\" else 1
            for blank in range(at, min(index + 1, end)):
                if out[blank] != "\n":
                    out[blank] = " "
            at = index + 1
            continue
        # A character literal — kept whole, or blanked for the brace matcher.
        # Told from a lifetime by what follows it: `'a'` closes, `'a` does not.
        char = re.match(r"'(\\.|[^\\'])'", rest)
        if char:
            if not keep_chars:
                for index in range(at, at + char.end()):
                    out[index] = " "
            at += char.end()
            continue
        at += 1
    return "".join(out)


def without_tests(text: str, braces: str) -> str:
    """`text` with every `mod tests { … }` block blanked out.

    Cut by matching braces rather than to the end of the file. Truncating at
    the first test module meant everything after it was never scanned at all,
    so a binding appended below the tests — or written in a second module
    after them — passed by sitting in the part nobody read.

    The matching counts over `braces`, the same text with character literals
    blanked as well: counting over the text the scan reads, which keeps them,
    meant a `KeyCode::Char('{')` inside a test left the depth one deep to the
    end of the file and blanked everything after it — reinstating exactly the
    blind spot the matching was written to close. `text` is what is masked and
    returned, so the scan still sees the literals it is looking for.
    """
    masked = list(text)
    for module in re.finditer(r"\bmod\s+tests\s*\{", braces):
        depth, at = 0, module.end() - 1
        while at < len(braces):
            if braces[at] == "{":
                depth += 1
            elif braces[at] == "}":
                depth -= 1
                if depth == 0:
                    at += 1
                    break
            at += 1
        for index in range(module.start(), at):
            if masked[index] != "\n":
                masked[index] = " "
    return "".join(masked)


# A `use` statement, up to the `;` that ends it.
USE_ITEM = re.compile(r"\buse\s[^;]*;")
# What a `use` item takes out of `KeyCode`: a braced group, a glob, or one
# name, each of which may be renamed with `as`.
VARIANT_IMPORT = re.compile(
    r"KeyCode::(?:\{(?P<group>[^}]*)\}|(?P<glob>\*)"
    r"|(?P<one>[A-Z][A-Za-z0-9_]*(?:\s+as\s+[A-Za-z_][A-Za-z0-9_]*)?))"
)
IMPORTED_NAME = re.compile(
    r"(?P<variant>\*|[A-Z][A-Za-z0-9_]*)"
    r"(?:\s+as\s+(?P<alias>[A-Za-z_][A-Za-z0-9_]*))?"
)


def imports(text: str) -> tuple[dict[str, str], list[str], str]:
    """The `KeyCode` variants a file's `use` items name, and `text` without them.

    A file that writes `use …event::KeyCode::{Char, Delete};` and then matches
    on `Char('D')` and `Delete` binds exactly what a file spelling the prefix
    out binds, and a scan that only knew the spelled-out form read none of it.
    So the names are taken out of the imports first, mapped from whatever `as`
    called them to the variant they are.

    The `use` items are then blanked, offsets preserved, because a `use` lists
    names rather than binding them: left in place, the `Char` inside the
    braces would be counted as a key the file presses.
    """
    scope: dict[str, str] = {}
    globs: list[str] = []
    masked = list(text)
    for item in USE_ITEM.finditer(text):
        for taken in VARIANT_IMPORT.finditer(item.group(0)):
            body = taken.group("glob") or taken.group("group") or taken.group("one")
            for name in IMPORTED_NAME.finditer(body):
                if name.group("variant") == "*":
                    # A glob names every variant without writing one of them
                    # down, so the bare bindings under it cannot be listed
                    # ahead of the scan. Reported rather than passed: what a
                    # check cannot read it says (§REQ-002-parity.5).
                    globs.append(" ".join(item.group(0).split()))
                    continue
                scope[name.group("alias") or name.group("variant")] = name.group("variant")
        for index in range(item.start(), item.end()):
            if masked[index] != "\n":
                masked[index] = " "
    return scope, globs, "".join(masked)


def binding(scope: dict[str, str]) -> re.Pattern[str]:
    """The binding shapes to look for in a file its imports gave `scope`.

    `KeyCode::Char('x')`, `KeyCode::Char(name)`, and a named variant —
    `KeyCode::Delete`, `KeyCode::F(5)`. All three are bindings; only the first
    was ever looked for, so a screen that bound Delete bound it invisibly. So
    is any of them written bare, which is the same key with the prefix left
    off, and is why the pattern is built per file rather than once.
    """
    ways = [r"KeyCode::(?P<qualified>[A-Z][A-Za-z0-9_]*)"]
    if scope:
        # Longest first so that `PageDown` is not read as `Page`; and never
        # straight after `::` or a word character, where `KeyCode::Char` is
        # the arm above already and `Chars` is a different word entirely.
        names = "|".join(sorted(map(re.escape, scope), key=len, reverse=True))
        ways.append(rf"(?<![A-Za-z0-9_:])(?P<bare>{names})\b")
    return re.compile(
        rf"(?:{'|'.join(ways)})"
        # What it is given, if anything. The character literal is tried first,
        # so that `Char(')')` is read as the key it binds rather than as an
        # argument that stopped at the first `)` it saw.
        r"(?:\(\s*(?:'(?P<literal>\\?.)'|(?P<argument>[^)]*?))\s*\))?"
    )


MODIFIER = re.compile(r"KeyModifiers::(CONTROL|ALT|SHIFT|SUPER|META)\b")
# `--json` where clap lists an option, with or without a short form beside it.
OPTION_JSON = re.compile(r"^\s+(?:-\w,\s+)?--json\b", re.M)
# Spelled the way a reader would say it, which is how the lists name keys.
CHORD = {"control": "ctrl", "meta": "super"}
# What closes the arm above: whatever sits between one of these and a binding
# belongs to the binding's own pattern.
ARM_EDGES = ("=>", "{", "}", ";")


def arm(clean: str, start: int, stop: int) -> str:
    """The match arm's head: from the line the binding sits on to its `=>`.

    Where the binding is an `if` rather than an arm — `key.code == …` — the
    head ends at the block it opens instead. This is the text a modifier and a
    guard are read out of, so that `ctrl+u` is not filed as `u`.
    """
    line = clean.rfind("\n", 0, start) + 1
    fat = clean.find("=>", stop)
    brace = clean.find("{", stop)
    ends = [end for end in (fat, brace, stop + 240) if end != -1]
    return clean[line : min(ends)]


def pattern_before(clean: str, start: int) -> str:
    """The binding's own pattern text that sits in front of it.

    `arm` reads forward from the line the binding is on, which is where a
    guard puts its modifier. A `KeyEvent { modifiers: …, code: … }`
    destructure and a `match (key.modifiers, key.code)` tuple put it in front
    instead, and spread over lines it lands where reading forward never goes.
    Read back to the nearest `=>`, brace or `;`: those end the arm above, so
    what is left over belongs to this one and to no neighbour.
    """
    edges = []
    for mark in ARM_EDGES:
        at = clean.rfind(mark, 0, start)
        if at != -1:
            edges.append(at + len(mark))
    return clean[max(edges, default=0) : start]


def sample(text: str) -> str:
    """One line of `text`, whitespace closed up, for a report to quote."""
    shown = " ".join(text.split())
    return shown if len(shown) <= 120 else shown[:117] + "…"


def bound() -> tuple[dict[str, list[str]], list[str]]:
    """Every key the interface binds, by file, and the bindings it cannot read.

    Test bodies are left out: a test that presses a key is exercising a
    binding this check already read from the code that declares it.
    """
    found: dict[str, list[str]] = {}
    unreadable: list[str] = []
    # Every depth, not just this one directory: a screen filed under
    # `board/keys.rs` binds keys exactly as one beside `mod.rs` does, and a
    # flat glob never opened it. Named by its path from the root rather than
    # by its bare name, because two directories may each hold a `keys.rs` and
    # a report naming only the last part says nothing about which.
    for path in sorted(SURFACE.rglob("*.rs")):
        where = path.relative_to(ROOT).as_posix()
        text = path.read_text()
        clean = without_tests(strip_noise(text), strip_noise(text, keep_chars=False))
        scope, globs, clean = imports(clean)
        for glob in globs:
            unreadable.append(
                f"  {where}: `{glob}` names every variant at once, so the keys"
                " bound bare under it are ones this check cannot read"
            )
        for match in binding(scope).finditer(clean):
            head = arm(clean, match.start(), match.end())
            ahead = set(MODIFIER.findall(head))
            behind = set(MODIFIER.findall(pattern_before(clean, match.start())))
            if behind - ahead:
                # The chord is named in front of the code, out of reach of the
                # forward read — and a `ctrl+u` filed as `u` is a bound key
                # passing under a listed one's name, which is the quietest way
                # of all to ship a key with nothing behind it. Widening the
                # read backwards would as easily borrow the arm above's
                # modifier and fail a key that *is* listed, so this reports
                # the shape rather than guessing at what it binds.
                unreadable.append(
                    f"  {where}: `{sample(match.group(0))}` is reached with"
                    f" {'+'.join(sorted(behind)).lower()} named before it, which this"
                    " check cannot read as one key"
                )
                continue
            names = match.groupdict()
            variant = names["qualified"] or scope[names["bare"]]
            argument = (names["argument"] or "").strip()
            modifiers = sorted(
                {CHORD.get(name.lower(), name.lower()) for name in ahead}
            )
            prefix = "".join(f"{name}+" for name in modifiers)
            if variant == "Char" and names["literal"] is not None:
                key = names["literal"]
                found.setdefault(prefix + ("space" if key == " " else key), []).append(
                    where
                )
            elif variant == "Char" and re.fullmatch(r"[a-z_][A-Za-z0-9_]*", argument):
                if any(word in head for word in DIGIT_GUARDS):
                    # The numbered rows, which `1-9` on the list covers.
                    found.setdefault("1-9", []).append(where)
                elif " if " in head:
                    # A key decided by a constant or a call. Whatever it is, it
                    # is not readable here — and a binding this check cannot
                    # name is a binding it cannot hold to the list.
                    unreadable.append(
                        f"  {where}: `{head.strip()}` binds a key this check cannot read"
                    )
                # A bare `KeyCode::Char(ch)` arm with no guard binds no
                # particular key: it is a line being typed into, which is text
                # entry rather than an ability (§REQ-002-parity.1).
            elif variant == "Char":
                # `Char(SOME_KEY)`, or a bare `Char` standing in a path: a
                # character named by something this check cannot resolve.
                unreadable.append(
                    f"  {where}: `{sample(match.group(0))}` binds a key this"
                    " check cannot read"
                )
            else:
                # `F(5)` is one key and is named as one; `F(n)` is not.
                if argument and not argument.isdigit():
                    unreadable.append(
                        f"  {where}: `{sample(match.group(0))}`"
                        " binds a key this check cannot read"
                    )
                    continue
                found.setdefault(prefix + variant.lower() + argument, []).append(where)
    return found, unreadable


def block(source: str, const: str) -> str:
    """The body of one ``const`` array of ``parity.rs``."""
    start = source.index(f"pub const {const}")
    return source[start : source.index("\n];", start)]


def abilities(source: str) -> tuple[set[str], set[str]]:
    """The keys the two lists name: owed a command, and exempt.

    Read out of the `keys:` arrays and the presentation pairs rather than out
    of every quoted string in the block. Case matters — `R` runs the runtime
    and `r` refreshes, and a check that folded them together would let either
    arrive with nothing behind it — and so does *which* string is taken: an
    ability's `command` is a word too, and counting it as a key would let a
    binding named `delete` pass because some command happens to be called
    that.
    """
    owed: set[str] = set()
    for keys in re.finditer(r"keys:\s*&\[([^]]*)\]", block(source, "ABILITIES")):
        owed.update(re.findall(r'"([^"]+)"', keys.group(1)))
    exempt: set[str] = set()
    for pair in re.finditer(
        r'\(\s*"([^"]+)"\s*,', block(source, "PRESENTATION")
    ):
        exempt.add(pair.group(1))
    return owed, exempt


def commands(source: str) -> list[tuple[str, str]]:
    """Each ability as `(what, command)`, in the order the list writes them."""
    found = []
    for entry in re.finditer(
        r'what:\s*"([^"]*)".*?command:\s*"([^"]*)"', source, re.S
    ):
        found.append((entry.group(1), entry.group(2)))
    return found


def binary() -> Path:
    """The built binary, whose own `--help` is asked what flags exist.

    Read from the command tree rather than from `src/cli.rs`, because what the
    check has to hold is what a reader can actually type.
    """
    for build in ("debug", "release"):
        path = ROOT / "target" / build / "ephor"
        if path.is_file():
            return path
    subprocess.run(
        ["cargo", "build", "--quiet", "--locked"], cwd=ROOT, check=True
    )
    return ROOT / "target" / "debug" / "ephor"


def machine_forms(ephor: Path, listed_commands: list[tuple[str, str]]) -> list[str]:
    """Abilities whose command does not take `--json` (§REQ-002-parity.3).

    Every reading a command prints is also available as JSON, and a move
    prints what it changed the same way. An ability that reached the command
    line without one is the degraded surface §GRUND-001-overseer.2 says a
    script must never be — and it is invisible to the key check, which only
    ever asked whether *a* command existed.
    """
    problems = []
    for what, command in listed_commands:
        path = [word for word in command.split() if not word.startswith("-")]
        help_text = subprocess.run(
            [str(ephor), *path, "--help"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if help_text.returncode != 0:
            problems.append(
                f"  `ephor {' '.join(path)}` does not resolve: {help_text.stderr.strip()}"
            )
            continue
        # The flag as clap lists it, never the word wherever it appears: a
        # help text that *mentions* `--json` in a description would otherwise
        # answer for a command that does not take it.
        if not OPTION_JSON.search(help_text.stdout):
            problems.append(
                f"  '{what}' is carried by `ephor {' '.join(path)}`, which takes no --json"
            )
    return problems


def main() -> int:
    if not PARITY.is_file():
        print(f"no parity list at {PARITY}", file=sys.stderr)
        return 1
    source = PARITY.read_text()
    owed, exempt = abilities(source)
    known = owed | exempt | STRUCTURAL | NUMBERED_ROWS
    keys, unreadable = bound()
    problems = []
    for key, files in sorted(keys.items()):
        if key in known:
            continue
        where = ", ".join(sorted(set(files)))
        problems.append(
            f"  '{key}' is bound in {where} and is on neither list in src/api/parity.rs"
        )
    failed = False
    if problems or unreadable:
        failed = True
        print(
            "Keys the interface binds that no command answers "
            "(§REQ-002-parity.5):",
            file=sys.stderr,
        )
        print("\n".join(problems + unreadable), file=sys.stderr)
        print(
            "\nAdd each one to ABILITIES with the command that carries it, or to "
            "PRESENTATION if it only moves a cursor or opens the reader's own "
            "pager, editor or browser on something a reading already names. A "
            "binding this check cannot read is spelled with the key it binds, so "
            "that it can be.",
            file=sys.stderr,
        )
    missing = machine_forms(binary(), commands(source))
    if missing:
        failed = True
        print(
            "\nAbilities whose command has no machine form (§REQ-002-parity.3):",
            file=sys.stderr,
        )
        print("\n".join(missing), file=sys.stderr)
        print(
            "\nEvery reading a command prints is also available as JSON, and a "
            "move prints what it changed the same way. Add `--json` to the "
            "command's arguments and pass it through to the outcome.",
            file=sys.stderr,
        )
    if failed:
        return 1
    print(
        f"parity: {len(owed)} abilities, {len(exempt)} presentation keys, "
        f"{len(keys)} bindings accounted for, every command has --json"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
