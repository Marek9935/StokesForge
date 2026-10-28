#!/usr/bin/env python3
"""Contracts of scripts/french-guard.py. The fixtures below are French on purpose."""

import contextlib
import importlib.util
import io
import subprocess
import tempfile
import unicodedata
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("french_guard", Path(__file__).with_name("french-guard.py"))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

FR = "Le calcul est fait pour les colonnes avec des valeurs"


def hits(text, path="a.rs"):
    return guard.scan_text(path, text)[0]


class Detector(unittest.TestCase):
    def test_french_comment_caught(self):
        self.assertEqual(len(hits(f"// {FR}\n")), 1)

    def test_marker_on_same_line_or_line_above(self):
        self.assertEqual(hits(f"// {FR} // lang: fr\n"), [])
        self.assertEqual(hits(f"# {FR} # lang: fr\n", "a.py"), [])
        self.assertEqual(hits(f"// lang: fr\n// {FR}\n"), [])
        self.assertEqual(hits(f"<!-- lang: fr -->\n{FR}\n", "a.md"), [])

    def test_marker_two_lines_above_is_too_far(self):
        self.assertEqual(len(hits(f"// lang: fr\n// other\n// {FR}\n")), 1)

    def test_marker_needs_a_comment_opener(self):
        self.assertEqual(len(hits(f"lang: fr {FR}\n")), 1)

    def test_block(self):
        text = f"// lang: fr begin\n// {FR}\n// {FR}\n// lang: fr end\n// {FR}\n"
        got = hits(text)
        self.assertEqual([n for n, _ in got], [5])

    def test_unclosed_block_is_an_error(self):
        _, errors = guard.scan_text("a.rs", f"// lang: fr begin\n// {FR}\n")
        self.assertEqual(len(errors), 1)
        _, errors = guard.scan_text("a.rs", "// lang: fr end\n")
        self.assertEqual(len(errors), 1)

    def test_english_false_positive_guards(self):
        for line in [
            "Measured near Créteil, then Bézier fit",
            "if la >= lb { (la, lb) }",
            "Fira Sans, Noto Sans",
            "Šàṽé Þŕöĵéçţ",
            "é ü 日本",
            "emoji 🙂 and 🧪 here",
            "x′ ≤ ε",
            "checksum = \"98de4bbd547a563b716d8dfa9aad1cb19bfab00f4fa09a6a4ed21dbcf44ce9c4\"",
            "a de facto standard",
            "let x = foo.le; bar::la; la(3)",
        ]:
            with self.subTest(line=line):
                self.assertEqual(hits(line + "\n"), [])

    def test_nfd_line_is_caught(self):
        line = unicodedata.normalize("NFD", "L'état est très bien")
        self.assertNotEqual(line, unicodedata.normalize("NFC", line))
        self.assertEqual(len(hits(line + "\n")), 1)

    def test_backticks_and_urls_are_dropped(self):
        self.assertEqual(hits("see `le calcul est fait` and https://x.org/le/est/pour\n"), [])

    def test_guillemets(self):
        self.assertEqual(hits("The phrase « est-ce que je peux » is a trigger\n"), [])
        self.assertEqual(hits("Phrases « est-ce que\nje peux » trigger it\n"), [])
        self.assertEqual(len(hits(f"Say « ok » then {FR}\n")), 1)

    def test_unclosed_guillemet_carries_one_line_only(self):
        got = hits(f"open « here\nstill inside\n{FR}\n")
        self.assertEqual([n for n, _ in got], [3])

    def test_changelog_versions(self):
        text = (f"# Changelog\n## [Unreleased]\n{FR}\n## [1.0.1] - x\n{FR}\n"
                f"## [1.0.0] - x\n{FR}\n## [0.9.0]\n{FR}\n")
        self.assertEqual([n for n, _ in hits(text, "CHANGELOG.md")], [3, 5])

    def test_changelog_rule_only_for_changelog(self):
        self.assertEqual(len(hits(f"## [1.0.0]\n{FR}\n", "docs/x.md")), 1)

    def test_column_table(self):
        text = (
            "<!-- lang: fr columns: French, Refused -->\n"
            "| French | Definition | Refused |\n"
            "|---|---|---|\n"
            f"| le fichier est pour les | plain English | {FR} |\n"
            f"| ok | {FR} | ok |\n"
            f"{FR}\n"
        )
        self.assertEqual([n for n, _ in hits(text, "a.md")], [5, 6])

    def test_exempt_paths(self):
        ex = guard.is_exempt_path
        for p in ["crates/x/i18n/fr/a.ftl", "docs/archive/a.md", "test/import/a.csv", "vendor/a/b.rs",
                  ".claude/skills/stokesforge-bug-report/references/decision-form.html",
                  "scripts/french-guard-test.py"]:
            self.assertTrue(ex(p), p)
        for p in ["docs/archived.md", "crates/x/i18n/fr-extra/a", "crates/x/i18n/en/a.ftl",
                  "src/vendor/a.rs", "docs/archive", "scripts/french-guard.py"]:
            self.assertFalse(ex(p), p)

    def test_guard_passes_on_itself(self):
        text = Path(guard.__file__).read_text(encoding="utf-8")
        self.assertEqual(hits(text, "scripts/french-guard.py"), [])

    def test_threshold(self):
        self.assertEqual(hits("une seule\n"), [])
        self.assertEqual(len(hits("une cette\n")), 1)


class EndToEnd(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)
        self.git("init", "-q")

    def git(self, *args):
        subprocess.run(["git", "-C", str(self.root), *args], check=True, capture_output=True)

    def add(self, name, data, track=True):
        p = self.root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data if isinstance(data, bytes) else data.encode())
        if track:
            self.git("add", name)

    def run_guard(self, *extra):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = guard.main(["--root", str(self.root), *extra])
        return code, out.getvalue(), err.getvalue()

    def test_clean_repo(self):
        self.add("a.rs", "// English only\n")
        code, out, err = self.run_guard()
        self.assertEqual((code, out), (0, ""))
        self.assertIn("0 problem", err)

    def test_output_format_and_exit_code(self):
        self.add("z.rs", f"fn a() {{}}\n// {FR}\n")
        self.add("b.md", f"{FR}\n")
        code, out, err = self.run_guard()
        self.assertEqual(code, 1)
        lines = out.splitlines()
        self.assertEqual([l.split(": ")[0] for l in lines], ["b.md:1", "z.rs:2"])
        self.assertTrue(lines[1].endswith(FR[:40]) or FR[:40] in lines[1])
        self.assertIn("2 problem", err)

    def test_excerpt_is_truncated(self):
        self.add("a.md", FR + " " + "x" * 300 + "\n")
        _, out, _ = self.run_guard()
        self.assertLessEqual(len(out.splitlines()[0].split(": ", 1)[1]), 120)

    def test_exempt_and_near_miss_paths(self):
        for name in ["docs/archive/a.md", "crates/x/i18n/fr/a.ftl", "vendor/v.rs"]:
            self.add(name, FR + "\n")
        self.add("docs/archived.md", FR + "\n")
        self.add("crates/x/i18n/fr-extra/a", FR + "\n")
        code, out, _ = self.run_guard()
        self.assertEqual(code, 1)
        self.assertEqual([l.split(":")[0] for l in out.splitlines()],
                         ["crates/x/i18n/fr-extra/a", "docs/archived.md"])

    def test_untracked_symlink_binary_ignored(self):
        self.add("untracked.md", FR + "\n", track=False)
        self.add("bin.stf", b"\0" + FR.encode())
        self.add("latin.txt", FR.encode("latin-1") + "é".encode("latin-1"))
        self.add("real.md", FR + "\n", track=False)
        (self.root / "link.md").symlink_to("real.md")
        self.git("add", "link.md")
        code, out, _ = self.run_guard()
        self.assertEqual((code, out), (0, ""))

    def test_unclosed_block_exit_1(self):
        self.add("a.rs", "// lang: fr begin\n")
        code, out, _ = self.run_guard()
        self.assertEqual(code, 1)
        self.assertIn("a.rs:1: error", out)

    def test_paths_restrict(self):
        self.add("a/x.md", FR + "\n")
        self.add("b/y.md", FR + "\n")
        _, out, _ = self.run_guard("a")
        self.assertEqual(out.splitlines()[0].split(":")[0], "a/x.md")
        self.assertEqual(len(out.splitlines()), 1)

    def test_not_a_git_repo_exit_2(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(SystemExit) as cm, contextlib.redirect_stderr(io.StringIO()):
                guard.main(["--root", d])
            self.assertEqual(cm.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
