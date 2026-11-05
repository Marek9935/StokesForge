#!/usr/bin/env python3
"""Tests of scripts/suivi.py (#441): the colour rule, claim, resume, the hook, the local server,
the trial-entry commands, the install scripts.

    python3 scripts/suivi-test.py

Everything runs in a throwaway repository and a throwaway state folder; the real
../_stokesforge/.suivi is never touched.
"""
import http.client
import http.server
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
SUIVI = os.path.join(HERE, "suivi.py")
spec = importlib.util.spec_from_file_location("suivi", SUIVI)
suivi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(suivi)
C = suivi.COLOURS  # blue green orange purple red yellow


def slurp(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def jload(path):
    return json.loads(slurp(path))


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = os.path.join(self.tmp.name, "StokesForge")
        self.root = os.path.join(self.tmp.name, "_stokesforge")
        self.state = os.path.join(self.root, ".suivi")
        os.makedirs(self.repo)
        self.git("init", "-q", "-b", "main")
        self.git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("CLAUDE", "SF_"))}
        self.env["SF_TODAY"] = "2026-11-03"
        self.env["SF_SUIVI_CI"] = "off"     # the CI watcher would call glab and the network (#452)

    def git(self, *a, cwd=None):
        subprocess.run(["git", *a], cwd=cwd or self.repo, check=True, capture_output=True)

    def worktree(self, name):
        self.git("worktree", "add", "-q", os.path.join(self.root, name), "-b", "feature/%s-x" % name, "--no-track", "main")

    def run_cli(self, *args, env=None, stdin=None, cwd=None):
        e = dict(self.env)
        e.update(env or {})
        return subprocess.run([sys.executable, "-I", SUIVI, *args], cwd=cwd or self.repo, env=e,
                              input=stdin, capture_output=True, text=True)

    def as_session(self, sid="S1", pid=None):
        return {"CLAUDE_CODE_SESSION_ID": sid, "CLAUDE_PID": str(pid or os.getpid())}

    def put(self, n, **fields):
        r = suivi.new_record(n)
        r.update(fields)
        r["issue"] = n
        d = os.path.join(self.state, "issues")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "%d.json" % n), "w") as f:
            json.dump(r, f)

    def rec(self, n):
        with open(os.path.join(self.state, "issues", "%d.json" % n)) as f:
            return json.load(f)

    def snapshot(self):
        out = {}
        d = os.path.join(self.state, "issues")
        for f in os.listdir(d):
            with open(os.path.join(d, f)) as fh:
                out[f] = fh.read()
        return out

    def live_owner(self, sid="S1", at=None):
        return {"session": sid, "pid": os.getpid(), "pid_start": suivi.pid_start(os.getpid()),
                "at": at if at is not None else time.time()}

    def status(self):
        r = self.run_cli("status", "--json")
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    def held(self, colour=C[0]):
        e = self.status()["colours"][C.index(colour)]
        return [] if e["free"] else e["held_by"]

    def claim(self, n, *extra, sid="S1", pid=None):
        r = self.run_cli("claim", str(n), *extra, env=self.as_session(sid, pid))
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout.strip()


class TestOwner(Base):
    def test_live_owner_holds(self):
        self.put(1, colour=C[0], owner=self.live_owner())
        self.assertEqual(self.held(), ["agent"])

    def test_dead_pid_pauses(self):
        child = subprocess.Popen(["sleep", "30"])
        start = suivi.pid_start(child.pid)
        child.kill()
        child.wait()
        self.put(1, colour=C[0], owner={"session": "S1", "pid": child.pid, "pid_start": start, "at": time.time()})
        self.assertEqual(self.held(), [])
        self.assertEqual([p["issue"] for p in self.status()["paused"]], [1])

    def test_reused_pid_pauses(self):
        o = self.live_owner()
        o["pid_start"] += 1
        self.put(1, colour=C[0], owner=o)
        self.assertEqual(self.held(), [])

    def test_ended_session_pauses_until_a_new_claim(self):
        self.put(1, colour=C[0], owner=self.live_owner(at=time.time() - 100))
        self.assertEqual(self.held(), ["agent"])
        os.makedirs(os.path.join(self.state, "ended"))
        ended = os.path.join(self.state, "ended", "S1")
        open(ended, "w").close()
        os.utime(ended, (time.time() - 5, time.time() - 5))
        self.assertEqual(self.held(), [])
        self.assertEqual(self.claim(1), C[0])
        self.assertEqual(self.held(), ["agent"])


class TestQcmAndTrials(Base):
    def qcm(self, issue, finished):
        d = os.path.join(self.state, "qcm", "q1")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump({"issue": issue, "finished": finished}, f)

    def trial(self, n, k, **fields):
        d = os.path.join(self.state, "tests")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "%d-%d.json" % (n, k)), "w") as f:
            json.dump(fields, f)

    def test_open_qcm_holds_finished_does_not(self):
        self.put(1, colour=C[0])
        self.qcm(1, False)
        self.assertEqual(self.held(), ["qcm"])
        self.qcm(1, True)
        self.assertEqual(self.held(), [])

    def test_qcm_of_another_issue_does_not_hold(self):
        self.put(1, colour=C[0])
        self.qcm(2, False)
        self.assertEqual(self.held(), [])

    def test_trial_todo_with_try_holds(self):
        self.put(1, colour=C[0])
        self.trial(1, 1, status="todo", **{"try": "scripts/try.sh 1"})
        self.assertEqual(self.held(), ["essai"])

    def test_trial_ok_does_not_hold(self):
        self.put(1, colour=C[0])
        self.trial(1, 1, status="ok", **{"try": "scripts/try.sh 1"})
        self.assertEqual(self.held(), [])

    def test_post_merge_entry_without_try_does_not_hold(self):
        self.put(1, colour=C[0])
        self.trial(1, 1, status="todo", commit="abc")
        self.assertEqual(self.held(), [])


class TestClaim(Base):
    def test_resume_takes_the_previous_colour_when_free(self):
        self.put(1, last_colour=C[2])
        self.assertEqual(self.claim(1), C[2])

    def test_resume_takes_the_first_free_when_the_previous_is_taken(self):
        self.put(2, colour=C[2], owner=self.live_owner())
        self.put(1, last_colour=C[2])
        self.assertEqual(self.claim(1), C[0])
        self.assertEqual(self.rec(1)["colour"], C[0])

    def test_rerun_keeps_colour_and_since_with_a_new_owner(self):
        self.assertEqual(self.claim(1), C[0])
        self.put(1, **{**self.rec(1), "since": "2026-10-01"})
        self.assertEqual(self.claim(1, sid="S2"), C[0])
        r = self.rec(1)
        self.assertEqual((r["since"], r["owner"]["session"]), ("2026-10-01", "S2"))

    def test_seventh_claim_fails(self):
        for n in range(1, 7):
            self.assertEqual(self.claim(n), C[n - 1])
        r = self.run_cli("claim", "7", env=self.as_session())
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("6 work packages in progress", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.state, "issues", "7.json")))

    def test_parallel_claims_get_distinct_colours(self):
        env = {**self.env, **self.as_session(), "SF_SUIVI_SLOW": "0.3"}
        procs = [subprocess.Popen([sys.executable, "-I", SUIVI, "claim", str(n)], cwd=self.repo, env=env,
                                  stdout=subprocess.PIPE, text=True) for n in (1, 2, 3)]
        colours = [p.communicate()[0].strip() for p in procs]
        self.assertEqual(len(set(colours)), 3, colours)

    def test_lock_timeout_names_the_lock_and_its_age(self):
        os.makedirs(os.path.join(self.root, ".colour.lock"))
        r = self.run_cli("claim", "1", env={**self.as_session(), "SF_SUIVI_LOCK_WAIT": "0.3"})
        self.assertNotEqual(r.returncode, 0)
        self.assertIn(".colour.lock", r.stderr)
        self.assertIn("age", r.stderr)

    def test_claim_outside_claude_has_no_owner_and_warns(self):
        r = self.run_cli("claim", "1")
        self.assertEqual(r.returncode, 0)
        self.assertEqual(r.stdout.strip(), C[0])
        self.assertIn("no Claude session", r.stderr)
        self.assertIsNone(self.rec(1)["owner"])

    def test_sub_issue_shares_the_package_colour(self):
        self.assertEqual(self.claim(9, "--package", "--sub", "5=Five"), C[0])
        self.assertEqual(self.claim(5, "--parent", "9"), C[0])
        self.assertEqual(self.claim(6), C[1])
        self.assertEqual(self.rec(9)["kind"], "package")
        self.assertEqual(self.rec(5)["parent"], 9)

    def test_package_between_two_sub_issues_follows_its_owner(self):
        self.claim(9, "--package")
        self.put(9, **{**self.rec(9), "owner": None})
        self.assertEqual(self.held(), [])
        self.assertEqual([p["issue"] for p in self.status()["paused"]], [9])

    def test_dry_run_writes_nothing(self):
        r = self.run_cli("claim", "--dry-run", "1", "--avoid", C[0], env=self.as_session())
        self.assertEqual(r.stdout.strip(), C[1])
        self.assertFalse(os.path.exists(self.state))


class TestPauseDone(Base):
    def test_pause_frees_the_colour_unless_a_qcm_holds_it(self):
        self.claim(1)
        self.claim(2)
        self.assertTrue(self.run_cli("pause", "1").stdout.startswith("Dossier #1"))
        self.assertEqual(self.held(C[0]), [])
        os.makedirs(os.path.join(self.state, "qcm", "q"))
        with open(os.path.join(self.state, "qcm", "q", "meta.json"), "w") as f:
            json.dump({"issue": 2, "finished": False}, f)
        out = self.run_cli("pause", "2").stdout
        self.assertIn("reste prise", out)
        self.assertEqual(self.held(C[1]), ["qcm"])

    def test_done_frees_and_lists_the_issue(self):
        self.claim(1)
        self.assertEqual(self.run_cli("done", "1").stdout.strip(), "Couleur %s libérée." % C[0])
        s = self.status()
        self.assertTrue(s["colours"][0]["free"])
        self.assertEqual([d["issue"] for d in s["done"]], [1])
        self.assertEqual(s["paused"], [])

    def test_set_records_the_state(self):
        self.claim(1)
        r = self.run_cli("set", "1", "--state", "essai", "--what", "trial", "--wait", "you", "--waiting", "your OK")
        self.assertEqual(r.returncode, 0, r.stderr)
        e = self.status()["colours"][0]
        self.assertEqual((e["state"], e["wait_kind"], e["waiting"]), ("essai", "you", "your OK"))
        self.assertNotEqual(self.run_cli("set", "1", "--state", "nonsense").returncode, 0)


class TestReleaseLine(Base):
    def test_lines(self):
        self.worktree("feature10")
        self.worktree("feature10b")
        self.put(10, colour=C[0], owner=self.live_owner())
        self.put(11, colour=C[1], owner=self.live_owner())
        self.put(12, colour=C[2], owner=self.live_owner())
        self.put(13, colour=C[3], owner=self.live_owner())
        os.makedirs(os.path.join(self.state, "qcm", "q"))
        with open(os.path.join(self.state, "qcm", "q", "meta.json"), "w") as f:
            json.dump({"issue": 11, "finished": False}, f)
        os.makedirs(os.path.join(self.state, "tests"))
        with open(os.path.join(self.state, "tests", "12-1.json"), "w") as f:
            json.dump({"status": "todo", "try": "scripts/try.sh 12"}, f)
        line = lambda n, *x: self.run_cli("release-line", str(n), *x).stdout.strip()
        # lang: fr begin
        self.assertEqual(line(10), "Couleur %s reste prise : feature10, feature10b" % C[0])
        self.assertEqual(line(11, "--mark-done"), "Couleur %s reste prise : un QCM de l'issue est ouvert" % C[1])
        self.assertEqual(line(12), "Couleur %s reste prise : un essai de l'issue est en attente" % C[2])
        self.assertEqual(line(13, "--mark-done"), "Couleur %s libérée." % C[3])
        self.assertEqual(line(99), "Ce dossier ne tenait aucune couleur.")
        # lang: fr end
        self.assertIsNone(self.rec(10)["done_at"])
        self.assertIsNone(self.rec(11)["done_at"])
        self.assertEqual(self.rec(13)["done_at"], "2026-11-03")
        self.assertIsNone(self.rec(13)["colour"])

    def test_sub_issue_keeps_the_package_colour(self):
        self.put(9, kind="package", colour=C[4], owner=self.live_owner())
        self.put(5, parent=9, colour=C[4], owner=self.live_owner())
        out = self.run_cli("release-line", "5", "--mark-done").stdout.strip()
        # lang: fr begin
        self.assertEqual(out, "Couleur %s reste prise : paquet #9, prochaine sous-issue ; après la dernière : "
                              "scripts/chantier.sh --done 9" % C[4])
        # lang: fr end
        self.assertEqual(self.held(C[4]), ["agent"])


class TestCheck(Base):
    def test_clean_and_problems(self):
        self.worktree("feature1")
        self.claim(1)
        r = self.run_cli("check")
        self.assertEqual((r.returncode, r.stdout), (0, ""), r.stderr)
        with open(os.path.join(self.state, "control.json")) as f:
            self.assertEqual(json.load(f)["problems"], [])
        self.worktree("feature2")          # no record
        self.put(3, colour=C[0], owner=self.live_owner())   # no worktree, same colour as #1
        out = self.run_cli("check")
        self.assertEqual(out.returncode, 1)
        for text in ("colour %s is held by #1, #3" % C[0], "issue #3 has a record but no worktree",
                     "worktree feature2 has no record"):
            self.assertIn(text, out.stdout)
        with open(os.path.join(self.state, "control.json")) as f:
            c = json.load(f)
        self.assertEqual(len(c["problems"]), 3)
        self.assertIn("at", c)


class TestSessionEnd(Base):
    def test_touches_ended(self):
        os.makedirs(self.state)
        r = self.run_cli("session-end", stdin='{"session_id":"X","reason":"clear"}')
        self.assertEqual(r.returncode, 0)
        self.assertTrue(os.path.isfile(os.path.join(self.state, "ended", "X")))

    def test_bad_json_exits_zero_and_writes_nothing(self):
        os.makedirs(self.state)
        for bad in ("not json", "", '{"reason":"clear"}', '{"session_id":"../evil"}'):
            r = self.run_cli("session-end", stdin=bad)
            self.assertEqual(r.returncode, 0, bad)
        self.assertEqual(os.listdir(self.state), [])
        self.assertFalse(os.path.exists(os.path.join(self.root, "evil")))

    def test_missing_state_dir_exits_zero_and_creates_nothing(self):
        r = self.run_cli("session-end", stdin='{"session_id":"X"}')
        self.assertEqual(r.returncode, 0)
        self.assertFalse(os.path.exists(self.state))

    def test_outside_a_repository_exits_zero(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.run_cli("session-end", stdin='{"session_id":"X"}', cwd=d)
        self.assertEqual(r.returncode, 0)


class TestMigrate(Base):
    def test_creates_records_with_the_same_colours_and_is_idempotent(self):
        for name, colour in (("feature5", C[3]), ("feature6", C[4])):
            self.worktree(name)
            gd = subprocess.run(["git", "-C", os.path.join(self.root, name), "rev-parse", "--absolute-git-dir"],
                                capture_output=True, text=True).stdout.strip()
            with open(os.path.join(gd, "stokesforge-colour"), "w") as f:
                f.write(colour + "\n")
        os.makedirs(os.path.join(self.root, ".packages"))
        with open(os.path.join(self.root, ".packages", "9"), "w") as f:
            f.write("%s\n6\tSix things\n7\tSeven\n" % C[4])
        with open(os.path.join(self.root, ".packages", "8"), "w") as f:
            f.write("\n3\tThree\n")   # paused package
        r = self.run_cli("migrate")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.rec(5)["colour"], C[3])
        self.assertEqual((self.rec(6)["colour"], self.rec(6)["parent"]), (C[4], 9))
        p = self.rec(9)
        self.assertEqual((p["kind"], p["colour"], [s["n"] for s in p["subs"]]), ("package", C[4], [6, 7]))
        self.assertIsNone(self.rec(8)["colour"])
        before = self.snapshot()
        again = self.run_cli("migrate")
        self.assertIn("nothing to do", again.stdout)
        after = self.snapshot()
        self.assertEqual(before, after)
        # A migrated issue resumes with its colour.
        self.assertEqual(self.claim(5), C[3])


class TestMigrateLegacy(Base):
    """Orchestrators already running when the new layout lands have no owner: keep their colours."""

    def migrate_two(self):
        for name, colour in (("feature5", C[3]), ("feature6", C[4])):
            self.worktree(name)
            gd = subprocess.run(["git", "-C", os.path.join(self.root, name), "rev-parse", "--absolute-git-dir"],
                                capture_output=True, text=True).stdout.strip()
            with open(os.path.join(gd, "stokesforge-colour"), "w") as f:
                f.write(colour + "\n")
        self.assertEqual(self.run_cli("migrate").returncode, 0)

    def test_migrated_issue_with_a_live_worktree_holds_its_colour_without_an_owner(self):
        self.migrate_two()
        self.assertTrue(self.rec(5)["legacy"])
        self.assertIsNone(self.rec(5)["owner"])
        self.assertEqual(self.held(C[3]), ["legacy"])
        # A new claim cannot take a legacy colour.
        self.assertEqual(self.claim(7), C[0])
        self.claim(8)
        self.claim(9)
        self.assertEqual(self.claim(10), C[5])

    def test_claim_pause_and_done_end_the_hold(self):
        self.migrate_two()
        self.assertEqual(self.claim(5), C[3])
        self.assertEqual(self.held(C[3]), ["agent"])
        self.assertFalse(self.rec(5)["legacy"])
        self.assertIn("Dossier #6 en pause", self.run_cli("pause", "6").stdout)   # lang: fr
        self.assertEqual(self.held(C[4]), [])
        self.run_cli("pause", "5")
        self.assertEqual(self.held(C[3]), [])

    def test_done_ends_the_hold(self):
        self.migrate_two()
        self.run_cli("done", "5")
        self.assertEqual(self.held(C[3]), [])

    def test_check_does_not_free_it_and_status_json_says_so(self):
        self.migrate_two()
        e = self.status()["colours"][C.index(C[4])]
        self.assertEqual((e["free"], e["held_by"]), (False, ["legacy"]))


class TestTestCommands(Base):
    def entries(self, tmp, data):
        p = os.path.join(tmp, "e.json")
        with open(p, "w") as f:
            json.dump(data, f)
        return p

    ONE = {"title": "A", "where": ["File", "Open"], "steps": ["Do"], "expect": "See", "try": "scripts/try.sh 7"}

    def gout(self, *a):
        return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *a], cwd=self.repo,
                              check=True, capture_output=True, text=True).stdout.strip()

    def feature(self, n, msg=None, keep=True, topic="x"):
        """Merge a real feature/<n>-<topic> branch into main with --no-ff; returns the merge commit."""
        br = "feature/%d-%s" % (n, topic)
        self.gout("checkout", "-q", "-b", br, "main")
        self.gout("commit", "-q", "--allow-empty", "-m", "work %d" % n)
        self.gout("checkout", "-q", "main")
        self.gout("merge", "-q", "--no-ff", "-m", msg or "Merge #%d: x" % n, br)
        h = self.gout("rev-parse", "HEAD")
        if not keep:
            self.gout("branch", "-q", "-D", br)
        return h

    def add(self, n, entries, *extra):
        with tempfile.TemporaryDirectory() as tmp:
            return self.run_cli("test", "add", str(n), self.entries(tmp, entries), *extra)

    def tfile(self, tid):
        return os.path.join(self.state, "tests", tid + ".json")

    def snap_tests(self):
        d = os.path.join(self.state, "tests")
        return {f: slurp(os.path.join(d, f)) for f in sorted(os.listdir(d))} if os.path.isdir(d) else {}

    def put_test(self, tid, **fields):
        os.makedirs(os.path.join(self.state, "tests"), exist_ok=True)
        with open(self.tfile(tid), "w") as f:
            json.dump(dict({"title": tid, "status": "todo", "n": int(tid.split("-")[0])}, **fields), f)

    def merged(self, n, commit, *extra):
        return self.run_cli("test", "merged", str(n), commit, *extra)

    def test_add_merged_list(self):
        self.claim(7, "--name", "Seven")
        r = self.add(7, [self.ONE, {"title": "B", "try": "scripts/try.sh 7"}])
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "7-1 7-2"), r.stderr)
        self.assertEqual(self.add(7, self.ONE).stdout.strip(), "7-3")
        t = jload(self.tfile("7-1"))
        self.assertEqual((t["status"], t["n"], t["name"], t["colour"], t["date"]), ("todo", 7, "Seven", C[0], "2026-11-03"))
        self.assertEqual(self.held(C[0]), ["agent", "essai"])
        out = self.run_cli("test", "list", "--issue", "7").stdout
        self.assertEqual(len(out.strip().splitlines()), 3)
        self.assertIn("(try)", out)
        full = self.feature(7)
        r = self.merged(7, full[:8])
        self.assertIn("3 entries", r.stdout)
        t = jload(self.tfile("7-1"))
        self.assertEqual((t["commit"], "try" in t), (full, False))
        self.assertEqual(self.held(C[0]), ["agent"])

    def test_rejects_unknown_fields_and_bad_commit(self):
        r = self.add(7, {"title": "A", "status": "ok"})
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("unknown field", r.stderr)
        self.assertNotEqual(self.add(7, {"steps": "not a list"}).returncode, 0)
        self.assertNotEqual(self.add(7, {"intro": ["not", "a", "str"]}).returncode, 0)
        self.assertFalse(os.path.exists(os.path.join(self.state, "tests")))
        r = self.merged(7, "../x")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("not a commit hash", r.stderr)

    def test_merged_refuses_a_commit_that_does_not_belong_to_the_issue(self):
        self.add(7, [self.ONE])
        self.add(44, [dict(self.ONE, title="F")])
        before = self.snap_tests()
        self.gout("commit", "-q", "--allow-empty", "-m", "prepare #7")          # main HEAD before the merge
        prev = self.gout("rev-parse", "HEAD")
        full = self.feature(7)
        self.gout("checkout", "-q", "-b", "side", full)                        # contains the branch, cites #7, not on main
        self.gout("commit", "-q", "--allow-empty", "-m", "side #7")
        side = self.gout("rev-parse", "HEAD")
        self.gout("checkout", "-q", "main")
        m449 = self.feature(449, keep=False)
        for what, issue, commit in (("unknown", 7, "deadbeef"), ("not on main", 7, side),
                                    ("main before the merge, branch still there", 7, prev),
                                    ("message cites #449, not #44", 44, m449)):
            r = self.merged(issue, commit)
            self.assertNotEqual(r.returncode, 0, what)
            self.assertEqual(self.snap_tests(), before, what)
        self.assertIn("no such commit", self.merged(7, "deadbeef").stderr)
        self.assertIn("not on main", self.merged(7, side).stderr)
        self.gout("branch", "-q", "-D", "feature/7-x")                         # branch gone: the message is enough
        r = self.merged(7, full)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(jload(self.tfile("7-1"))["commit"], full)

    def test_replace_changes_only_the_entries_that_carry_the_old_commit(self):
        old, other = "a" * 40, "b" * 40
        self.put_test("7-1", commit=old, status="ok", comment="fine", reviewed="2026-11-01")
        self.put_test("7-2", commit=old[:8].upper(), status="retour", comment="no")
        self.put_test("7-3", commit=other)
        self.put_test("7-4", **{"try": "scripts/try.sh 7"})
        full = self.feature(7)
        before = self.snap_tests()
        for bad in (("zzzz", "--replace", old), (full[:8], "--replace", "c" * 8)):
            r = self.merged(7, *bad)
            self.assertNotEqual(r.returncode, 0, bad)
            self.assertEqual(self.snap_tests(), before)
        r = self.merged(7, full[:8], "--replace", old[:6])                      # short old against a full stored one
        self.assertEqual(r.returncode, 0, r.stderr)
        after = self.snap_tests()
        self.assertEqual(json.loads(after["7-1.json"]), dict(json.loads(before["7-1.json"]), commit=full))
        self.assertEqual(json.loads(after["7-2.json"]), dict(json.loads(before["7-2.json"]), commit=full))
        self.assertEqual((after["7-3.json"], after["7-4.json"]), (before["7-3.json"], before["7-4.json"]))
        self.put_test("7-5", commit=full[:8])
        self.assertEqual(self.merged(7, full, "--replace", full).returncode, 0)  # full old against a short stored one

    def test_replace_matches_a_legacy_non_hex_commit_exactly(self):
        self.put_test("7-1", commit="main")
        self.put_test("7-2", commit="mainline")
        full = self.feature(7)
        self.assertEqual(self.merged(7, full, "--replace", "mai").returncode, 1)
        self.assertEqual(self.merged(7, full, "--replace", "main").returncode, 0)
        self.assertEqual((jload(self.tfile("7-1"))["commit"], jload(self.tfile("7-2"))["commit"]), (full, "mainline"))

    def test_second_merged_call_is_idempotent_or_points_at_replace(self):
        self.add(7, [self.ONE])
        full = self.feature(7)
        self.assertIn("1 entry", self.merged(7, full).stdout)
        for ref in (full, full[:8]):
            r = self.merged(7, ref)
            self.assertEqual((r.returncode, r.stdout.strip()), (0, "already recorded"))
        self.gout("commit", "-q", "--allow-empty", "-m", "touch #7")
        other = self.gout("rev-parse", "HEAD")
        before = self.snap_tests()
        r = self.merged(7, other)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn(full, r.stderr)
        self.assertIn("--replace", r.stderr)
        self.assertEqual(self.snap_tests(), before)
        r = self.merged(5, full)                                                # no entries for the issue
        self.assertEqual(r.returncode, 1)                                       # (message cites #7, not #5)
        self.feature(5)
        r = self.merged(5, self.gout("rev-parse", "HEAD"))
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "no entries"))

    def test_merged_writes_only_the_entries_to_try_when_there_are_some(self):
        self.put_test("7-1", commit="old12345", status="ok")
        self.put_test("7-2", **{"try": "scripts/try.sh 7"})
        full = self.feature(7)
        self.assertEqual(self.merged(7, full).returncode, 0)
        self.assertEqual(jload(self.tfile("7-1"))["commit"], "old12345")
        self.assertEqual(jload(self.tfile("7-2"))["commit"], full)

    def test_at_most_five_entries_to_try_per_issue(self):
        five = [dict(self.ONE, title=str(i)) for i in range(5)]
        self.assertEqual(self.add(7, five).returncode, 0)
        before = self.snap_tests()
        r = self.add(7, [self.ONE])
        self.assertNotEqual(r.returncode, 0)
        self.assertEqual(self.snap_tests(), before)
        r = self.add(7, [self.ONE], "--force")
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "7-6"))
        self.assertIn("justify", r.stderr)
        self.assertEqual(self.add(8, five + [self.ONE]).returncode, 1)          # one file of six
        self.assertEqual(self.snap_tests().keys() & {"8-1.json"}, set())
        self.assertEqual(self.add(8, five + [self.ONE], "--force").returncode, 0)
        self.put_test("9-1", commit="x" * 8)                                     # merged entries do not count
        self.assertEqual(self.add(9, five).returncode, 0)

    def test_big_legacy_issue_still_works_but_takes_no_new_entry_without_force(self):
        self.claim(5)
        for k in range(1, 27):
            self.put_test("5-%d" % k, **{"try": "scripts/try.sh 5"}, **({"issue": 434} if k == 3 else {}))
        self.assertEqual(len(self.run_cli("test", "list", "--issue", "5").stdout.splitlines()), 26)
        self.assertEqual(self.held(C[0]), ["agent", "essai"])
        self.assertNotEqual(self.add(5, [self.ONE]).returncode, 0)
        full = self.feature(5)
        self.assertIn("26 entries", self.merged(5, full).stdout)

    def own(self, sid="S1"):
        return self.as_session(sid)

    def remove(self, n, *ids, sid="S1", env=None):
        return self.run_cli("test", "remove", str(n), *ids, env=self.own(sid) if env is None else env)

    def test_remove_needs_the_owner_session_of_the_issue_or_its_parent(self):
        self.put_test("7-1", **{"try": "t"})
        self.put_test("9-1", **{"try": "t"})
        before = self.snap_tests()
        self.assertNotEqual(self.remove(7).returncode, 0)                                   # argparse: no ids
        self.assertIn("chantier.sh 7", self.remove(7, "7-1").stderr)                         # no record
        self.put(7, colour=C[0])                                                             # a record, no owner
        self.assertNotEqual(self.remove(7, "7-1").returncode, 0)
        self.put(7, colour=C[0], legacy=True)
        self.assertNotEqual(self.remove(7, "7-1").returncode, 0)
        self.put(7, colour=C[0], owner=self.live_owner("S2"))
        self.assertNotEqual(self.remove(7, "7-1", sid="S1").returncode, 0)                   # another session
        self.put(7, colour=C[0], owner=self.live_owner("S1"))
        self.assertNotEqual(self.remove(7, "7-1", env={}).returncode, 0)                     # no session
        self.assertEqual(self.snap_tests(), before)
        # package: the parent's owner removes a sub's entry, the sub's owner cannot touch the parent's
        self.put(9, kind="package", colour=C[1], owner=self.live_owner("S1"))
        self.put(7, colour=C[1], parent=9, owner=self.live_owner("S2"))
        self.assertEqual(self.remove(7, "7-1", sid="S1").returncode, 0)
        self.assertNotEqual(self.remove(9, "9-1", sid="S2").returncode, 0)
        self.assertTrue(os.path.exists(self.tfile("9-1")))

    def test_remove_never_touches_merged_or_answered_entries_without_the_option(self):
        self.claim(7)
        self.put_test("7-1", commit="a" * 8)
        self.put_test("7-2", status="ok")
        self.put_test("7-3", status="retour", comment="too slow", **{"try": "t"})
        self.put_test("7-4", comment="why?", **{"try": "t"})
        self.put_test("7-5", reviewed="2026-11-02", **{"try": "t"})
        self.put_test("7-6", status="ok", **{"try": "t"})
        self.put_test("7-7", **{"try": "t"})
        before = self.snap_tests()
        self.assertNotEqual(self.remove(7, "7-1", "--answered").returncode, 0)
        for ids in (("7-3",), ("7-4",), ("7-6",), ("7-7", "7-3"), ("7-7", "7-5", "7-6")):
            r = self.remove(7, *ids)
            self.assertNotEqual(r.returncode, 0, ids)
            self.assertEqual(self.snap_tests(), before, ids)
        r = self.run_cli("test", "remove", "7", "7-3", "7-4", "7-6", "--answered", env=self.own())
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("too slow", r.stdout)
        self.assertIn("retour", r.stdout)
        r = self.remove(7, "7-5", "7-7")                       # `reviewed` alone: still unanswered
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(sorted(self.snap_tests()), ["7-1.json", "7-2.json"])

    def test_remove_archives_the_entry_and_never_reuses_its_number(self):
        self.claim(7)
        self.add(7, [self.ONE, self.ONE])
        self.run_cli("test", "remove", "7", "7-2", "--answered", env=self.own())
        self.assertEqual(sorted(os.listdir(os.path.join(self.state, "tests"))), ["7-1.json"])
        arch = jload(os.path.join(self.state, "removed-tests", "7-2.json"))
        self.assertEqual((arch["title"], "removed_at" in arch), ("A", True))
        self.assertEqual(self.remove(7, "7-1").returncode, 0)
        self.assertEqual(self.held(C[0]), ["agent"])                                       # no entry left to hold it
        self.assertEqual(self.add(7, self.ONE).stdout.strip(), "7-3")
        self.assertEqual(self.run_cli("test", "list", "--issue", "7").stdout.count("\n"), 1)

    def test_set_erases_the_agent_note_unless_restated(self):
        self.claim(7)
        self.run_cli("set", "7", "--state", "code", "--what", "w", "--wait", "you", "--waiting", "note")
        self.run_cli("set", "7", "--what", "other")
        r = self.rec(7)
        self.assertEqual((r["state"], r["what"], r["waiting"], r["wait_kind"]), ("code", "other", "", ""))
        self.run_cli("set", "7", "--wait", "dep", "--waiting", "n2")
        self.run_cli("set", "7", "--wait", "dep", "--waiting", "n2", "--what", "z")
        r = self.rec(7)
        self.assertEqual((r["what"], r["waiting"], r["wait_kind"]), ("z", "n2", "dep"))


class TestImportOnline(Base):
    def export(self, tmp, docs):
        os.makedirs(os.path.join(tmp, "tests"))
        os.makedirs(os.path.join(tmp, "chantiers"))
        with open(os.path.join(tmp, "chantiers", "9.json"), "w") as f:
            json.dump({"id": "9", "data": {"colour": C[0]}}, f)
        for stem, doc in docs.items():
            with open(os.path.join(tmp, "tests", stem + ".json"), "w") as f:
                json.dump(doc, f)

    def files(self):
        d = os.path.join(self.state, "tests")
        return sorted(os.listdir(d)) if os.path.isdir(d) else []

    def test_import_is_complete_idempotent_and_only_tests(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.export(tmp, {
                "1-1": {"id": "1-1", "data": {"status": "ok", "comment": "fine", "reviewed": "2026-11-01", "title": "A"}},
                "1-2": {"data": {"status": "todo", "title": "B"}},
                "2-1": {"id": "2-1", "status": "retour", "comment": "no", "title": "C", "_version": 3}})
            r = self.run_cli("import-online", tmp)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(self.files(), ["1-1.json", "1-2.json", "2-1.json"])
            t = jload(os.path.join(self.state, "tests", "1-1.json"))
            self.assertEqual((t["status"], t["comment"], t["reviewed"]), ("ok", "fine", "2026-11-01"))
            self.assertNotIn("_version", jload(os.path.join(self.state, "tests", "2-1.json")))
            self.assertFalse(os.path.exists(os.path.join(self.state, "chantiers")))
            before = {f: slurp(os.path.join(self.state, "tests", f)) for f in self.files()}
            again = self.run_cli("import-online", tmp)
            self.assertIn("imported 0, unchanged 3", again.stdout)
            self.assertEqual(before, {f: slurp(os.path.join(self.state, "tests", f)) for f in self.files()})

    def test_unsafe_ids_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.export(tmp, {"ok-1": {"id": "../x", "data": {"title": "evil"}},
                              "3-1": {"id": ".hidden", "data": {}},
                              "3-2": {"id": "3-2", "data": {"title": "fine"}}})
            r = self.run_cli("import-online", tmp)
        self.assertEqual(r.returncode, 1)
        self.assertIn("rejected 2", r.stdout)
        self.assertEqual(self.files(), ["3-2.json"])
        self.assertFalse(os.path.exists(os.path.join(self.state, "x.json")))
        self.assertFalse(os.path.exists(os.path.join(self.root, "x.json")))


class ServerBase(Base):
    def setUp(self):
        super().setUp()
        self.claim(1, "--name", "One")
        self.run_cli("set", "1", "--state", "code", "--what", "doing", "--wait", "you", "--waiting", "your OK")
        self.claim(2, "--name", "Two")
        os.makedirs(os.path.join(self.state, "tests"))
        self.entry = os.path.join(self.state, "tests", "1-1.json")
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": "todo", "try": "scripts/try.sh 1"}, f)
        self.proc = subprocess.Popen([sys.executable, "-I", SUIVI, "serve", "--port", "0", "--state", self.state],
                                     cwd=self.repo, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop)
        line = self.proc.stdout.readline()
        m = re.search(r"http://127\.0\.0\.1:(\d+)/", line)
        self.assertTrue(m, line)
        self.port = int(m.group(1))

    def stop(self):
        self.proc.terminate()
        self.proc.wait(5)
        self.proc.stdout.close()
        self.proc.stderr.close()

    def req(self, method, path, headers=None, body=None, host="default"):
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        c.putrequest(method, path, skip_host=True, skip_accept_encoding=True)
        c.putheader("Host", "127.0.0.1:%d" % self.port if host == "default" else host)
        for k, v in (headers or {}).items():
            c.putheader(k, v)
        data = body if isinstance(body, bytes) else (json.dumps(body).encode() if body is not None else b"")
        if method == "POST":
            c.putheader("Content-Length", str(len(data)))
        c.endheaders(data)
        r = c.getresponse()
        out = (r.status, dict(r.getheaders()), r.read())
        c.close()
        return out

    def post(self, path="/api/tests/1-1", body=None, headers="ok", host="default"):
        h = {"X-Suivi": "1", "Content-Type": "application/json"} if headers == "ok" else headers
        return self.req("POST", path, h, {"status": "ok", "comment": "c"} if body is None else body, host)

    def entry_text(self):
        with open(self.entry) as f:
            return f.read()

class TestServer(ServerBase):
    def test_csp_lets_the_inline_tab_icon_load(self):
        status, headers, _ = self.req("GET", "/")
        csp = {k.lower(): v for k, v in headers.items()}["content-security-policy"]
        self.assertIn("img-src data:", csp)
        self.assertIn("default-src 'none'", csp)

    def test_post_without_the_custom_header_is_403_and_writes_nothing(self):
        before = self.entry_text()
        self.assertEqual(self.post(headers={"Content-Type": "text/plain"})[0], 403)
        self.assertEqual(self.entry_text(), before)

    def test_foreign_origin_is_403(self):
        before = self.entry_text()
        h = {"X-Suivi": "1", "Origin": "http://evil.example"}
        self.assertEqual(self.post(headers=h)[0], 403)
        self.assertEqual(self.entry_text(), before)
        h["Origin"] = "http://localhost:%d" % self.port
        self.assertEqual(self.post(headers=h)[0], 200)

    def test_foreign_host_is_403(self):
        for host in ("evil.example", "evil.example:%d" % self.port, "127.0.0.1:1", "127.0.0.1"):
            self.assertEqual(self.req("GET", "/api/state", host=host)[0], 403, host)
            self.assertEqual(self.post(host=host)[0], 403, host)
        self.assertEqual(self.req("GET", "/api/state", host="localhost:%d" % self.port)[0], 200)

    def test_unsafe_ids_are_400(self):
        for tid in ("../x", ".hidden", "a%2Fb", "a/b", ".."):
            self.assertEqual(self.post("/api/tests/" + tid)[0], 400, tid)
        self.assertEqual(self.req("GET", "/qcm/../x")[0], 400)
        self.assertFalse(os.path.exists(os.path.join(self.state, "x.json")))

    def test_big_body_is_413(self):
        before = self.entry_text()
        status = self.post(body=b'{"comment":"%s"}' % (b"x" * (256 * 1024 + 10)))[0]
        self.assertEqual(status, 413)
        self.assertEqual(self.entry_text(), before)

    def test_options_grants_no_cross_origin_access(self):
        status, headers, _ = self.req("OPTIONS", "/api/tests/1-1", {"Origin": "http://evil.example"})
        self.assertEqual(status, 403)
        status, headers, _ = self.req("OPTIONS", "/api/tests/1-1")
        self.assertEqual(status, 204)
        self.assertEqual([k for k in headers if k.lower().startswith("access-control-")], [])
        _, headers, _ = self.req("GET", "/api/state")
        self.assertEqual([k for k in headers if k.lower().startswith("access-control-")], [])

    def test_listens_on_loopback_only(self):
        listening = []
        for f in ("/proc/net/tcp", "/proc/net/tcp6"):
            with open(f) as fh:
                for line in fh.read().splitlines()[1:]:
                    cols = line.split()
                    addr, port = cols[1].split(":")
                    if cols[3] == "0A" and int(port, 16) == self.port:
                        listening.append((f, addr))
        self.assertEqual(listening, [("/proc/net/tcp", "0100007F")])

    def test_valid_post_updates_the_file_and_the_state(self):
        status, _, _ = self.post(body={"status": "retour", "comment": "no good"})
        self.assertEqual(status, 200)
        t = json.loads(self.entry_text())
        self.assertEqual((t["status"], t["comment"], t["title"]), ("retour", "no good", "T"))
        self.assertRegex(t["reviewed"], r"^\d{4}-\d{2}-\d{2}T")
        self.assertEqual([x for x in os.listdir(os.path.dirname(self.entry)) if x.startswith(".")], [])
        st = json.loads(self.req("GET", "/api/state")[2])
        self.assertEqual([(x["id"], x["status"]) for x in st["tests"]], [("1-1", "retour")])
        self.assertEqual(self.post(body={"status": "nonsense"})[0], 400)
        self.assertEqual(self.post("/api/tests/9-9")[0], 404)

    def test_etag_and_304(self):
        status, headers, body = self.req("GET", "/api/state")
        self.assertEqual(status, 200)
        tag = headers["ETag"]
        self.assertEqual(self.req("GET", "/api/state", {"If-None-Match": tag})[0], 304)
        self.post(body={"comment": "new"})
        status, headers2, _ = self.req("GET", "/api/state", {"If-None-Match": tag})
        self.assertEqual(status, 200)
        self.assertNotEqual(headers2["ETag"], tag)

    def test_state_colours_match_status_json(self):
        st = json.loads(self.req("GET", "/api/state")[2])
        self.assertEqual(st["colours"], self.status()["colours"])
        self.assertEqual(st["paused"], self.status()["paused"])
        self.assertEqual(st["pending"], 1)

    def test_open_qcm_and_page(self):
        os.makedirs(os.path.join(self.state, "qcm", "q1"))
        with open(os.path.join(self.state, "qcm", "q1", "meta.json"), "w") as f:
            json.dump({"issue": 1, "title": "Form", "cards": 5}, f)
        st = json.loads(self.req("GET", "/api/state")[2])
        self.assertEqual([(q["id"], q["issue"], q["progress"]) for q in st["qcms"]], [("q1", 1, {"answered": 0, "total": 5})])
        self.assertEqual(st["pending"], 2)
        status, headers, body = self.req("GET", "/")
        self.assertEqual(status, 200)
        self.assertIn(b"<title>Suivi StokesForge</title>", body)
        self.assertEqual(self.req("GET", "/qcm/nope")[0], 404)

    def test_decision_form_has_a_home_link_shown_only_when_local(self):
        os.makedirs(os.path.join(self.state, "qcm", "q1"))
        with open(os.path.join(self.state, "qcm", "q1", "meta.json"), "w") as f:
            json.dump({"issue": 1, "title": "Form", "cards": 1}, f)
        with open(os.path.join(self.state, "qcm", "q1", "data.js"), "w") as f:
            f.write("const TITRE='T';const SEC={};const ISSUES={};const INTRO='';const D=[];\n")
        status, _, body = self.req("GET", "/qcm/q1")
        self.assertEqual(status, 200)
        page = body.decode()
        self.assertRegex(page, r'<a class="link" id="goHome" href="/" hidden[^>]*>← Retour à l’accueil</a>')
        self.assertIn("if(LOCAL)document.getElementById('goHome').hidden=false", page)

    def test_port_in_use_exits_1(self):
        r = subprocess.run([sys.executable, "-I", SUIVI, "serve", "--port", str(self.port), "--state", self.state],
                           cwd=self.repo, env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(r.returncode, 1)
        self.assertIn("already in use", r.stderr)


DATA = ("const TITRE='T';\nconst SEC={};\nconst ISSUES={a:{n:1,name:'One',colour:'\U0001F7E6'}};\nconst INTRO='';\n"
        "const D=[\n{c:'R1',p:1,t:'Q1',is:'a',o:[O('Yes',['x'],'','p','m'),O('No')]},\n"
        "{c:'R2',p:2,t:'Q2',is:'a',o:[O('A'),O('B')]},\n];\n")
TEMPLATE = os.path.join(HERE, os.pardir, ".claude", "skills", "stokesforge-bug-report", "references", "decision-form.html")


def node_check(js):
    with tempfile.TemporaryDirectory() as tmp:
        p = os.path.join(tmp, "page.js")
        with open(p, "w", encoding="utf-8") as f:
            f.write(js)
        return subprocess.run(["node", "--check", p], capture_output=True, text=True)


class TestQcm(ServerBase):
    def setUp(self):
        super().setUp()
        self.assertEqual(self.run_cli("pause", "1").returncode, 0)     # only a form may hold the colour of #1
        os.remove(self.entry)
        self.data = os.path.join(self.tmp.name, "data.js")
        with open(self.data, "w", encoding="utf-8") as f:
            f.write(DATA)

    def pose(self, issue=1, data=None):
        r = self.run_cli("qcm", "pose", str(issue), data or self.data, "--title", "My <form>")
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout.split()[0]

    def answer(self, qid, code="R1", body=None, headers="ok"):
        h = {"X-Suivi": "1", "Content-Type": "application/json"} if headers == "ok" else headers
        b = {"code": code, "priorite": 1, "question": "Q1", "issue": 1, "statut": "tranche", "choix": "Yes",
             "recommande": True, "commentaire": "ok then", "date": "2026-11-03T10:00:00"} if body is None else body
        return self.req("POST", "/api/qcm/%s/answer" % qid, h, b)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_injects_the_data_block_and_title(self):
        qid = self.pose()
        self.assertEqual(qid, "1-1")
        self.assertEqual(self.pose(), "1-2")
        status, headers, body = self.req("GET", "/qcm/1-1")
        self.assertEqual(status, 200)
        text = body.decode()
        self.assertIn("{c:'R2',p:2,t:'Q2'", text)
        self.assertIn("<title>My &lt;form&gt;</title>", text)
        # lang: fr
        self.assertEqual(text.count("DONNÉES — début"), 1)
        self.assertEqual(self.req("GET", "/qcm/9-9")[0], 404)
        self.assertEqual(self.req("GET", "/qcm/..%2fx")[0], 400)
        meta = jload(os.path.join(self.state, "qcm", "1-1", "meta.json"))
        self.assertEqual((meta["issue"], meta["cards"], meta["finished"]), (1, 2, False))

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_refuses_a_syntax_error_and_writes_nothing(self):
        bad = os.path.join(self.tmp.name, "bad.js")
        with open(bad, "w") as f:
            f.write(DATA.replace("const INTRO='';", "const INTRO=;"))
        r = self.run_cli("qcm", "pose", "1", bad)
        self.assertEqual(r.returncode, 1)
        self.assertIn("syntax error", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm")))

    def test_pose_refuses_data_without_cards(self):
        empty = os.path.join(self.tmp.name, "empty.js")
        with open(empty, "w") as f:
            f.write("const D=[];\n")
        r = self.run_cli("qcm", "pose", "1", empty)
        self.assertEqual(r.returncode, 1)
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm")))

    def test_answer_is_written_and_read_back_by_answers(self):
        qid = self.pose()
        self.assertEqual(self.answer(qid)[0], 200)
        f = os.path.join(self.state, "qcm", qid, "answers", "R1.json")
        self.assertEqual(jload(f)["choix"], "Yes")
        r = self.run_cli("qcm", "answers", qid)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("R1 [recommended] Q1 -> Yes -- ok then", r.stdout)
        self.assertIn("1/2 answered, open", r.stdout)
        j = json.loads(self.run_cli("qcm", "answers", qid, "--json").stdout)
        self.assertEqual(j["answers"]["R1"]["commentaire"], "ok then")
        got = json.loads(self.req("GET", "/api/qcm/%s/answers" % qid)[2])
        self.assertEqual((got["finished"], list(got["answers"])), (False, ["R1"]))
        self.assertEqual(self.req("GET", "/api/qcm/9-9/answers")[0], 404)

    def test_answer_guards(self):
        qid = self.pose()
        self.assertEqual(self.answer(qid, headers={"Content-Type": "application/json"})[0], 403)
        self.assertEqual(self.answer(qid, body={"code": "../x"})[0], 400)
        self.assertEqual(self.answer(qid, body={"code": ".hid"})[0], 400)
        self.assertEqual(self.answer(qid, body={"nocode": 1})[0], 400)
        self.assertEqual(self.answer("9-9")[0], 404)
        h = {"X-Suivi": "1", "Origin": "http://evil.example"}
        self.assertEqual(self.req("POST", "/api/qcm/%s/answer" % qid, h, {"code": "R1"})[0], 403)
        big = {"code": "R1", "commentaire": "x" * 300000}
        self.assertEqual(self.answer(qid, body=big)[0], 413)
        self.assertEqual(os.listdir(os.path.join(self.state, "qcm", qid, "answers")), [])

    def test_finish_marks_the_form_and_frees_the_colour(self):
        qid = self.pose()
        self.assertEqual(self.held(C[0]), ["qcm"])
        self.assertEqual(self.req("POST", "/api/qcm/%s/finish" % qid, {}, {})[0], 403)
        self.assertEqual(self.held(C[0]), ["qcm"])
        self.assertEqual(self.req("POST", "/api/qcm/%s/finish" % qid, {"X-Suivi": "1"}, {})[0], 200)
        meta = jload(os.path.join(self.state, "qcm", qid, "meta.json"))
        self.assertTrue(meta["finished"])
        self.assertRegex(meta["finished_at"], r"^\d{4}-\d{2}-\d{2}T")
        self.assertEqual(self.held(C[0]), [])
        self.assertEqual(json.loads(self.req("GET", "/api/state")[2])["qcms"], [])

    def test_finish_keeps_the_colour_while_another_form_is_open(self):
        a, b = self.pose(), self.pose()
        self.req("POST", "/api/qcm/%s/finish" % a, {"X-Suivi": "1"}, {})
        self.assertEqual(self.held(C[0]), ["qcm"])

    def test_open_form_is_listed_with_progress(self):
        qid = self.pose()
        self.answer(qid)
        self.answer(qid, code="R2", body={"code": "R2", "statut": "plus_tard"})
        q = json.loads(self.req("GET", "/api/state")[2])["qcms"]
        self.assertEqual([(x["id"], x["issue"], x["progress"]) for x in q], [(qid, 1, {"answered": 1, "total": 2})])
        out = self.run_cli("qcm", "list", "--open").stdout
        self.assertIn("%s\tissue #1\t1/2\topen\tMy <form>" % qid, out)
        self.assertEqual(self.run_cli("qcm", "list", "--issue", "2").stdout, "")


@unittest.skipUnless(shutil.which("node"), "node is not installed")
class TestFormTemplate(unittest.TestCase):
    def script(self, html):
        return re.search(r"<script>(.*)</script>", html, re.S).group(1)

    def test_template_script_parses_alone_and_with_injected_data(self):
        with open(TEMPLATE, encoding="utf-8") as f:
            html = f.read()
        r = node_check(self.script(html))
        self.assertEqual(r.returncode, 0, r.stderr)
        r = node_check(self.script(suivi.inject_qcm(DATA, "T")))
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_template_keeps_both_storage_modes_and_the_sketch_helpers(self):
        with open(TEMPLATE, encoding="utf-8") as f:
            html = f.read()
        for needle in ("claude.use('db')", "/api/qcm/", "X-Suivi", "const slots=", ".mslot", "const flow=", "const sheet=", "const tree="):
            self.assertIn(needle, html)



FAKE_GLAB = r"""#!/usr/bin/env bash
# Fake glab: behaviour is driven by files in $FAKE_DIR (sha, pipelines, jobs, mode).
echo "$*" >> "$FAKE_DIR/calls"
case "$(cat "$FAKE_DIR/mode" 2>/dev/null)" in
    fail) exit 1 ;;
    sleep) sleep 30 ;;
    empty) exit 0 ;;
esac
case "$*" in
    *repository/commits*) echo "[{\"id\": \"$(cat "$FAKE_DIR/sha")\"}]" ;;
    *pipelines*) cat "$FAKE_DIR/pipelines" ;;
    *graphql*) cat "$FAKE_DIR/jobs" ;;
    *) echo "[]" ;;
esac
"""
SHA_A = "a" * 40
SHA_B = "b" * 40


class CiBase(unittest.TestCase):
    """The CI watcher with a fake glab on PATH and a local stand-in for the GitHub API."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = os.path.join(self.tmp.name, "fake")
        self.bin = os.path.join(self.tmp.name, "bin")
        self.state = os.path.join(self.tmp.name, "state")
        for d in (self.dir, self.bin, self.state):
            os.makedirs(d)
        with open(os.path.join(self.bin, "glab"), "w") as f:
            f.write(FAKE_GLAB)
        os.chmod(os.path.join(self.bin, "glab"), 0o755)
        patcher = mock.patch.dict(os.environ, {
            "PATH": self.bin + os.pathsep + os.environ["PATH"], "FAKE_DIR": self.dir,
            "SF_SUIVI_DIR": self.state, "SF_SUIVI_CI_TIMEOUT": "60"})
        patcher.start()
        self.addCleanup(patcher.stop)
        os.environ.pop("SF_SUIVI_CI", None)
        self.head(SHA_A)
        self.jobs()

    def put_file(self, name, text):
        with open(os.path.join(self.dir, name), "w") as f:
            f.write(text)

    def head(self, sha, pipeline=True):
        self.put_file("sha", sha)
        self.put_file("pipelines", '[{"iid": 5}]' if pipeline else "[]")

    def jobs(self, **status):
        nodes = [{"name": "github-actions/" + j, "status": status.get(j.replace("-", "_"), "SUCCESS"),
                  "detailedStatus": {"label": "x"}} for j in suivi.CI_JOBS]
        for j, st in status.items():
            if st is None:
                nodes = [n for n in nodes if n["name"] != "github-actions/" + j.replace("_", "-")]
        data = {"data": {"project": {"pipeline": {"stages": {"nodes": [{"name": "external", "jobs": {"nodes": nodes}}]}}}}}
        self.put_file("jobs", json.dumps(data))

    def poll(self, prev=None, now=None):
        return suivi.ci_poll(prev, now)


class TestCiVerdict(CiBase):
    def test_all_blocking_success_is_green(self):
        r = self.poll()
        self.assertEqual((r["verdict"], r["sha"]), ("green", SHA_A))

    def test_missing_job_is_pending_not_green(self):
        self.jobs(clippy=None)
        r = self.poll()
        self.assertEqual((r["verdict"], r["names"]), ("pending", ["clippy"]))

    def test_failed_is_red_with_every_name(self):
        self.jobs(fmt="FAILED", test="FAILED", clippy="RUNNING")
        r = self.poll()
        self.assertEqual((r["verdict"], r["names"]), ("red", ["fmt", "test"]))

    def test_robustness_skipped_or_missing_stays_green(self):
        self.jobs(robustness="SKIPPED")
        self.assertEqual(self.poll()["verdict"], "green")
        self.jobs(robustness="FAILED")
        self.assertEqual(self.poll()["verdict"], "green")

    def test_canceled_is_unknown(self):
        self.jobs(test="CANCELED")
        self.assertEqual(self.poll()["verdict"], "unknown")

    def test_glab_failure_timeout_or_empty_is_unknown(self):
        for mode in ("fail", "empty", "sleep"):
            self.put_file("mode", mode)
            if mode == "sleep":
                os.environ["SF_SUIVI_CI_TIMEOUT"] = "1"
            r = self.poll()
            self.assertEqual(r["verdict"], "unknown", mode)

    def test_no_pipeline_is_pending_then_unknown(self):
        self.head(SHA_A, pipeline=False)
        r = self.poll(now=1000)
        self.assertEqual(r["verdict"], "pending")
        r = self.poll(r, now=1000 + suivi.CI_PENDING_MAX + 5)
        self.assertEqual(r["verdict"], "unknown")

    def test_blocking_list_equals_gitlab_status(self):
        with open(os.path.join(HERE, "gitlab-status.py")) as f:
            m = re.search(r"^JOBS = \((.*?)\)", f.read(), re.M | re.S)
        self.assertEqual(tuple(re.findall(r'"([^"]+)"', m.group(1))), suivi.CI_JOBS)

    def test_ci_status_script_has_no_unknown_glab_use_outside_head_mode(self):
        r = subprocess.run(["bash", os.path.join(HERE, "ci-status.sh"), "--head"], capture_output=True, text=True,
                           env=dict(os.environ))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(r.stdout.startswith("HEAD " + SHA_A), r.stdout)
        self.assertNotIn("commit comments", r.stdout)
        calls = slurp(os.path.join(self.dir, "calls"))
        self.assertNotIn("ls-remote", calls)
        self.assertNotIn("comments", calls)


class TestCiWindows(CiBase):
    def test_windows_failure_turns_red_with_names(self):
        self.jobs(**{"dist_windows": "FAILED"})
        r = self.poll()
        self.assertEqual((r["verdict"], r["names"]), ("red", ["dist-windows"]))

    def test_windows_pending_or_missing_is_not_green(self):
        self.jobs(**{"test_windows": "RUNNING"})
        self.assertEqual(self.poll()["verdict"], "pending")
        self.jobs(**{"test_python_windows": None})
        self.assertEqual(self.poll()["verdict"], "pending")

    def test_windows_jobs_are_in_the_blocking_list_and_no_github_read_remains(self):
        for j in ("test-windows", "test-python-windows", "dist-windows"):
            self.assertIn(j, suivi.CI_JOBS)
        for gone in ("ci_windows_verdict", "ci_github_runs", "CI_WINDOWS_JOBS", "GITHUB_API"):
            self.assertFalse(hasattr(suivi, gone), gone)
        self.assertNotIn("SF_SUIVI_GITHUB", slurp(SUIVI))


class TestCiHeads(CiBase):
    def test_red_stays_until_a_newer_green(self):
        self.jobs(fmt="FAILED")
        r = self.poll()
        self.assertEqual(r["last_red"], {"sha": SHA_A, "names": ["fmt"]})
        self.head(SHA_B)
        self.jobs(fmt="RUNNING")
        r = self.poll(r)
        self.assertEqual((r["verdict"], r["sha"]), ("pending", SHA_B))
        self.assertEqual(r["last_red"]["sha"], SHA_A)
        self.jobs()
        r = self.poll(r)
        self.assertEqual(r["verdict"], "green")
        self.assertIsNone(r["last_red"])

    def test_red_rerun_green_on_same_sha_clears(self):
        self.jobs(fmt="FAILED")
        r = self.poll()
        r = self.poll(r)
        self.assertEqual(r["verdict"], "red")
        self.jobs()
        r = self.poll(r)
        self.assertEqual((r["verdict"], r["sha"]), ("green", SHA_A))
        self.assertIsNone(r["last_red"])


class TestCiState(ServerBase):
    def ci_file(self):
        return os.path.join(self.state, "ci.json")

    def state_json(self):
        st, _, body = self.req("GET", "/api/state")
        self.assertEqual(st, 200)
        return json.loads(body)

    def test_old_green_file_reads_unknown(self):
        suivi.write_json(self.ci_file(), {"checked_at": time.time() - 1000, "sha": SHA_A, "verdict": "green", "names": []})
        self.assertEqual(self.state_json()["ci"]["verdict"], "unknown")
        suivi.write_json(self.ci_file(), {"checked_at": time.time(), "sha": SHA_A, "verdict": "green", "names": []})
        self.assertEqual(self.state_json()["ci"]["verdict"], "green")

    def test_missing_or_corrupt_file_is_unknown_with_200(self):
        self.assertEqual(self.state_json()["ci"]["verdict"], "unknown")
        for junk in ("{not json", "[1,2]", '{"checked_at": "x", "verdict": "green"}'):
            with open(self.ci_file(), "w") as f:
                f.write(junk)
            self.assertEqual(self.state_json()["ci"]["verdict"], "unknown", junk)

    def test_checked_at_is_not_in_the_state(self):
        suivi.write_json(self.ci_file(), {"checked_at": time.time(), "sha": SHA_A, "verdict": "green", "names": []})
        self.assertNotIn("checked_at", self.state_json()["ci"])

    def test_red_ci_changes_neither_pending_nor_colours(self):
        before = self.state_json()
        suivi.write_json(self.ci_file(), {"checked_at": time.time(), "sha": SHA_A, "verdict": "red", "names": ["fmt"]})
        after = self.state_json()
        self.assertEqual(after["ci"]["verdict"], "red")
        for k in ("pending", "colours", "paused", "qcms", "tests"):
            self.assertEqual(before[k], after[k], k)


class TestCiService(Base):
    """The thread of `serve`, end to end, with the fake glab."""
    def start(self, **extra):
        self.fake = os.path.join(self.tmp.name, "fake")
        os.makedirs(self.fake)
        binp = os.path.join(self.tmp.name, "bin")
        os.makedirs(binp)
        with open(os.path.join(binp, "glab"), "w") as f:
            f.write(FAKE_GLAB)
        os.chmod(os.path.join(binp, "glab"), 0o755)
        for n, t in (("sha", SHA_A), ("pipelines", '[{"iid": 5}]'),
                     ("jobs", json.dumps({"data": {"project": {"pipeline": {"stages": {"nodes": [{"name": "e", "jobs": {"nodes": [
                         {"name": "github-actions/" + j, "status": "FAILED", "detailedStatus": {"label": "x"}} for j in suivi.CI_JOBS]}}]}}}}}))):
            with open(os.path.join(self.fake, n), "w") as f:
                f.write(t)
        env = dict(self.env, PATH=binp + os.pathsep + os.environ["PATH"], FAKE_DIR=self.fake, SF_SUIVI_CI_POLL="0.3", **extra)
        self.proc = subprocess.Popen([sys.executable, "-I", SUIVI, "serve", "--port", "0", "--state", self.state],
                                     cwd=self.repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop)
        m = re.search(r":(\d+)/", self.proc.stdout.readline())
        self.port = int(m.group(1))

    def stop(self):
        self.proc.terminate()
        self.proc.wait(5)
        self.proc.stdout.close()
        self.proc.stderr.close()

    def get_state(self):
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        c.request("GET", "/api/state")
        return json.loads(c.getresponse().read())

    def test_seam_off_never_calls_glab(self):
        os.makedirs(self.state)
        self.start(SF_SUIVI_CI="off")
        time.sleep(1.5)
        self.assertFalse(os.path.exists(os.path.join(self.fake, "calls")))
        self.assertEqual(self.get_state()["ci"]["verdict"], "unknown")

    def test_watcher_thread_publishes_red(self):
        os.makedirs(self.state)
        self.env.pop("SF_SUIVI_CI")
        self.start()
        for _ in range(50):
            ci = self.get_state()["ci"]
            if ci["verdict"] != "unknown":
                break
            time.sleep(0.2)
        self.assertEqual(ci["verdict"], "red", ci)


class TestPage(unittest.TestCase):
    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_script_parses(self):
        with open(os.path.join(HERE, "suivi-page.html"), encoding="utf-8") as f:
            html = f.read()
        js = re.search(r"<script>(.*)</script>", html, re.S).group(1)
        with tempfile.TemporaryDirectory() as tmp:
            p = os.path.join(tmp, "page.js")
            with open(p, "w") as f:
                f.write(js)
            r = subprocess.run(["node", "--check", p], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)

    def page(self):
        with open(os.path.join(HERE, "suivi-page.html"), encoding="utf-8") as f:
            return f.read()

    def ci_line(self, ci):
        src = re.search(r"/\*CILINE-START\*/(.*)/\*CILINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(ciLine(%s)));" % json.dumps(ci)
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_ci_line_red_visible_unknown_never_green(self):
        red = self.ci_line({"verdict": "red", "names": ["fmt", "clippy"]})
        self.assertEqual(red["cls"], "red")
        self.assertIn("fmt, clippy", red["text"])
        for ci in ({"verdict": "unknown", "names": []}, {"verdict": "weird"}, {"verdict": "unknown", "last_red": {"sha": "a", "names": ["fmt"]}}):
            u = self.ci_line(ci)
            self.assertNotIn("verte", u["text"])
            self.assertNotEqual(u["cls"], "green")
        self.assertEqual(self.ci_line({"verdict": "green", "names": []})["cls"], "green")
        p = self.ci_line({"verdict": "pending", "last_red": {"sha": "a", "names": ["fmt"]}})
        self.assertIn("fmt", p["text"])
        self.assertIsNone(self.ci_line(None))

    def test_ci_line_sits_above_the_tabs_outside_the_banner(self):
        html = self.page()
        self.assertLess(html.index('id="ciline"'), html.index('class="tabs"'))
        self.assertNotIn("'ciline'", html.split("function renderTests")[1].split("function render()")[0])

    def card_line(self, c, tests=(), qcms=()):
        src = re.search(r"/\*CARDLINE-START\*/(.*)/\*CARDLINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(cardLine(%s,%s,%s)));" % (
            json.dumps(c), json.dumps(list(tests)), json.dumps(list(qcms)))
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_card_line_says_whose_turn_it_is(self):
        card = {"issue": 7, "issues": [7], "package": None, "held_by": ["agent"], "waiting": "", "wait_kind": ""}
        todo = lambda n, st="todo": {"n": n, "status": st}
        L = self.card_line
        self.assertEqual(L(card, [], [{"issue": 7}])["text"], "À toi : répondre au questionnaire")        # lang: fr
        l = L(card, [todo(7), todo(7), todo(7), todo(7, "ok"), todo(8)])
        self.assertEqual((l["user"], l["text"], l["also"]), (True, "À toi : cocher 3 essais", "l’agent continue"))   # lang: fr
        self.assertEqual(L(card, [todo(7)])["text"], "À toi : cocher 1 essai")                              # lang: fr
        self.assertEqual(L(card, [todo(7, "ok")]), {"user": False, "text": "L’agent travaille", "also": "", "note": ""})   # lang: fr
        paused = dict(card, held_by=[])
        self.assertEqual(L(paused)["text"], "En pause")                                                     # lang: fr
        l = L(paused, [], [{"issue": 7}])
        self.assertEqual((l["user"], l["also"]), (True, ""))                                                # the agent is not working
        l = L(card, [], [{"issue": 7}])
        self.assertEqual((l["text"], l["also"]), ("À toi : répondre au questionnaire", "l’agent continue"))   # lang: fr
        # an agent note alone never makes it the user's turn
        l = L(dict(card, wait_kind="you", waiting="my note"))
        self.assertEqual((l["user"], l["text"], l["note"]), (False, "L’agent travaille", "my note"))        # lang: fr

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_card_line_counts_every_member_of_a_package(self):
        pkg = {"issue": 9, "issues": [9], "held_by": ["agent"], "package": {"parent": 9, "subs": [{"n": 11, "title": "a"}, {"n": 12, "title": "b"}]}}
        l = self.card_line(pkg, [{"n": 11, "status": "todo"}, {"n": 12, "status": "todo"}, {"n": 13, "status": "todo"}], [{"issue": 12}])
        self.assertEqual(l["text"], "À toi : répondre au questionnaire · cocher 2 essais")                  # lang: fr
        self.assertEqual(self.card_line(dict(pkg, package=None), [{"n": 11, "status": "todo"}])["user"], False)
        paused = {"issue": 9, "kind": "package", "subs": [{"n": 11, "title": "a"}]}                           # a paused box
        self.assertEqual(self.card_line(paused, [{"n": 11, "status": "todo"}])["text"], "À toi : cocher 1 essai")   # lang: fr

    def test_page_drops_the_held_colour_line_and_shows_the_intro(self):
        html = self.page()
        for gone in ("Couleur tenue", "T’attend", "waitEl", ".wait.dep"):                                   # lang: fr
            self.assertNotIn(gone, html)
        self.assertIn("x.intro", html)

    def test_legacy_row_highlights_no_state_step(self):
        with open(os.path.join(HERE, "suivi-page.html"), encoding="utf-8") as f:
            html = f.read()
        self.assertIn("h.includes('legacy')?-1:STATES.findIndex", html)

    def test_polls_only_while_visible(self):
        with open(os.path.join(HERE, "suivi-page.html"), encoding="utf-8") as f:
            html = f.read()
        self.assertIn("visibilitychange", html)
        self.assertIn("document.visibilityState==='visible'", html)
        self.assertIn("stopPolling()", html)

    def test_tab_icon_and_hash_filter(self):
        form = os.path.join(HERE, os.pardir, ".claude", "skills", "stokesforge-bug-report", "references", "decision-form.html")
        for path in (os.path.join(HERE, "suivi-page.html"), form):
            with open(path, encoding="utf-8") as f:
                self.assertIn('<link rel="icon" href="data:image/svg+xml', f.read())
        with open(os.path.join(HERE, "suivi-page.html"), encoding="utf-8") as f:
            html = f.read()
        self.assertIn("a.href='#attente/issue='+c.issue", html)
        self.assertIn("/^issue=(\\d+)$/", html)
        self.assertIn("x.reviewed||x.date", html)


class TestHelp(Base):
    def test_help_runs_isolated(self):
        r = self.run_cli("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("claim", r.stdout)


if __name__ == "__main__":
    unittest.main()
