#!/usr/bin/env python3
"""Tests of scripts/suivi.py (#441): the colour rule, claim, resume, the hook.

    python3 scripts/suivi-test.py

Everything runs in a throwaway repository and a throwaway state folder; the real
../_stokesforge/.suivi is never touched.
"""
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SUIVI = os.path.join(HERE, "suivi.py")
spec = importlib.util.spec_from_file_location("suivi", SUIVI)
suivi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(suivi)
C = suivi.COLOURS  # blue green orange purple red yellow


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


class TestHelp(Base):
    def test_help_runs_isolated(self):
        r = self.run_cli("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("claim", r.stdout)


if __name__ == "__main__":
    unittest.main()
