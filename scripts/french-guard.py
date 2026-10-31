#!/usr/bin/env python3
"""Guard against French text in tracked files (#377).

The repository is written in English. This script reads every tracked text
file and flags lines that look French. Output: `path:line: excerpt`, then a
summary on stderr. Exit codes: 0 clean, 1 hits or an unclosed block, 2 usage
or git error.

Exemptions: see EXEMPT_* below, `lang: fr` markers (line, line above, block,
table columns), «...» spans, `...` spans, URLs.
"""

import argparse
import os
import re
import subprocess
import sys
import unicodedata
from pathlib import Path

# CHANGELOG sections `## [X.Y.Z]` up to this version are French and exempt.
FRENCH_CHANGELOG_UP_TO = (1, 0, 0)

EXEMPT_PREFIXES = ("docs/archive/", "test/import/")
EXEMPT_FILES = {
    ".claude/skills/stokesforge-bug-report/references/decision-form.html",
    # Live page source (the user reads French on it), republished to claude.ai.
    ".claude/skills/stokesforge-verification/references/to-test-page.html",
    "scripts/french-guard-test.py",
}
THRESHOLD = 2

# The word lists below are French on purpose; the guard must not flag itself.
# lang: fr begin
STOP_WORDS = frozenset(
    "le la les des du est sont pour avec dans sans une qui que cette ces mais "
    "donc aussi leur leurs être aux où à et il elle nous vous ce cet ou sur "
    "pas ne se au un de très peut doit".split()
)
# Accented French-looking words that are legitimate in English text.
ACCENT_ALLOWLIST = frozenset(
    "café naïve résumé déjà façade rôle créteil poincaré fréchet bézier "
    "protégé fiancé touché cliché entrée voilà crème".split()
)
# lang: fr end

ACCENT_WORD = re.compile(r"[a-zàâçéèêëîïôûùœ]+")
ACCENTS = re.compile(r"[àâçéèêëîïôûùœ]")
WORD = re.compile(r"\w+")
URL = re.compile(r"https?://\S+")
BACKTICKS = re.compile(r"`[^`]*`")
OPENER = r"(?://|#|<!--|/\*|;)"
MARK_LINE = re.compile(OPENER + r"\s*lang:\s*fr\b(?!\s+(?:begin|end|columns))")
MARK_ALONE = re.compile(r"^" + OPENER + r"\s*lang:\s*fr\s*(?:-->|\*/)?$")
MARK_BEGIN = re.compile(OPENER + r"\s*lang:\s*fr\s+begin\b")
MARK_END = re.compile(OPENER + r"\s*lang:\s*fr\s+end\b")
MARK_COLUMNS = re.compile(r"<!--\s*lang:\s*fr\s+columns:\s*(.*?)\s*-->")
CHANGELOG_HEAD = re.compile(r"^##\s*\[([^\]]+)\]")


def is_exempt_path(path):
    if path in EXEMPT_FILES or path.startswith(EXEMPT_PREFIXES):
        return True
    parts = path.split("/")
    for i in range(1, len(parts) - 2):
        if parts[i] == "i18n" and parts[i + 1] == "fr":
            return True
    return False


def strip_guillemets(line, inside):
    """Remove «...» spans. Returns (text, unclosed_at_end)."""
    out = []
    i = 0
    n = len(line)
    while i < n:
        if inside:
            j = line.find("»", i)
            if j < 0:
                return "".join(out), True
            i = j + 1
            inside = False
        else:
            j = line.find("«", i)
            if j < 0:
                out.append(line[i:])
                break
            out.append(line[i:j])
            i = j + 1
            inside = True
    return "".join(out), inside


def count_signals(text):
    text = BACKTICKS.sub(" ", URL.sub(" ", text))
    found = set()
    for m in WORD.finditer(text):
        tok = m.group()
        if not tok.isalpha():  # hex, identifiers with digits or underscores
            continue
        s, e = m.start(), m.end()
        before = text[max(0, s - 2):s]
        if before.endswith(".") or before.endswith("::") or text[e:e + 1] == "(":
            continue
        if tok in STOP_WORDS:
            found.add(tok)
            continue
        low = tok.lower()
        if (len(tok) >= 2 and ACCENT_WORD.fullmatch(low) and ACCENTS.search(low)
                and low not in ACCENT_ALLOWLIST):
            found.add(low)
    return len(found)


def version_of(label):
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", label.strip())
    return tuple(int(x) for x in m.groups()) if m else None


def blank_columns(lines, start, names, effective):
    """Blank the named columns of the table starting at `start`; return its end."""
    header = [c.strip().lower() for c in lines[start].strip().strip("|").split("|")]
    wanted = {n.strip().lower() for n in names.split(",") if n.strip()}
    idx = {i for i, h in enumerate(header) if h in wanted}
    k = start
    while k < len(lines) and lines[k].lstrip().startswith("|"):
        cells = lines[k].strip().strip("|").split("|")
        effective[k] = "|".join("" if i in idx else c for i, c in enumerate(cells))
        k += 1
    return k


def scan_text(path, content):
    """Return (hits, errors); hits are (lineno, excerpt)."""
    lines = [unicodedata.normalize("NFC", l) for l in content.splitlines()]
    effective = list(lines)
    skip = [False] * len(lines)
    errors = []
    is_changelog = path == "CHANGELOG.md"
    in_block = None
    changelog_exempt = False
    i = 0
    while i < len(lines):
        line = lines[i]
        if in_block is not None:
            skip[i] = True
            if MARK_END.search(line):
                in_block = None
            i += 1
            continue
        if MARK_BEGIN.search(line):
            in_block = i + 1
            skip[i] = True
        elif MARK_END.search(line):
            errors.append((i + 1, "'lang: fr end' without begin"))
            skip[i] = True
        elif MARK_LINE.search(line):
            skip[i] = True
        elif i > 0 and MARK_ALONE.match(lines[i - 1].strip()):
            skip[i] = True
        m = MARK_COLUMNS.search(line)
        if m and i + 1 < len(lines) and lines[i + 1].lstrip().startswith("|"):
            i = blank_columns(lines, i + 1, m.group(1), effective)
            continue
        if is_changelog:
            h = CHANGELOG_HEAD.match(line)
            if h:
                v = version_of(h.group(1))
                changelog_exempt = v is not None and v <= FRENCH_CHANGELOG_UP_TO
            if changelog_exempt:
                skip[i] = True
        i += 1
    if in_block is not None:
        errors.append((in_block, "unclosed 'lang: fr begin'"))
    hits = []
    inside = False
    for n, line in enumerate(effective):
        text, unclosed = strip_guillemets(line, inside)
        # A « left open carries to the next line only, never further.
        inside = unclosed and not inside
        if skip[n]:
            continue
        if count_signals(text) >= THRESHOLD:
            hits.append((n + 1, lines[n].strip()[:120]))
    return hits, errors


def tracked_files(root):
    try:
        out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"],
                             capture_output=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError) as err:
        print(f"french-guard: git ls-files failed: {err}", file=sys.stderr)
        sys.exit(2)
    return sorted(p for p in out.decode("utf-8", "replace").split("\0") if p)


def read_text(full):
    if os.path.islink(full) or not os.path.isfile(full):
        return None
    try:
        data = Path(full).read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    ap.add_argument("paths", nargs="*", help="restrict to these files or directories")
    args = ap.parse_args(argv)
    root = Path(args.root)
    only = [p.strip("/") for p in args.paths]
    problems = []
    checked = 0
    for path in tracked_files(root):
        if only and not any(path == o or path.startswith(o + "/") for o in only):
            continue
        if is_exempt_path(path):
            continue
        content = read_text(root / path)
        if content is None:
            continue
        checked += 1
        hits, errors = scan_text(path, content)
        problems += [(path, n, t) for n, t in hits]
        problems += [(path, n, "error: " + t) for n, t in errors]
    for path, n, text in sorted(problems):
        print(f"{path}:{n}: {text}")
    print(f"french-guard: {len(problems)} problem(s) in {checked} file(s)", file=sys.stderr)
    if problems:
        print(
            "Translate the text to English, or mark deliberate French: "
            "`lang: fr` after a comment opener (line, or alone on the line above), "
            "a begin/end block, a `columns:` table marker, or a span in guillemets. "
            "Details: docs/tests.md, \"French guard\".",
            file=sys.stderr,
        )
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
