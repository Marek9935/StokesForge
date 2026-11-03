#!/usr/bin/env python3
"""Tests of scripts/suivi.py (#441): the colour rule, claim, resume, the hook, the local server,
the trial-entry commands, the install scripts.

    python3 scripts/suivi-test.py

Everything runs in a throwaway repository and a throwaway state folder; the real
../_stokesforge/.suivi is never touched.
"""
import http.client
import importlib.util
import json
import os
import re
import shutil
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

    def test_add_merged_list(self):
        self.claim(7, "--name", "Seven")
        with tempfile.TemporaryDirectory() as tmp:
            one = {"title": "A", "where": ["File", "Open"], "steps": ["Do"], "expect": "See", "try": "scripts/try.sh 7"}
            r = self.run_cli("test", "add", "7", self.entries(tmp, [one, {"title": "B", "try": "scripts/try.sh 7"}]))
            self.assertEqual((r.returncode, r.stdout.strip()), (0, "7-1 7-2"), r.stderr)
            r = self.run_cli("test", "add", "7", self.entries(tmp, one))
            self.assertEqual(r.stdout.strip(), "7-3")
        t = jload(os.path.join(self.state, "tests", "7-1.json"))
        self.assertEqual((t["status"], t["n"], t["name"], t["colour"], t["date"]), ("todo", 7, "Seven", C[0], "2026-11-03"))
        self.assertEqual(self.held(C[0]), ["agent", "essai"])
        out = self.run_cli("test", "list", "--issue", "7").stdout
        self.assertEqual(len(out.strip().splitlines()), 3)
        self.assertIn("(try)", out)
        r = self.run_cli("test", "merged", "7", "abcdef12")
        self.assertIn("3 entries", r.stdout)
        t = jload(os.path.join(self.state, "tests", "7-1.json"))
        self.assertEqual((t["commit"], "try" in t), ("abcdef12", False))
        self.assertEqual(self.held(C[0]), ["agent"])

    def test_rejects_unknown_fields_and_bad_commit(self):
        with tempfile.TemporaryDirectory() as tmp:
            r = self.run_cli("test", "add", "7", self.entries(tmp, {"title": "A", "status": "ok"}))
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("unknown field", r.stderr)
            r = self.run_cli("test", "add", "7", self.entries(tmp, {"steps": "not a list"}))
            self.assertNotEqual(r.returncode, 0)
        self.assertFalse(os.path.exists(os.path.join(self.state, "tests")))
        self.assertNotEqual(self.run_cli("test", "merged", "7", "../x").returncode, 0)


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


class TestHelp(Base):
    def test_help_runs_isolated(self):
        r = self.run_cli("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("claim", r.stdout)


if __name__ == "__main__":
    unittest.main()
