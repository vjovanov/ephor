#!/usr/bin/env python3
"""Render docs/manual.md as a standalone HTML page.

The manual is one document: this only wraps it. Everything readable comes from
the markdown, so the page cannot drift from the manual the repository carries —
regenerate rather than edit the output.

    python3 scripts/manual-page.py [OUT]     # default: target/manual.html

Needs `pandoc` for the markdown, and nothing at run time: the page has no
external stylesheet, script, or font, so it renders wherever it is opened.
"""

import html
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANUAL = ROOT / "docs/manual.md"

# The glyph vocabulary of the inbox, which the page shows as a legend. Kept
# beside the palette it is coloured from, since the two only mean anything
# together.
LEGEND = [
    ("!", "fail", "needs a response"),
    ("*", "wait", "unread"),
    ("✓", "pass", "passed / checked out"),
    ("✗", "fail", "failed"),
    ("⋯", "wait", "still running"),
    ("⊘", "fail", "the forge refuses"),
    ("⚙", "work", "work under way"),
    ("⟳", "wait", "the item moved on"),
]

STYLE = """
:root {
  color-scheme: light dark;
  --ground: #f2f3ef;
  --surface: #fbfbf9;
  --sunk: #eceee8;
  --ink: #191e1c;
  --muted: #5f6b67;
  --rule: #d7dcd4;
  --patina: #1b6b5a;
  --patina-soft: #e2ece7;
  --pass: #4c7a22;
  --fail: #b23a2e;
  --wait: #8a6412;
  --work: #2b5f86;
  --serif: ui-serif, "Iowan Old Style", "Palatino Linotype", Palatino,
           "Book Antiqua", Georgia, serif;
  --mono: ui-monospace, SFMono-Regular, Menlo, "DejaVu Sans Mono", Consolas,
          monospace;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --ground: #14181a;
    --surface: #1b2124;
    --sunk: #101315;
    --ink: #e5e9e3;
    --muted: #98a39e;
    --rule: #2b3337;
    --patina: #63c1a7;
    --patina-soft: #1c2b28;
    --pass: #8fbf5f;
    --fail: #e2796a;
    --wait: #d3a850;
    --work: #7fb1da;
  }
}
:root[data-theme="dark"] {
  --ground: #14181a;
  --surface: #1b2124;
  --sunk: #101315;
  --ink: #e5e9e3;
  --muted: #98a39e;
  --rule: #2b3337;
  --patina: #63c1a7;
  --patina-soft: #1c2b28;
  --pass: #8fbf5f;
  --fail: #e2796a;
  --wait: #d3a850;
  --work: #7fb1da;
}

* { box-sizing: border-box; }

body {
  margin: 0;
  background: var(--ground);
  color: var(--ink);
  font-family: var(--serif);
  font-size: 17px;
  line-height: 1.66;
  -webkit-font-smoothing: antialiased;
}

.page {
  max-width: 1180px;
  margin: 0 auto;
  padding: 0 clamp(1.1rem, 4vw, 3rem) 6rem;
}

/* ── masthead ─────────────────────────────────────────────────────────── */
.masthead {
  padding: clamp(3rem, 9vw, 6rem) 0 2.4rem;
  border-bottom: 1px solid var(--rule);
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
}
.eyebrow {
  font-family: var(--mono);
  font-size: .72rem;
  letter-spacing: .16em;
  text-transform: uppercase;
  color: var(--patina);
}
.masthead h1 {
  margin: 0;
  font-size: clamp(2.4rem, 6vw, 4rem);
  font-weight: 600;
  letter-spacing: -.015em;
  line-height: 1.05;
  text-wrap: balance;
}
.masthead .greek { font-style: italic; color: var(--muted); }
.masthead p {
  margin: 0;
  max-width: 62ch;
  font-size: 1.08rem;
  color: var(--muted);
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: .45rem .9rem;
  margin-top: .6rem;
  font-family: var(--mono);
  font-size: .78rem;
}
.legend span { color: var(--muted); white-space: nowrap; }
.legend b { font-weight: 600; margin-right: .3rem; }
.legend .pass { color: var(--pass); }
.legend .fail { color: var(--fail); }
.legend .wait { color: var(--wait); }
.legend .work { color: var(--work); }

/* ── frame ────────────────────────────────────────────────────────────── */
.frame { display: grid; grid-template-columns: 1fr; gap: 3rem; }
@media (min-width: 1000px) {
  .frame { grid-template-columns: 210px minmax(0, 68ch); gap: 4.5rem; }
}

.rail { font-family: var(--mono); font-size: .8rem; }
@media (min-width: 1000px) {
  .rail {
    position: sticky;
    top: 0;
    align-self: start;
    max-height: 100vh;
    overflow-y: auto;
    padding: 3rem 0;
  }
}
.rail ol { list-style: none; margin: 0; padding: 0; display: grid; gap: .5rem; }
.rail a {
  color: var(--muted);
  text-decoration: none;
  display: grid;
  grid-template-columns: 1.6em 1fr;
  gap: .2rem;
  line-height: 1.35;
}
.rail a:hover, .rail a:focus-visible { color: var(--patina); }
.rail .n { color: var(--patina); }
.rail > .eyebrow { margin-bottom: 1rem; display: block; }
details.contents { padding: 2rem 0 0; }
details.contents summary { font-family: var(--mono); font-size: .8rem; color: var(--patina); cursor: pointer; }
@media (min-width: 1000px) { details.contents { display: none; } }

/* ── prose ────────────────────────────────────────────────────────────── */
.prose { padding-top: 3rem; min-width: 0; }
.prose > * + * { margin-top: 1.1rem; }
.prose h2 {
  margin: 3.6rem 0 0;
  padding-top: 1.1rem;
  border-top: 1px solid var(--rule);
  font-size: 1.75rem;
  font-weight: 600;
  letter-spacing: -.01em;
  line-height: 1.2;
  text-wrap: balance;
}
.prose h2 .n {
  font-family: var(--mono);
  font-size: .72rem;
  letter-spacing: .16em;
  color: var(--patina);
  display: block;
  margin-bottom: .45rem;
}
.prose h3 {
  margin: 2.3rem 0 0;
  font-size: 1.12rem;
  font-weight: 600;
  letter-spacing: -.005em;
  text-wrap: balance;
}
.prose h3 .n { font-family: var(--mono); color: var(--patina); font-size: .95em; margin-right: .45em; }
.prose h1 { display: none; }
.prose p { max-width: 68ch; }
.prose strong { font-weight: 600; }
.prose a { color: var(--patina); text-decoration-thickness: 1px; text-underline-offset: 2px; }
.prose ul, .prose ol { max-width: 68ch; padding-left: 1.3rem; }
.prose li + li { margin-top: .35rem; }
.prose hr { border: 0; border-top: 1px solid var(--rule); margin: 3rem 0; }
.prose blockquote {
  margin: 0; padding-left: 1.1rem;
  border-left: 2px solid var(--patina);
  color: var(--muted);
}

code, kbd, pre, tt { font-family: var(--mono); }
.prose :not(pre) > code {
  font-size: .86em;
  background: var(--patina-soft);
  border-radius: 3px;
  padding: .1em .34em;
  overflow-wrap: break-word;
}
.prose pre {
  background: var(--sunk);
  border: 1px solid var(--rule);
  border-radius: 4px;
  padding: 1rem 1.1rem;
  overflow-x: auto;
  font-size: .82rem;
  line-height: 1.55;
}
.prose pre code { background: none; padding: 0; font-size: inherit; }

.table-scroll { overflow-x: auto; }
.prose table {
  border-collapse: collapse;
  width: 100%;
  font-size: .92rem;
  font-variant-numeric: tabular-nums;
}
.prose th {
  text-align: left;
  font-family: var(--mono);
  font-size: .7rem;
  letter-spacing: .12em;
  text-transform: uppercase;
  font-weight: 600;
  color: var(--muted);
  padding: 0 1rem .5rem 0;
  border-bottom: 1px solid var(--patina);
  white-space: nowrap;
}
.prose td {
  padding: .5rem 1rem .5rem 0;
  border-bottom: 1px solid var(--rule);
  vertical-align: baseline;
}
.prose td:first-child { white-space: nowrap; }
.prose td code { white-space: nowrap; }

footer {
  margin-top: 5rem;
  padding-top: 1.4rem;
  border-top: 1px solid var(--rule);
  font-family: var(--mono);
  font-size: .78rem;
  color: var(--muted);
}
footer a { color: var(--patina); }

:focus-visible { outline: 2px solid var(--patina); outline-offset: 2px; }
@media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
"""


def body_html() -> str:
    """The manual, as HTML, with GitHub's heading ids so its own links work."""
    return subprocess.run(
        ["pandoc", "-f", "gfm", "-t", "html", str(MANUAL)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout


def sections(markup: str):
    """The numbered H2s, for the rail."""
    for match in re.finditer(
        r'<h2 id="([^"]+)">\s*(\d+)\.\s*(.*?)</h2>', markup, re.S
    ):
        yield match.group(1), match.group(2), re.sub(r"<[^>]+>", "", match.group(3))


def main() -> None:
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "target/manual.html")
    markup = body_html()

    # The section number becomes its own line above the heading: the manual
    # cross-references these numbers, so they are content and not decoration.
    markup = re.sub(
        r'(<h2 id="[^"]+">)\s*(\d+)\.\s*',
        lambda m: f'{m.group(1)}<span class="n">§ {m.group(2)}</span>',
        markup,
    )
    markup = re.sub(
        r'(<h3 id="[^"]+">)\s*(\d+\.\d+)\s*',
        lambda m: f'{m.group(1)}<span class="n">{m.group(2)}</span> ',
        markup,
    )
    # Wide content scrolls inside itself rather than widening the page.
    markup = markup.replace("<table>", '<div class="table-scroll"><table>')
    markup = markup.replace("</table>", "</table></div>")
    # The rail replaces the markdown's own contents list.
    markup = re.sub(
        r'<h2 id="contents">.*?</ol>', "", markup, count=1, flags=re.S
    )

    rail = "\n".join(
        f'<li><a href="#{anchor}"><span class="n">{number}</span>'
        f"<span>{html.escape(title)}</span></a></li>"
        for anchor, number, title in sections(body_html())
    )
    legend = "\n".join(
        f'<span><b class="{tone}">{glyph}</b>{label}</span>'
        for glyph, tone, label in LEGEND
    )

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        f"""<title>The ephor manual</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>{STYLE}</style>
<div class="page">
  <header class="masthead">
    <span class="eyebrow">Manual · ephor</span>
    <h1>A watch over every project<br><span class="greek">ἔφορος</span></h1>
    <p>Every command, key, and configuration field — the whole surface, in one
       document.</p>
    <div class="legend">{legend}</div>
  </header>
  <div class="frame">
    <nav class="rail" aria-label="Sections">
      <span class="eyebrow">Contents</span>
      <ol>{rail}</ol>
    </nav>
    <main class="prose">{markup}
      <footer>
        Generated from <code>docs/manual.md</code> ·
        <a href="https://github.com/vjovanov/ephor">github.com/vjovanov/ephor</a>
      </footer>
    </main>
  </div>
</div>
""",
        encoding="utf-8",
    )
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
