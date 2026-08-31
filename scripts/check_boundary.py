#!/usr/bin/env python3
"""Hold the source to the boundary law, mechanically (§REQ-001-boundary.5).

Two checks, both over the Rust sources git tracks under ``src/``:

1. **No product literal outside its adapter.** The name of a forge, vendor
   CLI, runtime, or task store appears only in its own adapter. The law
   permits it in shipped assets, in examples, and in documentation, so this
   check reads only ``src/``, skips comments, and skips ``#[cfg(test)]``
   bodies -- inline, or a whole ``<name>_tests.rs`` sibling one of them has
   moved to: a doc-comment that spells ``gh:acme/widget#42`` is documenting
   the grammar, and a fixture that builds a ``github-prs`` row is an example.

2. **The core layer is IO-free.** Core is the innermost layer of
   §AR-001-layers.1: it depends on nothing above it and touches no filesystem,
   process, or socket. Verified by module structure -- the ``crate::`` paths a
   core module names must themselves be core, and no IO API may appear -- so
   that "core compiles without the rest" is a property of the tree rather
   than a claim in a comment.

Run it directly (``python3 scripts/check_boundary.py``), or through
``just check``; CI runs it too, beside the grund check.

Adding an adapter: give its literal an entry in PRODUCTS below, whose homes
are the files that are allowed to spell it. Adding a home to an existing
product is the same edit, and it is the edit a reviewer should argue with --
which is the point of keeping the list here rather than in each file.
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass
from fnmatch import fnmatch


# --------------------------------------------------------------------------
# 1. No product literal outside its adapter (§REQ-001-boundary.5)
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class Product:
    """One product name, and the files allowed to spell it."""

    name: str
    pattern: str
    homes: tuple[str, ...] = ()
    #: What the name belongs to, for the error message.
    kind: str = "product"


PRODUCTS = (
    # The forge ephor implements natively, and its vendor CLI. `gh` is matched
    # only as a word or an identifier stem so that "through" and "eight" are
    # not forge references.
    Product(
        "github",
        r"github",
        homes=("src/feed/providers/github*.rs", "src/feed/providers/mod.rs"),
        kind="forge",
    ),
    Product(
        "gh",
        r"\bgh\b|\bgh[-_]|[-_]gh\b|\bGH_[A-Z]",
        homes=("src/feed/providers/github*.rs", "src/feed/providers/mod.rs"),
        kind="vendor CLI",
    ),
    # No adapter, so no home: a forge ephor does not implement is reached
    # through the forge interface, never by name (§FS-001-forge-interface.2).
    Product("gitlab", r"gitlab", kind="forge"),
    Product(
        "slack",
        r"slack",
        homes=("src/feed/providers/slack.rs", "src/feed/providers/mod.rs"),
        kind="chat vendor",
    ),
    Product(
        "discord",
        r"discord",
        homes=("src/feed/providers/discord.rs", "src/feed/providers/mod.rs"),
        kind="chat vendor",
    ),
    # The shipped runtime and the directory its projects live in
    # (§AR-007-runtime), and the plan store read out of a checkout.
    Product(
        "rhei",
        r"rhei",
        homes=("src/work/runtime/*.rs", "src/seams/tasks.rs"),
        kind="runtime",
    ),
    Product(
        "panta",
        r"panta",
        homes=("src/work/runtime/*.rs", "src/seams/tasks.rs", "src/work/recipe.rs"),
        kind="runtime",
    ),
    Product("beads", r"beads", homes=("src/seams/tasks.rs",), kind="task store"),
    # The three windows ephor ships a binding for (§FS-005-dispatch.22,
    # §DA-007-window-is-a-bound-opener). Each product's name, and the variable
    # it sets for exactly this purpose, live in the opener and nowhere else --
    # a fourth binding is a pair of commands in configuration, not an edit here.
    Product(
        "tmux",
        r"tmux",
        homes=("src/seams/window.rs",),
        kind="terminal multiplexer",
    ),
    Product(
        "wezterm",
        r"wezterm",
        homes=("src/seams/window.rs",),
        kind="terminal",
    ),
    Product(
        "kitty",
        r"kitty",
        homes=("src/seams/window.rs",),
        kind="terminal",
    ),
)


@dataclass(frozen=True)
class Debt:
    """A literal that is still where it should not be, and why it is here.

    Every entry is a migration that has not happened yet, pinned to one file
    and one spelling so that the rest of that file stays under the law. An
    entry that stops matching is an error, not a leftover: the debt was paid
    and the exemption goes with it.
    """

    path: str
    product: str
    #: Only text this matches is excused -- not the whole file.
    pattern: str
    why: str


LEDGER = (
    Debt(
        "src/feed/config.rs",
        "github",
        r"github_user",
        "`defaults.github_user` is a site-configuration key, so moving it "
        "into the source's own block is a schema change with a migration",
    ),
    Debt("src/feed/provider.rs", "github", r"github_user", "carries the key above"),
    Debt("src/feed/refresh.rs", "github", r"github_user", "carries the key above"),
    Debt("src/forge/mod.rs", "github", r"github_user", "carries the key above"),
)


# --------------------------------------------------------------------------
# 2. The core layer is IO-free (§AR-001-layers.1)
# --------------------------------------------------------------------------

#: Today's core, as module paths. §AR-001-layers.3 is explicit that the layers
#: are a target the tree migrates toward: this list grows as modules split
#: their pure halves out, and it never shrinks. `forest` is core by
#: §AR-001-layers.1 and not yet core by structure -- it asks the git prober
#: what is on disk, and joins this list when that prober moves to sources.
CORE = ("matter", "attribution", "ticket_ids", "feed::model", "feed::gate")

IO_APIS = (
    (r"\bstd::fs\b", "std::fs"),
    (r"\bstd::io\b", "std::io"),
    (r"\bstd::process\b", "std::process"),
    (r"\bstd::net\b", "std::net"),
    (r"\bstd::env\b", "std::env"),
    (r"\bCommand::new\b", "Command::new"),
    (r"\bFile::(?:open|create)\b", "std::fs::File"),
    (r"\b(?:reqwest|ureq|tokio)\b", "a network client"),
)

#: `crate::<path>`, stopping at the first capitalized segment, which is a type
#: rather than a module.
CRATE_PATH = re.compile(r"\bcrate::((?:[a-z_][a-z0-9_]*)(?:::[a-z_][a-z0-9_]*)*)")


# --------------------------------------------------------------------------
# Reading Rust: comments out, strings kept, test bodies marked
# --------------------------------------------------------------------------


@dataclass
class Line:
    number: int
    #: The line with comments blanked out; string literals survive, because a
    #: product name in a string is code.
    code: str
    #: Whether the line sits inside a `#[cfg(test)]` body.
    in_test: bool
    #: Braces opened minus braces closed by this line, counting only the ones
    #: that are scope. A brace inside a string or a comment is neither.
    delta: int = 0
    raw: str = ""


def read_rust(path: str) -> list[Line]:
    """Split a Rust file into lines, blanking comments and marking test bodies.

    Comments are documentation, which the law permits everywhere, and
    `#[cfg(test)]` bodies are examples, which it permits too. Strings are kept
    -- a product name in one is code -- but the braces inside them are not
    counted as scope, which is what makes the test-body tracking trustworthy.

    A `<name>_tests.rs` file is a test body in its entirety, so it is marked
    by name rather than by tracking braces -- see `is_out_of_line_tests`.
    """
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    out: list[Line] = []
    line_no = 1
    kept: list[str] = []
    delta = 0
    index = 0
    end = len(text)
    block_depth = 0

    def flush() -> None:
        nonlocal kept, delta
        out.append(Line(line_no, "".join(kept), False, delta))
        kept = []
        delta = 0

    def keep_literal(chunk: str) -> None:
        """Keep a string's text without letting its braces move the scope."""
        nonlocal line_no
        for piece_index, piece in enumerate(chunk.split("\n")):
            if piece_index:
                flush()
                line_no += 1
            kept.append(piece)

    while index < end:
        char = text[index]
        if char == "\n":
            flush()
            line_no += 1
            index += 1
            continue
        if block_depth:
            if text.startswith("/*", index):
                block_depth += 1
                index += 2
            elif text.startswith("*/", index):
                block_depth -= 1
                index += 2
            else:
                index += 1
            continue
        if text.startswith("//", index):
            newline = text.find("\n", index)
            index = end if newline < 0 else newline
            continue
        if text.startswith("/*", index):
            block_depth = 1
            index += 2
            continue
        raw_string = re.match(r'r(#*)"', text[index:])
        if raw_string:
            hashes = raw_string.group(1)
            close = '"' + hashes
            stop = text.find(close, index + len(raw_string.group(0)))
            stop = end if stop < 0 else stop + len(close)
            keep_literal(text[index:stop])
            index = stop
            continue
        if char == '"':
            stop = index + 1
            while stop < end:
                if text[stop] == "\\":
                    stop += 2
                    continue
                if text[stop] == '"':
                    stop += 1
                    break
                stop += 1
            else:
                stop = end
            keep_literal(text[index:stop])
            index = stop
            continue
        if char == "'" and is_char_literal(text, index):
            stop = index + 1
            while stop < end:
                if text[stop] == "\\":
                    stop += 2
                    continue
                if text[stop] == "'":
                    stop += 1
                    break
                stop += 1
            else:
                stop = end
            keep_literal(text[index:stop])
            index = stop
            continue
        if char == "{":
            delta += 1
        elif char == "}":
            delta -= 1
        kept.append(char)
        index += 1
    if kept or not out:
        flush()

    if is_out_of_line_tests(path):
        for line in out:
            line.in_test = True
    else:
        mark_test_bodies(out)
    raw_lines = text.split("\n")
    for line in out:
        if line.number <= len(raw_lines):
            line.raw = raw_lines[line.number - 1].strip()
    return out


def is_char_literal(text: str, index: int) -> bool:
    """Whether the quote at `index` opens a char literal rather than a lifetime.

    `'a` in `&'a str` is a lifetime and has no closing quote; consuming to the
    next quote would swallow whatever stands between two of them.
    """
    if text[index + 1 : index + 2] == "\\":
        return True
    return text[index + 2 : index + 3] == "'"


def is_out_of_line_tests(path: str) -> bool:
    """Whether the whole file is one module's test body, moved to a sibling.

    A source file that outgrows its size budget moves its inline
    `#[cfg(test)] mod tests` whole to `<name>_tests.rs` and attaches it with
    `#[cfg(test)] #[path = "<name>_tests.rs"] mod tests;` -- the first seam
    `.agents/fissile.toml` names for an oversized source file. The
    `#[cfg(test)]` then sits on the attachment in the parent, so the file
    carries no marker of its own and `mark_test_bodies` would read its
    fixtures as production code. The name is what says it, and every line of
    such a file is a test body.
    """
    return path.endswith("_tests.rs")


def mark_test_bodies(lines: list[Line]) -> None:
    """Set `in_test` on every line inside a `#[cfg(test)]` item."""
    depth = 0
    pending = False
    test_depth: int | None = None
    for line in lines:
        if test_depth is not None:
            line.in_test = True
        if re.match(r"\s*#\[cfg\(test\)\]", line.code):
            pending = True
        if pending and line.delta > 0:
            pending = False
            if test_depth is None:
                test_depth = depth
                line.in_test = True
        depth += line.delta
        if test_depth is not None and depth <= test_depth:
            test_depth = None


# --------------------------------------------------------------------------
# The checks
# --------------------------------------------------------------------------


@dataclass
class Finding:
    path: str
    number: int
    what: str
    raw: str


def sources() -> list[str]:
    listed = subprocess.run(
        ["git", "ls-files", "src"], capture_output=True, text=True, check=True
    ).stdout.split()
    return sorted(name for name in listed if name.endswith(".rs"))


def at_home(path: str, product: Product) -> bool:
    return any(fnmatch(path, home) for home in product.homes)


def check_literals(files: dict[str, list[Line]]) -> tuple[list[Finding], list[str]]:
    compiled = [(product, re.compile(product.pattern, re.I)) for product in PRODUCTS]
    debts = [(debt, re.compile(debt.pattern, re.I)) for debt in LEDGER]
    used: set[Debt] = set()
    findings: list[Finding] = []

    for path, lines in files.items():
        for product, pattern in compiled:
            if at_home(path, product):
                continue
            excuses = [
                (debt, expression)
                for debt, expression in debts
                if debt.path == path and debt.product == product.name
            ]
            for line in lines:
                if line.in_test:
                    continue
                for match in pattern.finditer(line.code):
                    excused = False
                    for debt, expression in excuses:
                        if any(
                            span.start() <= match.start() and span.end() >= match.end()
                            for span in expression.finditer(line.code)
                        ):
                            used.add(debt)
                            excused = True
                            break
                    if excused:
                        continue
                    homes = ", ".join(product.homes) or "no adapter here"
                    findings.append(
                        Finding(
                            path,
                            line.number,
                            f"names the {product.kind} '{product.name}' "
                            f"(belongs in {homes})",
                            line.raw,
                        )
                    )
                    break

    stale = [
        f"{debt.path}: '{debt.pattern}' ({debt.product})"
        for debt in LEDGER
        if debt not in used
    ]
    return findings, stale


def check_core(files: dict[str, list[Line]]) -> list[Finding]:
    findings: list[Finding] = []
    core_paths = {"src/" + module.replace("::", "/") + ".rs" for module in CORE}
    missing = sorted(core_paths - set(files))
    for path in missing:
        findings.append(Finding(path, 0, "is listed as core but is not in src/", ""))

    for path in sorted(core_paths & set(files)):
        for line in files[path]:
            if line.in_test:
                continue
            for pattern, api in IO_APIS:
                if re.search(pattern, line.code):
                    findings.append(
                        Finding(path, line.number, f"reaches {api}", line.raw)
                    )
                    break
            else:
                if "crate::{" in line.code:
                    findings.append(
                        Finding(
                            path,
                            line.number,
                            "uses a braced crate:: import this check cannot read",
                            line.raw,
                        )
                    )
                    continue
                for match in CRATE_PATH.finditer(line.code):
                    cited = match.group(1)
                    if not any(
                        cited == module or cited.startswith(module + "::")
                        for module in CORE
                    ):
                        findings.append(
                            Finding(
                                path,
                                line.number,
                                f"depends on crate::{cited}, which is not core",
                                line.raw,
                            )
                        )
                        break
    return findings


def report(findings: list[Finding], citation: str) -> None:
    sys.stdout.flush()
    print(f"error: {len(findings)} site(s) break the boundary:", file=sys.stderr)
    for finding in findings[:40]:
        where = f"{finding.path}:{finding.number}" if finding.number else finding.path
        print(f"  {where}: {finding.what}", file=sys.stderr)
        if finding.raw:
            print(f"      {finding.raw}", file=sys.stderr)
    if len(findings) > 40:
        print(f"  … and {len(findings) - 40} more", file=sys.stderr)
    print(f"       {citation}", file=sys.stderr)


def main() -> int:
    paths = sources()
    files = {path: read_rust(path) for path in paths}
    status = 0

    print("==> 1/2  no product literal outside its adapter")
    findings, stale = check_literals(files)
    if findings:
        report(findings, "§REQ-001-boundary.5, §AR-001-layers.2")
        status = 1
    if stale:
        print(
            "error: the migration ledger excuses what is no longer there; "
            "delete these entries from scripts/check_boundary.py:",
            file=sys.stderr,
        )
        for entry in stale:
            print(f"  {entry}", file=sys.stderr)
        status = 1
    if not findings and not stale:
        excused = len(LEDGER)
        carried = f", {excused} on the migration ledger" if excused else ""
        print(f"ok: {len(paths)} source file(s) keep every name in its adapter{carried}")

    print("==> 2/2  the core layer is IO-free")
    core = check_core(files)
    if core:
        report(core, "§AR-001-layers.1")
        status = 1
    else:
        print(f"ok: core ({', '.join(CORE)}) reaches nothing above it")

    return status


if __name__ == "__main__":
    sys.exit(main())
