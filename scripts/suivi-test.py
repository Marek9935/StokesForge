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


# Minimal files with the real magic bytes (#456); the server only looks at the first bytes.
IMG = {"png": b"\x89PNG\r\n\x1a\n" + b"png-body", "jpg": b"\xff\xd8\xff\xe0" + b"jpg-body",
       "gif": b"GIF89a" + b"gif-body", "webp": b"RIFF\x10\x00\x00\x00WEBPVP8 body"}
IMG_NAME = re.compile(r"[0-9a-f]{24}\.(png|jpg|gif|webp)")


def png(tag=b""):
    return IMG["png"] + tag


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
        # The build modes (#476) never touch the real queue or the real systemd: own state, a fake systemctl.
        self.cqdir = os.path.join(self.tmp.name, "cq")
        fake = os.path.join(self.tmp.name, "fakebin")
        os.makedirs(fake)
        with open(os.path.join(fake, "systemctl"), "w") as f:
            f.write('#!/bin/sh\necho "$*" >> "%s/ctl.log"\n' % self.tmp.name)
        os.chmod(os.path.join(fake, "systemctl"), 0o755)
        # #477: a canary busctl (the power profile of this PC must never be reached) and a user who is never idle
        self.buslog = os.path.join(self.tmp.name, "bus.log")
        for name, body in (("busctl", '#!/bin/sh\necho "$*" >> "%s"\nexit 1\n' % self.buslog),
                           ("gdbus", '#!/bin/sh\necho "(uint64 0,)"\n')):
            with open(os.path.join(fake, name), "w") as f:
                f.write(body)
            os.chmod(os.path.join(fake, name), 0o755)
        self.addCleanup(self.check_canary)
        self.env.update(SF_CARGO_QUEUE_POWER_DIR=os.path.join(self.tmp.name, "nopower"), SF_CARGO_QUEUE_DIR=self.cqdir, SF_CARGO_QUEUE_SLICE="sfq-test.slice", SF_SUIVI_BUILD_AUTO="off",
                        SF_CARGO_QUEUE_INSTALLED=os.path.join(self.tmp.name, "installed-cargo"),
                        PATH=fake + os.pathsep + self.env.get("PATH", "/usr/bin:/bin"))

    def check_canary(self):
        self.assertFalse(os.path.exists(self.buslog), "a test reached busctl: " + slurp(self.buslog) if os.path.exists(self.buslog) else "")

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


class TestWaitYouNeedsACard(Base):
    """#483: `set --wait you` is a complement; the action itself is a card on the page."""

    qcm = TestQcmAndTrials.qcm
    trial = TestQcmAndTrials.trial

    def setUp(self):
        super().setUp()
        self.claim(1)

    def wait_you(self, *extra):
        return self.run_cli("set", "1", "--wait", "you", "--waiting", "run it", *extra)

    def test_refused_with_no_card_and_names_the_ways_out(self):
        r = self.wait_you()
        self.assertEqual(r.returncode, 1)
        for word in ("test add", "qcm pose", "--force"):
            self.assertIn(word, r.stderr)
        self.assertEqual(self.rec(1)["wait_kind"], "")

    def test_accepted_with_a_todo_entry_to_try(self):
        self.trial(1, 1, status="todo", **{"try": "scripts/try.sh 1"})
        self.assertEqual(self.wait_you().returncode, 0)

    def test_refused_when_the_only_entry_is_ok_or_has_no_try(self):
        self.trial(1, 1, status="ok", **{"try": "scripts/try.sh 1"})
        self.trial(1, 2, status="todo")
        self.assertEqual(self.wait_you().returncode, 1)

    def test_accepted_with_an_open_form(self):
        self.qcm(1, False)
        self.assertEqual(self.wait_you().returncode, 0)

    def test_refused_when_the_form_is_finished(self):
        self.qcm(1, True)
        self.assertEqual(self.wait_you().returncode, 1)

    def test_force_overrides_and_asks_for_a_justification(self):
        r = self.wait_you("--force")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("justify", r.stderr)
        self.assertEqual(self.rec(1)["wait_kind"], "you")

    def test_other_wait_kinds_need_no_card(self):
        self.assertEqual(self.run_cli("set", "1", "--wait", "dep", "--waiting", "x").returncode, 0)

    def test_entry_kind_measure_accepted_other_refused(self):
        f = os.path.join(self.state, "e.json")
        os.makedirs(self.state, exist_ok=True)
        for kind, rc in (("measure", 0), ("trial", 1)):
            with open(f, "w") as fh:
                json.dump({"title": "t", "try": "echo hi", "kind": kind, "goal": "g", "around": "a"}, fh)
            r = self.run_cli("test", "add", "1", f)
            self.assertEqual(r.returncode, rc, r.stderr)
        self.assertIn("kind", r.stderr)
        self.assertEqual(self.run_cli("test", "list", "--issue", "1").returncode, 0)



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
        r = self.run_cli("set", "1", "--state", "essai", "--what", "trial", "--wait", "you", "--waiting", "your OK", "--force")
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

    def test_a_withdrawn_form_no_longer_keeps_the_colour(self):
        self.put(11, colour=C[1], owner=None)
        d = os.path.join(self.state, "qcm", "11-1")
        os.makedirs(d)
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump({"issue": 11, "finished": False}, f)
        # lang: fr begin
        self.assertIn("reste prise : un QCM de l'issue est ouvert", self.run_cli("release-line", "11").stdout)
        os.makedirs(os.path.join(self.state, "removed-qcm"))
        os.rename(d, os.path.join(self.state, "removed-qcm", "11-1"))
        self.assertEqual(self.run_cli("release-line", "11").stdout.strip(), "Couleur %s libérée." % C[1])
        # lang: fr end

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


def suivi_state_counts(state):
    """pending, trial_pending and is_answered of the entries of a state folder, read in a child process."""
    code = ("import json,sys;sys.path.insert(0,%r);import suivi;"
            "print(json.dumps([suivi.build_state()['pending'],suivi.trial_pending(7),"
            "[suivi.is_answered(t) for t in suivi.load_tests(7)]]))" % os.path.dirname(SUIVI))
    return subprocess.run([sys.executable, "-I", "-c", code], env=dict(os.environ, SF_SUIVI_DIR=state, SF_SUIVI_CI="off"),
                          capture_output=True, text=True, check=True).stdout




class TestTestCommands(Base):
    def entries(self, tmp, data):
        p = os.path.join(tmp, "e.json")
        with open(p, "w") as f:
            json.dump(data, f)
        return p

    ONE = {"title": "A", "where": ["File", "Open"], "steps": ["Do"], "expect": "See", "try": "scripts/try.sh 7",
           "goal": "g", "around": "a"}

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
        return {f: slurp(os.path.join(d, f)) for f in sorted(os.listdir(d)) if os.path.isfile(os.path.join(d, f))} if os.path.isdir(d) else {}

    def put_test(self, tid, **fields):
        os.makedirs(os.path.join(self.state, "tests"), exist_ok=True)
        with open(self.tfile(tid), "w") as f:
            json.dump(dict({"title": tid, "status": "todo", "n": int(tid.split("-")[0])}, **fields), f)

    def merged(self, n, commit, *extra):
        return self.run_cli("test", "merged", str(n), commit, *extra)

    def test_add_merged_list(self):
        self.claim(7, "--name", "Seven")
        r = self.add(7, [self.ONE, {"title": "B", "try": "scripts/try.sh 7", "goal": "g"}])
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

    def put_image(self, tid, name, data=None):
        d = os.path.join(self.state, "tests", tid + ".images")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, name), "wb") as f:
            f.write(data or png())
        return os.path.join(d, name)

    NAME = "0123456789abcdef01234567.png"

    def test_list_prints_the_absolute_path_of_each_image_and_nothing_without(self):
        self.put_test("7-1", **{"try": "t"})
        plain = self.run_cli("test", "list", "--issue", "7").stdout
        self.assertEqual(plain, "7-1\ttodo\t7-1\t(try)\n")
        path = self.put_image("7-1", self.NAME)
        self.put_test("7-1", images=[self.NAME, "ffffffffffffffffffffffff.png"], **{"try": "t"})   # the 2nd file is missing
        out = self.run_cli("test", "list", "--issue", "7").stdout
        self.assertEqual(out, plain + "  image: %s\n  image removed by the size limit: ffffffffffffffffffffffff.png\n" % path)
        self.assertTrue(os.path.isabs(path) and os.path.exists(path))

    def test_qcm_answers_show_the_image_paths_at_display_time_only(self):
        qd = os.path.join(self.state, "qcm", "7-1")
        os.makedirs(os.path.join(qd, "answers"))
        with open(os.path.join(qd, "meta.json"), "w") as f:
            json.dump({"issue": 7, "title": "T", "cards": 1, "finished": False}, f)
        ans = {"code": "R1", "question": "Q", "choix": "Yes", "statut": "tranche"}
        with open(os.path.join(qd, "answers", "R1.json"), "w") as f:
            json.dump(ans, f)
        text, js = (self.run_cli("qcm", "answers", "7-1", *x).stdout for x in ((), ("--json",)))
        self.assertNotIn("image", text + js)
        path = os.path.join(qd, "images", "R1", self.NAME)
        os.makedirs(os.path.dirname(path))
        with open(path, "wb") as f:
            f.write(png())
        with open(os.path.join(qd, "answers", "R1.json"), "w") as f:
            json.dump(dict(ans, images=[self.NAME]), f)
        out = self.run_cli("qcm", "answers", "7-1").stdout
        self.assertEqual(out, text + "  image: %s\n" % path)
        got = json.loads(self.run_cli("qcm", "answers", "7-1", "--json").stdout)["answers"]["R1"]
        self.assertEqual((got["images"], got["image_paths"]), ([self.NAME], [path]))
        self.assertNotIn("image_paths", jload(os.path.join(qd, "answers", "R1.json")))

    def test_remove_treats_an_image_as_an_answer_and_archives_the_folder(self):
        self.claim(7)
        self.put_test("7-1", images=[self.NAME], **{"try": "t"})
        self.put_image("7-1", self.NAME)
        self.put_test("7-2", **{"try": "t"})
        before = self.snap_tests()
        r = self.remove(7, "7-1")
        self.assertNotEqual(r.returncode, 0)                                                # image-only counts as answered
        self.assertEqual(self.snap_tests(), before)
        self.assertTrue(os.path.isdir(os.path.join(self.state, "tests", "7-1.images")))
        r = self.run_cli("test", "remove", "7", "7-1", "7-2", "--answered", env=self.own())
        self.assertEqual(r.returncode, 0, r.stderr)
        arch = os.path.join(self.state, "removed-tests", "7-1.images")
        self.assertIn("images archived: " + arch, r.stdout)
        self.assertEqual(os.listdir(arch), [self.NAME])
        self.assertFalse(os.path.exists(os.path.join(self.state, "tests", "7-1.images")))
        self.assertEqual(jload(os.path.join(self.state, "removed-tests", "7-1.json"))["images"], [self.NAME])
        self.assertNotIn("images archived: " + os.path.join(self.state, "removed-tests", "7-2.images"), r.stdout)

    def test_merged_keeps_the_images(self):
        self.claim(7)
        self.put_test("7-1", images=[self.NAME], **{"try": "t"})
        self.put_image("7-1", self.NAME)
        h = self.feature(7)
        self.assertEqual(self.merged(7, h).returncode, 0)
        t = jload(self.tfile("7-1"))
        self.assertEqual((t["images"], "try" in t), ([self.NAME], False))
        self.assertTrue(os.path.exists(os.path.join(self.state, "tests", "7-1.images", self.NAME)))

    def test_set_erases_the_agent_note_unless_restated(self):
        self.claim(7)
        self.run_cli("set", "7", "--state", "code", "--what", "w", "--wait", "you", "--waiting", "note", "--force")
        self.run_cli("set", "7", "--what", "other")
        r = self.rec(7)
        self.assertEqual((r["state"], r["what"], r["waiting"], r["wait_kind"]), ("code", "other", "", ""))
        self.run_cli("set", "7", "--wait", "dep", "--waiting", "n2")
        self.run_cli("set", "7", "--wait", "dep", "--waiting", "n2", "--what", "z")
        r = self.rec(7)
        self.assertEqual((r["what"], r["waiting"], r["wait_kind"]), ("z", "n2", "dep"))


    def settle(self, *args, sid=None):
        return self.run_cli("test", "settle", *args, env=self.as_session(sid) if sid else None)

    def test_settle_a_retour_keeps_its_status(self):
        self.put_test("7-1", status="retour", comment="fix it", **{"try": "scripts/try.sh 7"})
        r = self.settle("7-1", "--note", "  raw « text »  ")
        self.assertEqual(r.returncode, 0, r.stderr)
        t = jload(self.tfile("7-1"))
        self.assertEqual((t["status"], t["comment"]), ("retour", "fix it"))
        self.assertEqual((t["settled"]["by"], t["settled"]["note"]), ("agent", "  raw « text »  "))
        self.assertRegex(t["settled"]["at"], r"^\d{4}-\d\d-\d\dT")
        self.assertNotIn("commit", t["settled"])
        self.assertIn("(settled)", self.run_cli("test", "list", "--issue", "7").stdout)

    def test_settle_refuses_todo_ok_missing_and_twice(self):
        self.put_test("7-1", status="todo")
        self.put_test("7-2", status="ok")
        self.put_test("7-3", status="retour")
        for tid in ("7-1", "7-2"):
            before = slurp(self.tfile(tid))
            r = self.settle(tid)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("only a retour", r.stderr)
            self.assertEqual(slurp(self.tfile(tid)), before)
        self.assertNotEqual(self.settle("7-9").returncode, 0)
        self.assertNotEqual(self.settle("bad").returncode, 0)
        self.assertEqual(self.settle("7-3").returncode, 0)
        before = slurp(self.tfile("7-3"))
        r = self.settle("7-3", "--note", "again")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("already settled", r.stderr)
        self.assertEqual(slurp(self.tfile("7-3")), before)

    def test_settle_an_ok_with_an_idea_but_not_a_plain_ok(self):
        self.put_test("7-1", status="ok", idea="more", **{"try": "x"})
        self.put_test("7-2", status="ok", **{"try": "x"})
        self.assertEqual(self.run_cli("test", "unsettled", "7", "--ideas").stdout.split(), ["7-1"])
        self.assertIn("(idea)", self.run_cli("test", "list", "--issue", "7").stdout)
        self.assertEqual(self.run_cli("test", "unsettled", "7").returncode, 0)                 # an idea never blocks
        self.assertNotEqual(self.settle("7-2").returncode, 0)
        self.assertEqual(self.settle("7-1", "--note", "#12").returncode, 0)
        self.assertEqual(jload(self.tfile("7-1"))["settled"]["note"], "#12")
        self.assertEqual(self.run_cli("test", "unsettled", "7", "--ideas").stdout.split(), [])
        self.assertNotIn("(idea)", self.run_cli("test", "list", "--issue", "7").stdout)

    def test_check_lists_an_unsettled_idea_of_a_done_issue(self):
        self.put(5, colour=None, done_at="2026-11-01")
        self.put_test("5-1", status="ok", idea="more")
        self.put_test("5-2", status="ok", idea="more", settled={"at": "t", "by": "agent"})
        out = self.run_cli("check").stdout
        self.assertIn("5-1", out)
        self.assertNotIn("5-2", out)

    def test_add_needs_a_goal_on_a_routed_entry_and_an_around_in_the_batch(self):
        full = dict(self.ONE)
        no_goal = {k: v for k, v in full.items() if k != "goal"}
        r = self.add(7, [no_goal])
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("goal", r.stderr)
        r = self.add(7, [dict(full, goal="  ")])
        self.assertNotEqual(r.returncode, 0)
        no_around = {k: v for k, v in full.items() if k != "around"}
        r = self.add(7, [no_around, dict(no_around, title="B")])
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("around", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.state, "tests")))
        self.assertEqual(self.add(7, [no_around, dict(full, title="B")]).returncode, 0)   # around on one entry is enough
        self.assertEqual(self.add(7, [{"title": "m", "try": "x", "kind": "measure"}]).returncode, 0)   # a measure needs neither
        self.assertEqual(self.add(7, [{"title": "n"}]).returncode, 0)                    # nor an entry without `try`

    def test_settle_commit_must_exist_but_need_not_cite_the_issue(self):
        self.put_test("7-1", status="retour", commit="m" * 40)
        self.put_test("7-2", status="retour")
        before = slurp(self.tfile("7-1"))
        r = self.settle("7-1", "--commit", "deadbeef0000")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("no such commit", r.stderr)
        self.assertEqual(slurp(self.tfile("7-1")), before)
        full = self.gout("rev-parse", "HEAD")             # the "init" commit: cites no issue
        self.assertEqual(self.settle("7-1", "--commit", full[:8]).returncode, 0)
        t = jload(self.tfile("7-1"))
        self.assertEqual((t["settled"]["commit"], t["commit"]), (full, "m" * 40))

    def test_settle_ownership(self):
        self.put_test("7-1", status="retour")
        self.put_test("8-1", status="retour")
        self.put_test("9-1", status="retour")
        self.put(8, colour=C[0], owner=self.live_owner("OTHER"))
        r = self.settle("8-1", sid="S1")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("another live session", r.stderr)
        self.assertNotIn("settled", jload(self.tfile("8-1")))
        self.assertNotEqual(self.settle("8-1").returncode, 0)                      # no session at all
        self.assertEqual(self.settle("7-1", sid="S1").returncode, 0)               # no record
        self.put(9, colour=None, last_colour=C[0])                                 # record, no owner
        self.assertEqual(self.settle("9-1", sid="S1").returncode, 0)
        self.put_test("8-2", status="retour")
        self.assertEqual(self.settle("8-2", sid="OTHER").returncode, 0)            # the live owner itself

    def test_settle_does_not_change_the_counts_or_the_colour(self):
        self.claim(7, "--name", "Seven")
        self.put_test("7-1", status="retour", **{"try": "scripts/try.sh 7"})
        self.put_test("7-2", status="todo", **{"try": "scripts/try.sh 7"})
        before = (suivi_state_counts(self.state), self.held())
        self.assertEqual(self.settle("7-1", sid="S1").returncode, 0)
        self.assertEqual((suivi_state_counts(self.state), self.held()), before)

    def test_merge_ready_counts_a_settled_retour_but_not_an_unsettled_one(self):
        def items(*es):
            shutil.rmtree(os.path.join(self.state, "tests"), ignore_errors=True)
            for k, f in enumerate(es, 1):
                self.put_test("7-%d" % k, **dict({"try": "x"}, **f))
            with mock.patch.dict(os.environ, {"SF_SUIVI_DIR": self.state}):
                return suivi.scan_group([7])[0]
        st = {"at": "t", "by": "agent"}
        self.assertTrue(suivi.merge_ready(items({"status": "ok"}, {"status": "retour", "settled": st})))
        self.assertFalse(suivi.merge_ready(items({"status": "ok"}, {"status": "retour"})))
        self.assertFalse(suivi.merge_ready(items({"status": "retour", "settled": st}, {"status": "todo"})))
        a = items({"status": "retour"})
        b = {k: dict(v, settled=True) for k, v in a.items()}          # same file times: only `settled` differs
        self.assertNotEqual(suivi.merge_key(a), suivi.merge_key(b))

    def test_check_lists_an_unsettled_retour_of_a_done_issue_or_without_record(self):
        self.put(5, colour=None, done_at="2026-11-01")
        self.put(6, colour=C[0], owner=self.live_owner())
        self.put_test("5-1", status="retour")
        self.put_test("5-2", status="retour", settled={"at": "t", "by": "agent"})
        self.put_test("8-1", status="retour")                    # no record
        self.put_test("6-1", status="retour")                    # a live issue: its own business
        out = self.run_cli("check").stdout
        self.assertIn("5-1", out)
        self.assertIn("8-1", out)
        self.assertNotIn("5-2", out)
        self.assertNotIn("6-1", out)

    def test_unsettled_lists_and_exits_1_only_while_one_remains(self):
        self.put_test("7-1", status="retour")
        self.put_test("7-2", status="ok")
        r = self.run_cli("test", "unsettled", "7")
        self.assertEqual((r.returncode, r.stdout.split()), (1, ["7-1"]))
        self.settle("7-1")
        self.assertEqual(self.run_cli("test", "unsettled", "7").returncode, 0)


class TestAgentIdle(Base):
    def setUp(self):
        super().setUp()
        self.proj = os.path.join(self.tmp.name, "projects")
        self.env["SF_CLAUDE_PROJECTS"] = self.proj

    def touch(self, rel, age_min):
        p = os.path.join(self.proj, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p, "w").close()
        t = time.time() - age_min * 60
        os.utime(p, (t, t))

    def idle(self):
        return self.status()["colours"][0].get("agent_idle_min")

    def test_idle_minutes_come_from_the_newest_transcript_stat(self):
        self.put(1, colour=C[0], owner=self.live_owner("S1"))
        self.assertIsNone(self.idle())                                         # no file found
        self.touch("-p/S1.jsonl", 30)
        self.touch("-q/S2.jsonl", 0)                                           # another session
        self.assertEqual(self.idle(), 30)
        self.touch("-p/S1/subagents/a.jsonl", 7)                               # a newer subagent file wins
        self.assertEqual(self.idle(), 7)
        self.touch("-p/S1.jsonl", 0)
        self.assertEqual(self.idle(), 0)

    def test_dead_owner_has_no_idle_time(self):
        self.touch("-p/S1.jsonl", 3)
        child = subprocess.Popen(["sleep", "30"])
        start = suivi.pid_start(child.pid)
        child.kill()
        child.wait()
        self.put(1, colour=C[0], owner={"session": "S1", "pid": child.pid, "pid_start": start, "at": time.time()})
        self.assertIsNone(self.status()["paused"][0]["agent_idle_min"])      # paused: the owner is gone


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
    EXTRA_ENV = {}

    def setUp(self):
        super().setUp()
        self.claim(1, "--name", "One")
        self.run_cli("set", "1", "--state", "code", "--what", "doing", "--wait", "you", "--waiting", "your OK", "--force")
        self.claim(2, "--name", "Two")
        os.makedirs(os.path.join(self.state, "tests"))
        self.entry = os.path.join(self.state, "tests", "1-1.json")
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": "todo", "try": "scripts/try.sh 1"}, f)
        self.proc = subprocess.Popen([sys.executable, "-I", SUIVI, "serve", "--port", "0", "--state", self.state],
                                     cwd=self.repo, env=dict(self.env, **self.EXTRA_ENV), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
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

class TestBuildMode(ServerBase):
    """The build mode control (#476): state key and POST route."""

    def state_json(self):
        st, _, body = self.req("GET", "/api/state")
        self.assertEqual(st, 200)
        return json.loads(body)

    def install(self):
        shutil.copy(os.path.join(HERE, "cargo-queue.py"), self.env["SF_CARGO_QUEUE_INSTALLED"])

    def mode(self, body, **kw):
        return self.post("/api/build-mode", body=body, **kw)

    def test_not_installed_reads_as_such_and_refuses_writes(self):
        self.assertEqual(self.state_json()["build_queue"], {"installed": False})
        self.assertEqual(self.mode({"mode": "full"})[0], 409)
        self.assertFalse(os.path.exists(os.path.join(self.cqdir, "mode.json")))

    def test_installed_state_carries_modes_and_selection(self):
        self.install()
        bq = self.state_json()["build_queue"]
        self.assertTrue(bq["installed"] and not bq["stale"])
        self.assertEqual((bq["selected"], bq["effective"], bq["auto"]), ("dev", "dev", False))
        self.assertEqual(set(bq["modes"]), {"full", "dev", "background"})
        self.assertEqual((bq["running"], bq["waiting"]), ([], []))

    def test_stale_copy_is_flagged(self):
        self.install()
        with open(self.env["SF_CARGO_QUEUE_INSTALLED"], "a") as f:
            f.write("# old\n")
        self.assertTrue(self.state_json()["build_queue"]["stale"])

    def test_post_selects_a_mode_and_applies_it(self):
        self.install()
        st, _, body = self.mode({"mode": "background"})
        self.assertEqual(st, 200, body)
        bq = self.state_json()["build_queue"]
        self.assertEqual((bq["selected"], bq["effective"]), ("background", "background"))
        self.assertIn("set-property --runtime sfq-test.slice", slurp(os.path.join(self.tmp.name, "ctl.log")))
        st, _, _ = self.mode({"auto": True})
        self.assertEqual(st, 200)
        self.assertTrue(self.state_json()["build_queue"]["auto"])

    def test_post_validation_and_guard(self):
        self.install()
        self.assertEqual(self.mode({"mode": "turbo"})[0], 400)
        self.assertEqual(self.mode({"auto": "yes"})[0], 400)
        self.assertEqual(self.mode({})[0], 400)
        self.assertEqual(self.mode({"mode": "full"}, headers={"Content-Type": "application/json"})[0], 403)
        self.assertEqual(self.mode({"mode": "full"}, host="evil.example:80")[0], 403)
        self.assertEqual(self.state_json()["build_queue"]["selected"], "dev")

    def test_state_does_not_change_between_polls(self):
        self.install()
        a = self.req("GET", "/api/state")
        b = self.req("GET", "/api/state")
        self.assertEqual(a[1]["ETag"], b[1]["ETag"])

    def test_corrupt_profile_record_never_breaks_the_state(self):
        self.install()
        os.makedirs(self.cqdir)
        with open(os.path.join(self.cqdir, "profile.json"), "w") as f:
            f.write("{not json")
        self.assertEqual(self.state_json()["build_queue"]["profile_note"], "")

    def test_profile_note_reaches_the_page_state(self):
        self.install()
        os.makedirs(self.cqdir)
        with open(os.path.join(self.cqdir, "profile.json"), "w") as f:
            json.dump({"wrote": False, "note": "write-failed"}, f)
        self.assertEqual(self.state_json()["build_queue"]["profile_note"], "write-failed")
        self.assertEqual(self.state_json()["build_queue"]["modes"]["full"]["profile"], "performance")
        self.assertIsNone(self.state_json()["build_queue"]["modes"]["dev"]["profile"])

    def test_post_full_never_reaches_the_real_profile_here(self):
        self.install()
        self.assertEqual(self.mode({"mode": "full"})[0], 200)
        self.assertEqual(self.state_json()["build_queue"]["effective"], "full")

    def test_broken_queue_never_breaks_the_state(self):
        self.install()
        os.makedirs(self.cqdir)
        with open(os.path.join(self.cqdir, "mode.json"), "w") as f:
            f.write("{not json")
        bq = self.state_json()["build_queue"]
        self.assertEqual(bq["selected"], "dev")


class WaitBase(Base):
    """`suivi.py wait` (#454): background processes polling every 0.05 s, `--timeout` as the guard."""

    def setUp(self):
        super().setUp()
        self.env["SF_SUIVI_WAIT_POLL"] = "0.05"
        self.env["SF_SUIVI_WAIT_SETTLE"] = "0"
        self.procs = []
        self.addCleanup(self.reap)

    def reap(self):
        for p in self.procs:
            if p.poll() is None:
                p.kill()
            p.communicate()

    def owned(self, n, sid="S1", **fields):
        self.put(n, colour=C[0], name="N%d" % n, owner=self.live_owner(sid), **fields)

    def entry(self, tid, status="todo", comment="", **extra):
        os.makedirs(os.path.join(self.state, "tests"), exist_ok=True)
        t = {"title": "T", "status": status, "try": "scripts/try.sh"}
        if comment:
            t["comment"] = comment
        t.update(extra)
        for k in [k for k, v in t.items() if v is None]:
            del t[k]
        with open(os.path.join(self.state, "tests", tid + ".json"), "w") as f:
            json.dump(t, f)

    def spawn(self, n, *extra, sid="S1", env=None, cwd=None):
        e = dict(self.env)
        e.update(self.as_session(sid))
        e.update(env or {})
        p = subprocess.Popen([sys.executable, "-I", SUIVI, "wait", str(n), *extra], cwd=cwd or self.repo, env=e,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.procs.append(p)
        return p

    def finish(self, p, t=10):
        out, err = p.communicate(timeout=t)
        return p.returncode, out, err

    def run_wait(self, n, *extra, sid="S1", env=None, timeout="2"):
        return self.finish(self.spawn(n, "--timeout", timeout, *extra, sid=sid, env=env))

    def until(self, cond, t=5):
        end = time.time() + t
        while time.time() < end:
            if cond():
                return True
            time.sleep(0.02)
        self.fail("condition not met")

    def reg_path(self, root=1):
        return os.path.join(self.state, "waits", "%d.json" % root)

    def armed(self, root=1):
        self.until(lambda: os.path.exists(self.reg_path(root)))
        time.sleep(0.15)       # past the first tick


class TestWaitEvents(WaitBase):
    def test_answered_before_start_returns_at_once(self):
        self.owned(1)
        self.entry("1-1", "retour", "x")
        self.entry("1-2")
        t0 = time.time()
        rc, out, _ = self.run_wait(1, timeout="30")
        self.assertEqual(rc, 0)
        self.assertIn("1-1 retour: x", out)
        self.assertLess(time.time() - t0, 5)
        self.assertIn("Still open: 1 trial entry to tick. Re-arm: python3 scripts/suivi.py wait 1", out)

    def test_settling_a_retour_wakes_nothing(self):
        self.owned(1)
        self.entry("1-1", "retour", "x")
        self.entry("1-2")
        self.run_wait(1)                                              # hears the retour
        p = self.spawn(1, "--timeout", "2")
        self.armed()
        r = self.run_cli("test", "settle", "1-1", "--note", "done", env=self.as_session("S1"))
        self.assertEqual(r.returncode, 0, r.stderr)
        rc, out, _ = self.finish(p)
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("timeout"), out)
        self.assertNotIn("1-1", out)

    def test_a_settle_by_the_user_wakes_the_wait_like_an_answer(self):
        self.owned(1)
        self.entry("1-1", "retour", "x")
        self.entry("1-2")
        self.run_wait(1)                                              # hears the retour
        p = self.spawn(1, "--timeout", "5")
        self.armed()
        self.entry("1-1", "retour", "x", settled={"at": "t", "by": "user"})
        rc, out, _ = self.finish(p)
        self.assertEqual(rc, 0)
        self.assertIn("1-1 retour: x (settled by the user on the page)", out)

    def test_rearmed_reports_nothing_again(self):
        self.owned(1)
        self.entry("1-1", "retour", "bad")
        self.entry("1-2")
        rc, out, _ = self.run_wait(1)
        self.assertIn("1-1 retour: bad", out)
        rc, out, _ = self.run_wait(1, timeout="1")
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("timeout"), out)
        self.assertNotIn("1-1", out)

    def test_new_session_hears_everything_again(self):
        self.owned(1)
        self.entry("1-1", "retour", "x")
        self.entry("1-2")
        self.assertIn("1-1 retour", self.run_wait(1)[1])
        self.owned(1, sid="S2")
        self.assertIn("1-1 retour", self.run_wait(1, sid="S2")[1])

    def test_kill_before_printing_keeps_the_event_for_the_next_wait(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        self.entry("1-1", "retour", "x")
        time.sleep(0.4)           # seen, held by the delay
        p.kill()
        p.communicate()
        self.assertFalse(os.path.exists(os.path.join(self.state, "heard", "1.json")))
        self.assertIn("1-1 retour", self.run_wait(1, timeout="3")[1])

    def test_settle_groups_events_then_wakes(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "0.6"})
        self.armed()
        self.entry("1-1", "retour", "x")
        time.sleep(0.3)
        self.assertIsNone(p.poll())
        rc, out, _ = self.finish(p)
        self.assertIn("1-1 retour", out)

    def test_an_ok_with_an_idea_wakes_after_the_delay_and_carries_the_idea(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        self.entry("1-1", "ok", idea="add a zoom")
        t0 = time.time()
        rc, out, _ = self.run_wait(1, timeout="5", env={"SF_SUIVI_WAIT_SETTLE": "1.0"})
        self.assertIn("1-1 ok + idea: add a zoom", out)
        self.assertGreaterEqual(time.time() - t0, 0.9)

    def test_plain_ok_with_entries_left_wakes_nothing(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        self.entry("1-1", "ok")
        rc, out, _ = self.run_wait(1, timeout="1")
        self.assertTrue(out.startswith("timeout"), out)

    def test_an_image_alone_wakes_like_a_comment_and_names_its_path(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        name = "0123456789abcdef01234567.png"
        d = os.path.join(self.state, "tests", "1-1.images")
        os.makedirs(d)
        with open(os.path.join(d, name), "wb") as f:
            f.write(png())
        self.entry("1-1", images=[name])
        rc, out, _ = self.run_wait(1, timeout="5")
        self.assertEqual(rc, 0)
        self.assertEqual(out.splitlines()[0], "1-1 todo")
        self.assertEqual(out.splitlines()[1], "  image: " + os.path.join(d, name))
        self.assertTrue(os.path.isabs(out.splitlines()[1][len("  image: "):]))
        self.assertIn("Still open: 2 trial entries", out)

    def test_an_image_waits_for_the_settle_delay_like_a_comment(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        self.entry("1-1", images=["0123456789abcdef01234567.png"])
        t0 = time.time()
        rc, out, _ = self.run_wait(1, timeout="5", env={"SF_SUIVI_WAIT_SETTLE": "1.0"})
        self.assertIn("1-1 todo", out)
        self.assertGreaterEqual(time.time() - t0, 0.9)

    def test_the_fingerprint_is_unchanged_without_images(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2", "retour", "bad")
        with mock.patch.dict(os.environ, {"SF_SUIVI_DIR": self.state}):
            items, _, todos = suivi.scan_group([1])
        self.assertEqual(items["1-1"]["fp"], suivi.BASE_FP)            # stored `heard/` fingerprints stay valid
        self.assertEqual(items["1-2"]["fp"], "retour|bad")
        self.entry("1-1", images=["0123456789abcdef01234567.png"])
        with mock.patch.dict(os.environ, {"SF_SUIVI_DIR": self.state}):
            items, _, _ = suivi.scan_group([1])
        self.assertNotEqual(items["1-1"]["fp"], suivi.BASE_FP)

    def test_each_change_restarts_the_delay(self):
        self.owned(1)
        for k in (1, 2, 3):
            self.entry("1-%d" % k)
        p = self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "1.0"})
        self.armed()
        self.entry("1-1", "retour", "x")
        time.sleep(0.6)
        self.entry("1-3", "ok")                     # a silent tick restarts the delay
        t1 = time.time()
        time.sleep(0.6)
        self.assertIsNone(p.poll())
        rc, out, _ = self.finish(p)
        self.assertGreaterEqual(time.time() - t1, 0.9)
        self.assertIn("1-1 retour: x", out)
        self.assertIn("1-3 ok", out)

    def test_last_tick_wakes_at_once_and_reports_the_silent_oks(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        self.entry("1-1", "ok")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        self.assertIsNone(p.poll())
        self.entry("1-2", "ok")
        rc, out, _ = self.finish(p, 5)
        self.assertIn("1-1 ok", out)
        self.assertIn("1-2 ok", out)

    def test_finished_form_wakes_at_once_despite_the_delay(self):
        self.owned(1)
        self.entry("1-1")
        d = os.path.join(self.state, "qcm", "1-1")
        os.makedirs(os.path.join(d, "answers"))
        meta = {"issue": 1, "title": "F", "cards": 1, "finished": False}
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump(meta, f)
        p = self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        meta["finished"] = True
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump(meta, f)
        self.assertIn("form 1-1 finished", self.finish(p, 5)[1])

    def test_misclick_unticked_before_the_delay_wakes_nothing(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        p = self.spawn(1, "--timeout", "2.5", env={"SF_SUIVI_WAIT_SETTLE": "1.0"})
        self.armed()
        self.entry("1-1", "retour")
        time.sleep(0.3)
        self.entry("1-1", "todo")
        self.assertTrue(self.finish(p)[1].startswith("timeout"))

    def test_prefix_and_try_filter(self):
        self.owned(454)
        self.entry("45-1", "ok")
        self.entry("454-2", "ok", **{"try": None})
        self.entry("454-9")
        self.entry("454-8", "retour", "x", **{"try": None})
        rc, out, _ = self.run_wait(454, timeout="1")
        self.assertTrue(out.startswith("timeout"), out)

    def test_unticking_a_reported_ok_is_an_event_an_unreported_one_is_not(self):
        self.owned(1)
        self.entry("1-1", "ok")
        self.entry("1-2", "ok")
        self.assertIn("1-1 ok", self.run_wait(1)[1])      # no todo left: wakes and records
        self.entry("1-1", "todo")
        self.entry("1-3", "ok")
        self.entry("1-3", "todo")                          # never reported: nothing to say
        news = self.run_cli("news", "1", env=self.as_session()).stdout
        self.assertIn("1-1 todo", news)
        self.assertNotIn("1-3", news)

    def test_form_event_only_once_finished_and_again_when_an_answer_changes(self):
        self.owned(1)
        self.entry("1-9")
        d = os.path.join(self.state, "qcm", "1-1")
        os.makedirs(os.path.join(d, "answers"))
        meta = {"issue": 1, "title": "F", "cards": 2, "finished": False}
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump(meta, f)
        with open(os.path.join(d, "answers", "1.json"), "w") as f:
            json.dump({"code": "1", "choix": "a"}, f)
        self.assertTrue(self.run_wait(1, timeout="1")[1].startswith("timeout"))
        meta["finished"] = True
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump(meta, f)
        out = self.run_wait(1)[1]
        self.assertIn("form 1-1 finished, 1/2 answered, read with: suivi.py qcm answers 1-1", out)
        self.assertTrue(self.run_wait(1, timeout="1")[1].startswith("timeout"))
        with open(os.path.join(d, "answers", "1.json"), "w") as f:
            json.dump({"code": "1", "choix": "b"}, f)
        self.assertIn("form 1-1 finished", self.run_wait(1)[1])

    def test_nothing_open_returns_at_once(self):
        self.owned(1)
        t0 = time.time()
        rc, out, _ = self.run_wait(1, timeout="30")
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("nothing open"), out)
        self.assertIn("end the turn without a message", out)
        self.assertLess(time.time() - t0, 5)

    def test_package_watches_all_members_and_shares_one_registration(self):
        self.owned(10, kind="package", subs=[{"n": 11, "title": "s"}])
        self.owned(11, parent=10)
        self.entry("11-1")
        a = self.spawn(11)
        self.armed(10)
        self.assertFalse(os.path.exists(self.reg_path(11)))
        rc, out, _ = self.finish(self.spawn(10))
        self.assertIn("already listening", out)
        self.assertEqual(rc, 0)
        self.entry("11-1", "ok")
        self.assertIn("11-1 ok", self.finish(a)[1])

    def test_parent_wait_sees_a_sub_issue_entry(self):
        self.owned(10, kind="package", subs=[{"n": 11, "title": "s"}])
        self.owned(11, parent=10)
        self.entry("11-1", "retour", "x")
        self.entry("11-2")
        self.assertIn("11-1 retour", self.run_wait(10)[1])

    def test_no_record_and_not_the_owner_exit_one(self):
        self.assertEqual(self.run_wait(5)[0], 1)
        self.owned(1, sid="S2")
        self.entry("1-1")
        rc, _, err = self.run_wait(1)
        self.assertEqual(rc, 1)
        self.assertIn("does not own", err)


class TestWaitWithdrawn(WaitBase):
    """#479: a page withdrawal wakes the waiting agent once; a CLI withdrawal wakes nothing."""

    def archived(self, qid, by, answers=0, ts=None):
        d = os.path.join(self.state, "removed-qcm", qid)
        os.makedirs(os.path.join(d, "answers"))
        for k in range(answers):
            with open(os.path.join(d, "answers", "R%d.json" % (k + 1)), "w") as f:
                json.dump({"code": "R%d" % (k + 1), "statut": "tranche", "question": "Q"}, f)
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump({"issue": int(qid.split("-")[0]), "title": "F", "cards": 2, "finished": False, "withdrawn_by": by,
                       "withdrawn_ts": time.time() if ts is None else ts}, f)

    def test_page_withdrawal_wakes_the_wait_once_naming_the_form(self):
        self.owned(1)
        self.entry("1-1")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        self.archived("1-1", "page", answers=2)
        rc, out, _ = self.finish(p, 5)
        self.assertEqual(rc, 0)
        self.assertIn("form 1-1 withdrawn on the page by the user, 2 answers recorded, read with: suivi.py qcm answers 1-1", out)
        rc, out, _ = self.run_wait(1, timeout="1")                 # reported once
        self.assertTrue(out.startswith("timeout"), out)

    def test_page_withdrawal_without_answers_says_so(self):
        self.owned(1)
        self.entry("1-1")
        self.archived("1-1", "page")
        self.assertIn("1-1 withdrawn on the page by the user, no answer recorded", self.run_wait(1)[1])

    def test_cli_withdrawal_wakes_nothing_and_nothing_left_ends_the_wait(self):
        self.owned(1)
        self.entry("1-1")
        self.archived("1-1", "cli")
        rc, out, _ = self.run_wait(1, timeout="1")
        self.assertTrue(out.startswith("timeout"), out)             # a todo is left: still armed, no line
        os.unlink(os.path.join(self.state, "tests", "1-1.json"))
        self.assertTrue(self.run_wait(1)[1].startswith("nothing open"))

    def test_a_withdrawal_older_than_the_claim_is_not_news(self):
        self.owned(1)
        self.entry("1-1")
        self.archived("1-1", "page", ts=time.time() - 100)
        self.assertTrue(self.run_wait(1, timeout="1")[1].startswith("timeout"))

    def test_a_withdrawal_is_never_an_unread_answer(self):
        self.put(1, colour=None, last_colour=C[0], name="N1")      # paused: nobody owns it
        self.archived("1-1", "page", answers=1)
        self.assertEqual(self.status()["paused"][0]["unread"], 0)
        self.assertEqual(self.hook("S9").stdout, "")

    def hook(self, sid):
        return self.run_cli("hook-news", env={"CLAUDE_PROJECT_DIR": self.repo}, stdin=json.dumps({"session_id": sid}))


class TestWaitExits(WaitBase):
    def test_dead_pid_is_session_gone(self):
        child = subprocess.Popen(["sleep", "30"])
        start = suivi.pid_start(child.pid)
        child.kill()
        child.wait()
        self.put(1, colour=C[0], owner={"session": "S1", "pid": child.pid, "pid_start": start, "at": time.time()})
        self.entry("1-1")
        rc, out, _ = self.run_wait(1)
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("session gone"), out)
        self.assertIn("end the turn without a message", out)

    def test_newer_ended_marker_is_session_gone(self):
        self.put(1, colour=C[0], owner=self.live_owner("S1", at=time.time() - 100))
        self.entry("1-1")
        os.makedirs(os.path.join(self.state, "ended"))
        open(os.path.join(self.state, "ended", "S1"), "w").close()
        self.assertTrue(self.run_wait(1)[1].startswith("session gone"))

    def test_pause_ends_the_wait_and_the_wait_never_held_the_colour(self):
        self.owned(1)
        self.entry("1-1")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_POLL": "2"})
        self.armed()
        os.unlink(os.path.join(self.state, "tests", "1-1.json"))     # nothing holds the colour now
        self.assertEqual(self.run_cli("pause", "1").returncode, 0)
        self.assertEqual(self.held(), [])          # colour free although the wait is alive
        self.assertIsNone(p.poll())
        self.assertTrue(self.finish(p)[1].startswith("paused"))

    def test_done_ends_the_wait(self):
        self.owned(1)
        self.entry("1-1")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_POLL": "1"})
        self.armed()
        self.run_cli("done", "1")
        self.assertTrue(self.finish(p)[1].startswith("done"))

    def test_other_claim_ends_the_wait_as_taken(self):
        self.owned(1)
        self.entry("1-1")
        p = self.spawn(1, env={"SF_SUIVI_WAIT_POLL": "1"})
        self.armed()
        self.owned(1, sid="S2")
        self.assertTrue(self.finish(p)[1].startswith("taken"))

    def test_second_wait_same_session_is_already_listening(self):
        self.owned(1)
        self.entry("1-1")
        a = self.spawn(1, env={"SF_SUIVI_WAIT_POLL": "5"})
        self.armed()
        rc, out, _ = self.finish(self.spawn(1))
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("already listening"), out)
        self.assertIsNone(a.poll())

    def test_second_wait_other_session_exits_one(self):
        self.owned(1)
        self.entry("1-1")
        a = self.spawn(1, env={"SF_SUIVI_WAIT_POLL": "5"})
        self.armed()
        self.owned(1, sid="S2")
        rc, _, err = self.finish(self.spawn(1, sid="S2"))
        self.assertEqual(rc, 1)
        self.assertIn("another session", err)
        self.assertIsNone(a.poll())

    def test_no_git_after_start_and_survives_a_removed_worktree(self):
        real = shutil.which("git")
        bindir = os.path.join(self.tmp.name, "bin")
        os.makedirs(bindir)
        log = os.path.join(self.tmp.name, "git.log")
        with open(os.path.join(bindir, "git"), "w") as f:
            f.write('#!/bin/sh\necho "$@" >> "%s"\nexec "%s" "$@"\n' % (log, real))
        os.chmod(os.path.join(bindir, "git"), 0o755)
        self.worktree("feature1")
        wt = os.path.join(self.root, "feature1")
        self.owned(1)
        self.entry("1-1")
        p = self.spawn(1, env={"PATH": bindir + os.pathsep + os.environ["PATH"]}, cwd=wt)
        self.armed()
        before = slurp(log) if os.path.exists(log) else ""
        self.git("worktree", "remove", "--force", wt)
        time.sleep(0.6)
        after = slurp(log) if os.path.exists(log) else ""
        self.assertEqual(before, after)
        self.assertIsNone(p.poll())

    def test_sigterm_is_logged_and_unregisters(self):
        self.owned(1)
        self.entry("1-1")
        p = self.spawn(1)
        self.armed()
        p.terminate()
        self.finish(p)
        self.assertIn("SIGTERM", slurp(os.path.join(self.state, "waits", "log")))
        self.assertFalse(os.path.exists(self.reg_path()))

    def test_status_listening_and_unread(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2", "ok")
        e = self.status()["colours"][0]
        self.assertFalse(e["listening"])
        self.assertEqual(e["unread"], 1)
        self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        self.assertTrue(self.status()["colours"][0]["listening"])
        reg = jload(self.reg_path())
        reg["pid_start"] += 1
        with open(self.reg_path(), "w") as f:
            json.dump(reg, f)
        self.assertFalse(self.status()["colours"][0]["listening"])

    def test_hint_when_nobody_listens(self):
        self.owned(1)
        with open(os.path.join(self.tmp.name, "e.json"), "w") as f:
            json.dump({"title": "T", "try": "x", "goal": "g", "around": "a"}, f)
        r = self.run_cli("test", "add", "1", os.path.join(self.tmp.name, "e.json"))
        self.assertEqual(r.stdout.strip(), "1-1")
        self.assertIn("nobody is listening for #1", r.stderr)
        self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        r = self.run_cli("test", "add", "1", os.path.join(self.tmp.name, "e.json"), env=self.as_session())
        self.assertNotIn("nobody is listening", r.stderr)

    def test_news_prints_and_records(self):
        self.owned(1)
        self.entry("1-1", "retour", "c")
        r = self.run_cli("news", "1", env=self.as_session())
        self.assertIn("1-1 retour: c", r.stdout)
        self.assertIn("no news", self.run_cli("news", "1", env=self.as_session()).stdout)


class TestHookNews(WaitBase):
    def hook(self, sid, **env):
        e = {"CLAUDE_PROJECT_DIR": self.repo}
        e.update(env)
        return self.run_cli("hook-news", env=e, stdin=json.dumps({"session_id": sid}))

    def test_bad_json_or_no_state_is_silent_exit_zero(self):
        for stdin in ("", "not json", "[]", '{"x": 1}'):
            r = self.run_cli("hook-news", env={"CLAUDE_PROJECT_DIR": self.repo}, stdin=stdin)
            self.assertEqual((r.returncode, r.stdout, r.stderr), (0, "", ""), stdin)
        r = self.hook("S1")                                   # no state folder at all
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, "", ""))
        r = self.run_cli("hook-news", env={}, stdin='{"session_id": "S1"}')
        self.assertEqual((r.returncode, r.stdout), (0, ""))

    def test_owner_hears_each_event_once_and_is_told_to_arm_the_wait(self):
        self.owned(1)
        self.entry("1-1", "retour", "dark")
        self.entry("1-2")
        r = self.hook("S1")
        self.assertEqual(r.returncode, 0)
        self.assertIn("1-1 retour: dark", r.stdout)
        self.assertIn("no wait running", r.stdout)
        r = self.hook("S1")
        self.assertNotIn("1-1 retour", r.stdout)
        self.assertIn("no wait running", r.stdout)
        self.spawn(1, env={"SF_SUIVI_WAIT_SETTLE": "60"})
        self.armed()
        self.assertNotIn("no wait running", self.hook("S1").stdout)

    def test_new_session_lists_dead_owners_with_unread_answers_and_records_nothing(self):
        child = subprocess.Popen(["sleep", "30"])
        start = suivi.pid_start(child.pid)
        child.kill()
        child.wait()
        self.put(1, colour=C[0], owner={"session": "S0", "pid": child.pid, "pid_start": start, "at": time.time()})
        self.entry("1-1", "retour", "x")
        self.owned(2, sid="S3")                              # a live owner stays silent
        self.entry("2-1", "retour", "y")
        r = self.hook("S9")
        self.assertEqual(r.returncode, 0)
        self.assertIn("#1", r.stdout)
        self.assertNotIn("#2", r.stdout)
        self.assertFalse(os.path.exists(os.path.join(self.state, "heard")))
        self.assertEqual(self.hook("S9").stdout, r.stdout)

    def test_another_live_sessions_issue_is_silent(self):
        self.owned(1, sid="S3")
        self.entry("1-1", "retour", "x")
        self.assertEqual(self.hook("S9").stdout, "")

    def test_one_run_is_fast_and_forks_no_git(self):
        self.owned(1)
        self.entry("1-1", "retour", "x")
        real = shutil.which("git")
        bindir = os.path.join(self.tmp.name, "bin")
        os.makedirs(bindir)
        log = os.path.join(self.tmp.name, "git.log")
        with open(os.path.join(bindir, "git"), "w") as f:
            f.write('#!/bin/sh\necho "$@" >> "%s"\nexec "%s" "$@"\n' % (log, real))
        os.chmod(os.path.join(bindir, "git"), 0o755)
        t0 = time.time()
        r = self.hook("S1", PATH=bindir + os.pathsep + os.environ["PATH"])
        dt = time.time() - t0
        self.assertIn("1-1 retour", r.stdout)
        self.assertFalse(os.path.exists(log))
        self.assertLess(dt, 1.0)
        sys.stderr.write("hook-news one run: %.0f ms\n" % (dt * 1000))


class TestHookWait(WaitBase):
    def hook(self, sid, command, stdin=None, env=None):
        e = dict(self.env)
        e.update({"CLAUDE_PROJECT_DIR": self.repo})
        e.update(env or {})
        p = subprocess.Popen([sys.executable, "-I", SUIVI, "hook-wait"], cwd=self.repo, env=e, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.procs.append(p)
        p.stdin.write(json.dumps({"session_id": sid, "tool_input": {"command": command}}) if stdin is None else stdin)
        p.stdin.close()
        p.stdin = None  # communicate() flushes a stdin it still sees; Python 3.12 raises on a closed one
        return p

    def test_a_pose_command_arms_the_wait_and_an_event_wakes_with_exit_two_on_stderr(self):
        self.owned(1)
        self.entry("1-1")
        self.entry("1-2")
        p = self.hook("S1", "python3 scripts/suivi.py qcm pose 1 d.js --title x")
        self.armed()
        self.entry("1-1", "retour", "too dark")
        rc, out, err = self.finish(p)
        self.assertEqual(rc, 2)
        self.assertEqual(out, "")
        self.assertIn("1-1 retour: too dark", err)
        self.assertIn("resumes by itself at your next suivi.py call on #1", err.splitlines()[-1])
        self.assertFalse(os.path.exists(self.reg_path()))

    def test_unrelated_commands_exit_zero_at_once_and_register_nothing(self):
        self.owned(1)
        self.entry("1-1")
        for cmd in ("ls -la", "python3 scripts/suivi.py status", "python3 scripts/suivi.py qcm list"):
            rc, out, err = self.finish(self.hook("S1", cmd), 5)
            self.assertEqual((rc, out, err), (0, "", ""), cmd)
        self.assertFalse(os.path.exists(self.reg_path()))

    def test_ownership_comes_from_stdin_not_the_environment(self):
        self.owned(1, sid="S1")
        self.entry("1-1")
        p = self.hook("S1", "python3 scripts/suivi.py news 1", env={"CLAUDE_CODE_SESSION_ID": "OTHER"})
        self.armed()
        self.assertIsNone(p.poll())
        os.unlink(os.path.join(self.state, "tests", "1-1.json"))      # let it end: nothing open
        self.assertEqual(self.finish(p)[0], 0)

    def test_another_sessions_issue_exits_zero_at_once(self):
        self.owned(1, sid="S2")
        self.entry("1-1")
        self.assertEqual(self.finish(self.hook("S1", "python3 scripts/suivi.py news 1"), 5), (0, "", ""))
        self.assertFalse(os.path.exists(self.reg_path()))

    def test_second_hook_while_listening_exits_zero_and_the_first_stays(self):
        self.owned(1)
        self.entry("1-1")
        a = self.hook("S1", "python3 scripts/suivi.py news 1")
        self.armed()
        self.assertEqual(self.finish(self.hook("S1", "python3 scripts/suivi.py set 1 --what x"), 5), (0, "", ""))
        self.assertIsNone(a.poll())

    def test_nothing_open_exits_zero_at_once(self):
        self.owned(1)
        self.assertEqual(self.finish(self.hook("S1", "python3 scripts/suivi.py news 1"), 5), (0, "", ""))

    def test_bad_input_exits_zero_silent(self):
        for stdin in ("", "not json", "[]", '{"session_id": "S1"}', '{"session_id": "S1", "tool_input": 3}'):
            self.assertEqual(self.finish(self.hook("S1", "", stdin=stdin), 5), (0, "", ""), stdin)
        r = subprocess.run([sys.executable, "-I", SUIVI, "hook-wait", "--nope"], input="", capture_output=True,
                           text=True, env=self.env, cwd=self.repo)
        self.assertEqual((r.returncode, r.stdout), (0, ""))          # argparse must not reach exit 2

    def test_an_exception_inside_the_wait_exits_zero(self):
        self.owned(1)
        self.entry("1-1")
        with open(os.path.join(self.state, "waits"), "w") as f:       # a file where the folder should be
            f.write("x")
        self.assertEqual(self.finish(self.hook("S1", "python3 scripts/suivi.py news 1"), 5), (0, "", ""))

    def test_the_issue_is_parsed_from_the_command(self):
        n = suivi.hook_issue
        for cmd, want in (("cd x && python3 scripts/suivi.py test add 7 f.json", 7), ("python3 -I scripts/suivi.py qcm pose 12 d.js", 12),
                          ("suivi.py test remove 7 7-1", 7), ("suivi.py set 454 --state code", 454), ("suivi.py news 3", 3),
                          ("suivi.py test merged 7 abc", 7), ("suivi.py qcm answers 7-2", 7), ("suivi.py qcm withdraw 7-1", 7), ("suivi.py qcm withdraw 7-1 --answered", 7), ("suivi.py qcm pose 12 d.js --force", 12), ("suivi.py test list --issue 9", 9),
                          ("suivi.py status", None), ("suivi.py test list", None), ("suivi.py qcm list --issue 7", None),
                          ("echo 7", None), ("", None)):
            self.assertEqual(n(cmd), want, cmd)


class TestSettingsHooks(unittest.TestCase):
    """.claude/settings.json (#454): the hooks that report answers and arm the wait."""
    PATH = os.path.join(HERE, os.pardir, ".claude", "settings.json")

    def problems(self, d):
        out = []
        subs = set(next(a for a in suivi.parser()._actions if a.dest == "cmd").choices)
        hooks = d.get("hooks", {})
        for ev in ("SessionStart", "UserPromptSubmit", "PostToolUse", "SessionEnd"):
            for group in hooks.get(ev, []):
                for h in group.get("hooks", []):
                    m = re.search(r'scripts/suivi\.py"? (\S+)', h.get("command", ""))
                    if not m or m.group(1) not in subs:
                        out.append("%s: command names no subcommand of suivi.py: %r" % (ev, h.get("command")))
        def cmds(ev):
            return [h for g in hooks.get(ev, []) for h in g.get("hooks", [])]
        for ev in ("SessionStart", "UserPromptSubmit"):
            hs = cmds(ev)
            if not hs or not all(" hook-news" in h["command"] and h["command"].endswith("|| true") for h in hs):
                out.append(ev + ": must run hook-news and end with || true")
        post = hooks.get("PostToolUse", [])
        ok = any(g.get("matcher") == "Bash" and any(
            "suivi.py" in h.get("if", "") and h.get("asyncRewake") is True and h.get("timeout", 0) >= 259200
            and h["command"].rstrip().endswith("hook-wait") for h in g.get("hooks", [])) for g in post)
        if not ok:
            out.append("PostToolUse: needs matcher Bash, if naming suivi.py, asyncRewake, timeout >= 259200, hook-wait")
        if not any(" session-end" in h["command"] for h in cmds("SessionEnd")):
            out.append("SessionEnd: session-end hook is gone")
        return out

    def test_the_committed_hooks_are_as_pinned(self):
        with open(self.PATH, encoding="utf-8") as f:
            d = json.load(f)
        self.assertEqual(self.problems(d), [])

    def test_a_dropped_or_renamed_piece_is_caught(self):
        with open(self.PATH, encoding="utf-8") as f:
            text = f.read()
        for old, new in ((" || true", ""), ("hook-news", "hook-nws"), ("hook-wait", "hook-wiat"),
                         ('"asyncRewake": true', '"asyncRewake": false'), ("259200", "100"),
                         ('"matcher": "Bash"', '"matcher": "Edit"')):
            self.assertNotEqual(self.problems(json.loads(text.replace(old, new))), [], old)


class TestSettingsPermissions(unittest.TestCase):
    """.claude/settings.json (#516): hard bans and approvals by settings, not by prose."""
    PATH = TestSettingsHooks.PATH
    WANT = {"deny": ("Bash(git stash)", "Bash(git stash *)"),
            "ask": ("Bash(git tag *)", "Bash(git push * --tags*)", "Bash(git push origin v*)"),
            "allow": ("Bash(scripts/deliver.sh *)",)}

    def problems(self, d):
        perm = d.get("permissions", {})
        out = []
        for kind, rules in self.WANT.items():
            for rule in rules:
                if rule not in perm.get(kind, []):
                    out.append("permissions.%s lacks %s" % (kind, rule))
        for rule in perm.get("deny", []):
            if "git tag" in rule:
                out.append("git tag must ask, not be denied: " + rule)
        return out

    def test_the_committed_permissions_are_as_pinned(self):
        with open(self.PATH, encoding="utf-8") as f:
            d = json.load(f)
        self.assertEqual(self.problems(d), [])

    def test_a_dropped_or_renamed_rule_is_caught(self):
        with open(self.PATH, encoding="utf-8") as f:
            text = f.read()
        for old, new in (("Bash(git stash *)", "Bash(git stach *)"), ("Bash(git tag *)", "Bash(git tags *)"),
                         ("Bash(scripts/deliver.sh *)", "Bash(scripts/deliver.sh)"),
                         ('"ask"', '"deny"'), ('"deny"', '"ask"')):
            self.assertNotEqual(self.problems(json.loads(text.replace(old, new))), [], old)


class TestMergeRequest(WaitBase):
    """The page's « Fusionner » button (#454): a recorded request, never a merge."""

    def request(self, n=1, at="t1"):
        with mock.patch.dict(os.environ, {"SF_SUIVI_DIR": self.state}):
            items, _, _ = suivi.scan_group([n], n)
            key = suivi.merge_key(items)
        os.makedirs(os.path.join(self.state, "merge"), exist_ok=True)
        with open(os.path.join(self.state, "merge", "%d.json" % n), "w") as f:
            json.dump({"at": at, "key": key}, f)

    def all_ok(self):
        self.owned(1)
        self.entry("1-1", "ok")
        self.entry("1-2", "ok")
        self.assertIn("1-2 ok", self.run_wait(1)[1])          # the all-OK wake, recorded

    def test_the_wait_stays_armed_after_all_ok_and_wakes_once_on_the_click(self):
        self.all_ok()
        p = self.spawn(1)
        self.armed()
        time.sleep(0.3)
        self.assertIsNone(p.poll())                           # `nothing open` must not fire
        self.request()
        rc, out, _ = self.finish(p, 5)
        self.assertIn("#1 merge requested on the page by the user: this is the go-ahead", out)
        rc, out, _ = self.run_wait(1, timeout="1")             # same request: not again
        self.assertTrue(out.startswith("timeout"), out)

    def test_a_voided_request_then_a_new_click_wakes_again(self):
        self.all_ok()
        self.request(at="t1")
        self.assertIn("merge requested", self.run_wait(1)[1])
        self.entry("1-1", "todo")
        self.entry("1-1", "ok")                                # voided: the entries changed
        self.assertTrue(self.run_wait(1, timeout="1")[1].startswith("timeout"))
        self.request(at="t2")
        self.assertIn("merge requested", self.run_wait(1)[1])

    def test_delivered_merge_ends_the_wait_quietly(self):
        self.all_ok()
        p = self.spawn(1)
        self.armed()
        self.entry("1-1", "ok", **{"try": None})              # what `test merged` does to the entries
        self.entry("1-2", "ok", **{"try": None})
        self.assertTrue(self.finish(p, 5)[1].startswith("nothing open"))

    def test_hook_wait_arms_while_a_merge_is_ready(self):
        self.all_ok()
        e = dict(self.env)
        e["CLAUDE_PROJECT_DIR"] = self.repo
        p = subprocess.Popen([sys.executable, "-I", SUIVI, "hook-wait"], cwd=self.repo, env=e, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.procs.append(p)
        p.stdin.write(json.dumps({"session_id": "S1", "tool_input": {"command": "python3 scripts/suivi.py news 1"}}))
        p.stdin.close()
        p.stdin = None  # communicate() flushes a stdin it still sees; Python 3.12 raises on a closed one
        self.armed()
        self.assertIsNone(p.poll())
        self.request()
        rc, out, err = self.finish(p, 5)
        self.assertEqual((rc, out), (2, ""))
        self.assertIn("merge requested", err)

    def test_status_carries_the_flags_and_the_colour_rule_is_unchanged(self):
        self.owned(1)
        self.entry("1-1", "ok")
        self.assertEqual(self.held(), ["agent"])
        e = self.status()["colours"][0]
        self.assertEqual((e["merge_ready"], e["merge_requested"]), (True, False))
        self.request()
        e = self.status()["colours"][0]
        self.assertEqual((e["merge_ready"], e["merge_requested"]), (True, True))
        self.entry("1-1", "retour", "x")
        e = self.status()["colours"][0]
        self.assertEqual((e["merge_ready"], e["merge_requested"]), (False, False))

    def test_all_ok_without_an_agent_frees_the_colour(self):
        self.put(1, colour=C[0], name="N", owner=None)
        self.entry("1-1", "ok")
        self.request()
        self.assertEqual(self.held(), [])
        self.assertEqual([p["issue"] for p in self.status()["paused"]], [1])

    def test_test_merged_deletes_the_request(self):
        self.claim(1, "--name", "One")
        self.entry("1-1", "ok")
        self.request()
        self.git("checkout", "-q", "-b", "feature/1-x")
        self.git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "work (#1)")
        self.git("checkout", "-q", "main")
        self.git("-c", "user.name=t", "-c", "user.email=t@t", "merge", "-q", "--no-ff", "-m", "Merge #1", "feature/1-x")
        full = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo, capture_output=True, text=True).stdout.strip()
        self.assertTrue(os.path.exists(os.path.join(self.state, "merge", "1.json")))
        r = self.run_cli("test", "merged", "1", full)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.state, "merge", "1.json")))


class TestMergePost(ServerBase):
    def merge(self, n=1, **kw):
        return self.post("/api/issue/%d/merge" % n, body={}, **kw)

    def flags(self, n=0):
        e = json.loads(self.req("GET", "/api/state")[2])["colours"][n]
        return e["merge_ready"], e["merge_requested"]

    def test_refused_with_409_unless_every_try_entry_is_ok(self):
        self.assertEqual(self.merge()[0], 409)                      # 1-1 is todo
        for status in ("retour", "todo"):
            with open(self.entry, "w") as f:
                json.dump({"n": 1, "title": "T", "status": status, "try": "x"}, f)
            self.assertEqual(self.merge()[0], 409, status)
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": "ok"}, f)    # no `try`
        self.assertEqual(self.merge()[0], 409)
        self.assertEqual(self.merge(9)[0], 409)                     # no such issue
        self.assertFalse(os.path.exists(os.path.join(self.state, "merge")))

    def test_guards(self):
        self.ok()
        self.assertEqual(self.merge(headers={})[0], 403)
        self.assertEqual(self.merge(headers={"X-Suivi": "1", "Origin": "http://evil.example"})[0], 403)
        self.assertEqual(self.merge(host="evil.example")[0], 403)
        self.assertFalse(os.path.exists(os.path.join(self.state, "merge")))

    def ok(self, status="ok"):
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": status, "try": "x"}, f)

    def test_a_click_records_once_and_a_change_voids_it(self):
        self.ok()
        self.assertEqual(self.flags(), (True, False))
        self.assertEqual(self.merge()[0], 200)
        path = os.path.join(self.state, "merge", "1.json")
        first = slurp(path)
        self.assertEqual(self.merge()[0], 200)
        self.assertEqual(slurp(path), first)                        # idempotent
        self.assertEqual(self.flags(), (True, True))
        self.ok("todo")
        self.assertEqual(self.flags(), (False, False))
        self.ok("ok")
        self.assertEqual(self.flags(), (True, False))               # not back on its own
        self.assertEqual(self.merge()[0], 200)
        self.assertEqual(self.flags(), (True, True))
        self.assertNotEqual(slurp(path), first)


class TestWaitServed(ServerBase):
    def test_real_post_during_the_wait_prints_retour_and_comment(self):
        e = dict(self.env)
        e.update({"CLAUDE_CODE_SESSION_ID": "S1", "SF_SUIVI_WAIT_POLL": "0.05"})
        p = subprocess.Popen([sys.executable, "-I", SUIVI, "wait", "1", "--timeout", "20"], cwd=self.repo, env=e,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(lambda: (p.poll() is None and p.kill(), p.communicate()))
        end = time.time() + 5
        while not os.path.exists(os.path.join(self.state, "waits", "1.json")) and time.time() < end:
            time.sleep(0.02)
        time.sleep(0.2)
        self.assertEqual(self.post(body={"status": "retour", "comment": "too dark"})[0], 200)
        out, _ = p.communicate(timeout=10)
        self.assertIn("1-1 retour: too dark", out)


class TestAgentIdleServed(ServerBase):
    def test_api_state_carries_agent_idle_min(self):
        proj = os.path.join(self.tmp.name, "projects", "-p")
        os.makedirs(proj)
        self.stop()
        self.env["SF_CLAUDE_PROJECTS"] = os.path.dirname(proj)
        p = os.path.join(proj, "S1.jsonl")
        open(p, "w").close()
        os.utime(p, (time.time() - 600, time.time() - 600))
        self.put(1, colour=C[0], name="One", owner=self.live_owner("S1"))
        self.proc = subprocess.Popen([sys.executable, "-I", SUIVI, "serve", "--port", "0", "--state", self.state],
                                     cwd=self.repo, env=dict(self.env, **self.EXTRA_ENV), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.port = int(re.search(r":(\d+)/", self.proc.stdout.readline()).group(1))
        status, _, body = self.req("GET", "/api/state")
        self.assertEqual(json.loads(body)["colours"][0]["agent_idle_min"], 10)


class TestServer(ServerBase):
    def test_state_carries_settled_and_pending_is_unchanged(self):
        self.assertEqual(json.loads(self.req("GET", "/api/state")[2])["pending"], 1)
        self.assertEqual(self.post(body={"status": "retour", "comment": "no"})[0], 200)
        self.assertEqual(json.loads(self.req("GET", "/api/state")[2])["pending"], 0)
        r = subprocess.run([sys.executable, "-I", SUIVI, "test", "settle", "1-1", "--note", "n"], cwd=self.repo,
                           env=dict(self.env, **self.as_session("S1")), capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        st = json.loads(self.req("GET", "/api/state")[2])
        e = [t for t in st["tests"] if t["id"] == "1-1"][0]
        self.assertEqual((e["status"], e["settled"]["note"], st["pending"]), ("retour", "n", 0))

    def settled_entry(self, status="retour"):
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": status, "comment": "bad", "try": "scripts/try.sh 1",
                       "settled": {"at": "2026-11-09T10:00:00", "by": "agent"}}, f)

    def test_a_note_keeps_settled_a_new_retour_or_an_ok_clears_it(self):
        self.settled_entry()
        self.assertEqual(self.post(body={"comment": "just a note"})[0], 200)
        e = json.loads(self.entry_text())
        self.assertEqual((e["comment"], e["settled"]["by"]), ("just a note", "agent"))
        self.assertEqual(self.post(body={"status": "retour", "comment": "again"})[0], 200)
        self.assertNotIn("settled", json.loads(self.entry_text()))
        self.settled_entry()
        self.assertEqual(self.post(body={"status": "ok"})[0], 200)
        e = json.loads(self.entry_text())
        self.assertEqual((e["status"], "settled" in e), ("ok", False))

    def test_an_idea_goes_with_ok_and_is_dropped_by_a_later_status(self):
        self.ok_entry()
        self.assertEqual(self.post(body={"status": "retour", "idea": "x"})[0], 400)
        self.assertEqual(self.post(body={"idea": "x"})[0], 400)
        self.assertEqual(self.post(body={"status": "ok", "idea": "  "})[0], 400)
        self.assertEqual(json.loads(self.entry_text()).get("idea"), None)
        self.assertEqual(self.post(body={"status": "ok", "idea": " more "})[0], 200)
        e = json.loads(self.entry_text())
        self.assertEqual((e["status"], e["idea"]), ("ok", "more"))
        st = json.loads(self.req("GET", "/api/state")[2])
        self.assertEqual([t["idea"] for t in st["tests"] if t["id"] == "1-1"], ["more"])
        self.assertEqual(self.post(body={"comment": "note"})[0], 200)                    # a note keeps it
        self.assertEqual(json.loads(self.entry_text())["idea"], "more")
        self.assertEqual(self.post(body={"status": "todo"})[0], 200)
        self.assertNotIn("idea", json.loads(self.entry_text()))
        self.assertEqual(self.post(body={"status": "ok", "idea": "again"})[0], 200)
        self.assertEqual(self.post(body={"status": "ok"})[0], 200)
        self.assertNotIn("idea", json.loads(self.entry_text()))

    def test_an_ok_with_an_idea_is_ready_to_merge(self):
        self.ok_entry()
        self.assertEqual(self.post(body={"status": "ok", "idea": "more"})[0], 200)
        e = json.loads(self.req("GET", "/api/state")[2])["colours"][0]
        self.assertTrue(e["merge_ready"])
        self.assertEqual(self.run_cli("test", "unsettled", "1").returncode, 0)

    def ok_entry(self):
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": "todo", "try": "x"}, f)

    def test_the_page_settles_a_retour_as_the_user_and_only_a_retour_once(self):
        S = {"settle": True}
        for st in ("todo", "ok"):
            with open(self.entry, "w") as f:
                json.dump({"n": 1, "title": "T", "status": st, "try": "x"}, f)
            before = self.entry_text()
            self.assertEqual(self.post(body=S)[0], 409)
            self.assertEqual(self.entry_text(), before)
        with open(self.entry, "w") as f:
            json.dump({"n": 1, "title": "T", "status": "retour", "comment": "c", "reviewed": "R", "try": "x"}, f)
        self.assertEqual(self.post(body={"settle": "yes"})[0], 400)
        self.assertEqual(self.post(body={"settle": True, "status": "ok"})[0], 400)
        self.assertEqual(self.post(body=S, headers={"Content-Type": "application/json"})[0], 403)
        self.assertEqual(self.post(body=S)[0], 200)
        e = json.loads(self.entry_text())
        self.assertEqual((e["status"], e["settled"]["by"], e["reviewed"], e["comment"]), ("retour", "user", "R", "c"))
        self.assertNotIn("commit", e["settled"])
        before = self.entry_text()
        self.assertEqual(self.post(body=S)[0], 409)
        self.assertEqual(self.entry_text(), before)
        self.assertEqual(self.post("/api/tests/1-9", body=S)[0], 404)

    def test_csp_lets_the_inline_tab_icon_load(self):
        status, headers, _ = self.req("GET", "/")
        csp = {k.lower(): v for k, v in headers.items()}["content-security-policy"]
        self.assertIn("img-src 'self' data:", csp)
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



class TestImageCap(ServerBase):
    """The global cap on stored screenshots (#456, lot 1b): a small cap through the env hook, the CLI in-process."""
    EXTRA_ENV = {"SF_SUIVI_MAX_IMAGES_TOTAL": "2500"}
    H = {"X-Suivi": "1", "Content-Type": "image/png"}

    def blob(self, tag, size=1000):
        return png(tag) + bytes(size)

    def up(self, data, path="/api/tests/1-1/images"):
        return self.req("POST", path, self.H, data)

    def stored(self, data, path="/api/tests/1-1/images"):
        status, _, body = self.up(data, path)
        self.assertEqual(status, 200)
        return json.loads(body)["name"]

    def folder(self):
        return os.path.join(self.state, "tests", "1-1.images")

    def age(self, name, secs, folder=None):
        p = os.path.join(folder or self.folder(), name)
        t = time.time() - secs
        os.utime(p, (t, t))

    def exists(self, name, folder=None):
        return os.path.exists(os.path.join(folder or self.folder(), name))

    def test_the_oldest_files_go_first_and_the_new_one_stays(self):
        a, b = self.stored(self.blob(b"a")), self.stored(self.blob(b"b"))
        self.age(a, 300)
        self.age(b, 200)
        c = self.stored(self.blob(b"c"))                       # 3 x ~1000 > 2500
        self.assertEqual((self.exists(a), self.exists(b), self.exists(c)), (False, True, True))

    def test_the_file_just_uploaded_stays_even_when_it_is_the_oldest(self):
        a, b = self.stored(self.blob(b"a")), self.stored(self.blob(b"b"))
        self.age(a, 100)
        self.age(b, 200)
        c = self.stored(self.blob(b"c"))
        self.age(c, 1000)                                       # now the oldest, but new files are judged at upload
        d = self.stored(self.blob(b"d", 3000))                  # alone above the cap
        self.assertEqual(os.listdir(self.folder()), [d])

    def test_uploading_the_same_bytes_again_counts_as_new(self):
        a, b = self.stored(self.blob(b"a")), self.stored(self.blob(b"b"))
        self.age(a, 300)
        self.age(b, 200)
        self.assertEqual(self.stored(self.blob(b"a")), a)       # refreshes a: now b is the oldest
        self.stored(self.blob(b"c"))
        self.assertEqual((self.exists(a), self.exists(b)), (True, False))

    def test_the_archives_and_the_qcm_folders_are_counted_and_evicted(self):
        arch = os.path.join(self.state, "removed-tests", "0-1.images")
        os.makedirs(arch)
        old = "0123456789abcdef01234567.png"
        for n, tag in ((old, b"o"), ("1123456789abcdef01234567.png", b"p")):
            with open(os.path.join(arch, n), "wb") as f:
                f.write(self.blob(tag))
        self.age(old, 900, arch)
        self.age("1123456789abcdef01234567.png", 800, arch)
        self.stored(self.blob(b"n"))                            # 3 files now > 2500
        self.assertFalse(self.exists(old, arch))
        self.assertTrue(self.exists("1123456789abcdef01234567.png", arch))

    def test_a_withdrawn_form_s_screenshots_are_counted_and_evicted(self):
        arch = os.path.join(self.state, "removed-qcm", "0-1", "images", "R1")
        os.makedirs(arch)
        old = "0123456789abcdef01234567.png"
        for n, tag in ((old, b"o"), ("1123456789abcdef01234567.png", b"p")):
            with open(os.path.join(arch, n), "wb") as f:
                f.write(self.blob(tag))
        self.age(old, 900, arch)
        self.age("1123456789abcdef01234567.png", 800, arch)
        self.stored(self.blob(b"n"))                            # 3 files now > 2500
        self.assertFalse(self.exists(old, arch))
        self.assertTrue(self.exists("1123456789abcdef01234567.png", arch))

    def test_eviction_rewrites_no_json_and_leaves_the_state_etag_alone(self):
        a = self.stored(self.blob(b"a"))
        self.assertEqual(self.req("POST", "/api/tests/1-1", {"X-Suivi": "1", "Content-Type": "application/json"},
                                  {"images": [a]})[0], 200)
        self.age(a, 500)
        tag = self.req("GET", "/api/state")[1]["ETag"]
        with open(self.entry, "rb") as f:
            before = f.read()
        mtime = os.stat(self.entry).st_mtime_ns
        self.stored(self.blob(b"b"))
        self.stored(self.blob(b"c"))                            # evicts a
        self.assertFalse(self.exists(a))
        with open(self.entry, "rb") as f:
            self.assertEqual(f.read(), before)
        self.assertEqual(os.stat(self.entry).st_mtime_ns, mtime)
        self.assertEqual(self.req("GET", "/api/state", {"If-None-Match": tag})[0], 304)

    def test_a_save_drops_a_recorded_missing_name_but_refuses_an_unrecorded_one(self):
        a, b = self.stored(self.blob(b"a")), self.stored(self.blob(b"b"))
        post = lambda body: self.req("POST", "/api/tests/1-1", {"X-Suivi": "1", "Content-Type": "application/json"}, body)
        self.assertEqual(post({"images": [a, b]})[0], 200)
        os.unlink(os.path.join(self.folder(), a))               # what the cap does
        self.assertEqual(post({"images": [a, b], "comment": "c"})[0], 200)
        self.assertEqual(jload(self.entry)["images"], [b])
        self.assertEqual(post({"images": [b, "ffffffffffffffffffffffff.png"]})[0], 400)   # never recorded
        self.assertEqual(jload(self.entry)["images"], [b])

    def test_a_qcm_save_does_the_same(self):
        d = os.path.join(self.state, "qcm", "1-1")
        os.makedirs(os.path.join(d, "answers"))
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump({"issue": 1, "title": "T", "cards": 1, "finished": False}, f)
        up = "/api/qcm/1-1/images/R1"
        a, b = self.stored(self.blob(b"a"), up), self.stored(self.blob(b"b"), up)
        ans = lambda imgs: self.req("POST", "/api/qcm/1-1/answer", {"X-Suivi": "1"}, {"code": "R1", "choix": "Y", "images": imgs})
        self.assertEqual(ans([a, b])[0], 200)
        os.unlink(os.path.join(d, "images", "R1", a))
        self.assertEqual(ans([a, b])[0], 200)
        self.assertEqual(jload(os.path.join(d, "answers", "R1.json"))["images"], [b])
        self.assertEqual(ans([b, "ffffffffffffffffffffffff.png"])[0], 400)


class TestImageCapCli(Base):
    """What the CLI says about a name whose file the cap deleted."""
    NAME = "0123456789abcdef01234567.png"

    def own(self):
        return self.as_session("S1")

    def test_dangling_names_are_said_to_be_removed_everywhere(self):
        self.claim(7)
        d = os.path.join(self.state, "tests")
        os.makedirs(d)
        with open(os.path.join(d, "7-1.json"), "w") as f:
            json.dump({"n": 1, "title": "T", "status": "todo", "try": "t", "images": [self.NAME]}, f)
        out = self.run_cli("test", "list", "--issue", "7").stdout
        self.assertIn("  image removed by the size limit: " + self.NAME, out)
        self.assertNotIn("image: ", out.replace("image removed", ""))
        r = self.run_cli("test", "remove", "7", "7-1", env=self.own())  # still counts as answered
        self.assertNotEqual(r.returncode, 0)
        self.assertEqual(self.run_cli("test", "remove", "7", "7-1", "--answered", env=self.own()).returncode, 0)
        qd = os.path.join(self.state, "qcm", "7-1")
        os.makedirs(os.path.join(qd, "answers"))
        with open(os.path.join(qd, "meta.json"), "w") as f:
            json.dump({"issue": 7, "title": "T", "cards": 1, "finished": False}, f)
        with open(os.path.join(qd, "answers", "R1.json"), "w") as f:
            json.dump({"code": "R1", "choix": "Y", "images": [self.NAME]}, f)
        text = self.run_cli("qcm", "answers", "7-1").stdout
        self.assertIn("  image removed by the size limit: " + self.NAME, text)
        got = json.loads(self.run_cli("qcm", "answers", "7-1", "--json").stdout)["answers"]["R1"]
        self.assertEqual((got.get("image_paths"), got["images_removed_by_size_limit"]), (None, [self.NAME]))


class TestImages(ServerBase):
    """Screenshots on trial entries and QCM cards (#456): raw upload, strict names, safe GET."""
    H = {"X-Suivi": "1", "Content-Type": "image/png"}

    def up(self, data, path="/api/tests/1-1/images", headers=None, host="default"):
        return self.req("POST", path, self.H if headers is None else headers, data, host)

    def folder(self):
        return os.path.join(self.state, "tests", "1-1.images")

    def files(self):
        return sorted(os.listdir(self.folder())) if os.path.isdir(self.folder()) else []

    def stored(self, data):
        status, _, body = self.up(data)
        self.assertEqual(status, 200)
        return json.loads(body)["name"]

    def make_qcm(self):
        d = os.path.join(self.state, "qcm", "1-1")
        os.makedirs(os.path.join(d, "answers"))
        with open(os.path.join(d, "meta.json"), "w") as f:
            json.dump({"issue": 1, "title": "T", "cards": 2, "finished": False}, f)
        return d

    def answer(self, body, qid="1-1"):
        return self.req("POST", "/api/qcm/%s/answer" % qid, {"X-Suivi": "1"}, dict({"code": "R1", "choix": "Yes"}, **body))

    def test_each_format_is_stored_as_sent_without_touching_the_entry(self):
        before = self.entry_text()
        tag = self.req("GET", "/api/state")[1]["ETag"]
        for ext, data in IMG.items():
            status, _, body = self.up(data)
            self.assertEqual(status, 200, ext)
            name = json.loads(body)["name"]
            self.assertRegex(name, IMG_NAME)
            self.assertTrue(name.endswith("." + ext), name)
            with open(os.path.join(self.folder(), name), "rb") as f:
                self.assertEqual(f.read(), data)
        self.assertEqual(self.entry_text(), before)                              # the entry lists images only when sent
        self.assertEqual(self.req("GET", "/api/state", {"If-None-Match": tag})[0], 304)
        self.assertEqual(len(self.files()), 4)
        self.stored(IMG["png"])
        self.assertEqual(len(self.files()), 4)                                   # same bytes: one file

    def test_what_is_not_a_listed_image_is_refused_and_leaves_no_file(self):
        for data in (b"<svg xmlns='http://www.w3.org/2000/svg'><script/></svg>", b"hello", b"<html></html>", b"", b"RIFF\0\0\0\0WAVE"):
            self.assertIn(self.up(data)[0], (400, 415), data)                     # always sent as image/png
        self.assertEqual(self.files(), [])
        self.assertFalse(os.path.exists(self.folder()))

    def test_a_body_shorter_than_its_content_length_is_refused(self):
        import socket
        s = socket.create_connection(("127.0.0.1", self.port), timeout=10)
        s.sendall(("POST /api/tests/1-1/images HTTP/1.0\r\nHost: 127.0.0.1:%d\r\nX-Suivi: 1\r\nContent-Length: %d\r\n\r\n"
                   % (self.port, len(IMG["png"]) + 50)).encode() + IMG["png"])
        s.shutdown(socket.SHUT_WR)
        reply = s.recv(4096)
        s.close()
        self.assertTrue(reply.startswith(b"HTTP/1.0 400"), reply)
        self.assertEqual(self.files(), [])

    def test_upload_guards_and_limits(self):
        data = IMG["png"]
        self.assertEqual(self.up(data, headers={"Content-Type": "image/png"})[0], 403)           # no X-Suivi
        self.assertEqual(self.up(data, headers=dict(self.H, Origin="http://evil.example"))[0], 403)
        self.assertEqual(self.up(data, host="evil.example")[0], 403)
        for bad in (".hidden", "a%2Fb", ".."):
            self.assertEqual(self.up(data, "/api/tests/%s/images" % bad)[0], 400, bad)
        self.assertEqual(self.up(data, "/api/qcm/1-1/images/.h")[0], 400)
        self.assertEqual(self.up(data, "/api/tests/9-9/images")[0], 404)
        self.assertEqual(self.up(data, "/api/qcm/9-9/images/R1")[0], 404)
        self.assertEqual(self.files(), [])
        big = IMG["png"] + b"\0" * suivi.MAX_IMAGE                                               # one byte past the cap
        self.assertEqual(len(big), suivi.MAX_IMAGE + len(IMG["png"]))
        self.assertEqual(self.up(big)[0], 413)
        self.assertEqual(self.files(), [])
        self.assertEqual(self.post(body=b'{"comment":"%s"}' % (b"x" * (256 * 1024 + 10)))[0], 413)   # the JSON cap stays

    def test_get_serves_the_stored_bytes_with_the_server_type_and_safe_headers(self):
        for ext, data in IMG.items():
            name = self.stored(data)
            status, h, body = self.req("GET", "/api/tests/1-1/images/" + name)
            h = {k.lower(): v for k, v in h.items()}
            self.assertEqual((status, body), (200, data))
            self.assertEqual(h["content-type"], suivi.IMAGE_TYPES[ext])
            self.assertEqual(h["x-content-type-options"], "nosniff")
            self.assertEqual(h["cross-origin-resource-policy"], "same-origin")
            self.assertIn("immutable", h["cache-control"])
            self.assertNotIn("no-store", h["cache-control"])
        self.assertEqual(self.req("GET", "/api/state")[1]["Cache-Control"], "no-store")
        self.assertIn("img-src 'self' data:", self.req("GET", "/api/tests/1-1/images/" + name)[1]["Content-Security-Policy"])

    def test_get_refuses_other_names_other_sites_and_other_hosts(self):
        name = self.stored(IMG["png"])
        for planted in ("x.svg", "1-1.json", name + ".svg", "ABCDEF0123456789ABCDEF01.png"):
            with open(os.path.join(self.folder(), planted), "wb") as f:
                f.write(b"<svg/>")
            self.assertEqual(self.req("GET", "/api/tests/1-1/images/" + planted)[0], 400, planted)
        self.assertEqual(self.req("GET", "/api/tests/1-1/images/..%2f1-1.json")[0], 400)
        self.assertEqual(self.req("GET", "/api/tests/..%2f/images/" + name)[0], 400)
        self.assertEqual(self.req("GET", "/api/tests/1-1/images/0123456789abcdef01234567.png")[0], 404)
        for site in ("cross-site", "same-site"):
            self.assertEqual(self.req("GET", "/api/tests/1-1/images/" + name, {"Sec-Fetch-Site": site})[0], 403, site)
        for site in ("same-origin", "none"):
            self.assertEqual(self.req("GET", "/api/tests/1-1/images/" + name, {"Sec-Fetch-Site": site})[0], 200, site)
        self.assertEqual(self.req("GET", "/api/tests/1-1/images/" + name, host="evil.example")[0], 403)
        self.assertEqual(self.req("GET", "/api/tests/1-1/images/" + name, {"Origin": "http://evil.example"})[0], 403)

    def test_the_final_post_lists_images_without_a_comment_and_removes_the_others(self):
        a, b = self.stored(png(b"a")), self.stored(png(b"b"))
        status, _, _ = self.post(body={"images": [a]})
        self.assertEqual(status, 200)
        t = json.loads(self.entry_text())
        self.assertEqual((t["images"], t["status"], "comment" in t), ([a], "todo", False))   # status unchanged
        self.assertEqual(self.files(), [a])                                                  # unlisted file deleted
        c = self.stored(png(b"c"))
        self.assertEqual(self.post(body={"status": "ok"})[0], 200)                           # no `images`: left as they are
        self.assertEqual(json.loads(self.entry_text())["images"], [a])
        self.assertEqual(self.post(body={"images": [a, a, c]})[0], 200)
        self.assertEqual(json.loads(self.entry_text())["images"], [a, c])
        self.assertEqual(self.post(body={"images": []})[0], 200)
        self.assertNotIn("images", json.loads(self.entry_text()))
        self.assertFalse(os.path.exists(self.folder()))

    def test_the_final_post_refuses_bad_image_lists_and_changes_nothing(self):
        a = self.stored(png(b"a"))
        self.assertEqual(self.post(body={"images": [a]})[0], 200)
        before, files = self.entry_text(), self.files()
        names = [self.stored(png(bytes([i, 200]))) for i in range(suivi.MAX_IMAGES)]           # MAX_IMAGES + a = one too many
        for bad in ("abc", {"x": 1}, [1], ["0123456789abcdef01234567.png"], ["../x"], [a] + names, None):
            self.assertEqual(self.post(body={"images": bad})[0], 400, bad)
        self.assertEqual(self.entry_text(), before)
        self.assertEqual(self.files(), sorted(files + names))
        self.assertEqual(self.post(body={"images": names})[0], 200)                            # exactly the sanity bound is fine

    def test_more_than_five_images_are_accepted(self):
        names = [self.stored(png(bytes([i]))) for i in range(8)]
        self.assertEqual(self.post(body={"images": names})[0], 200)
        self.assertEqual(json.loads(self.entry_text())["images"], names)

    def test_an_image_changes_neither_the_colour_nor_the_pending_count(self):
        held, pending = self.held(C[0]), json.loads(self.req("GET", "/api/state")[2])["pending"]
        self.assertEqual(self.post(body={"images": [self.stored(IMG["png"])]})[0], 200)
        self.assertEqual((self.held(C[0]), json.loads(self.req("GET", "/api/state")[2])["pending"]), (held, pending))
        self.assertIn("essai", self.held(C[0]))

    def test_qcm_answers_take_images_and_any_other_list_stays_refused(self):
        self.make_qcm()
        up = "/api/qcm/1-1/images/R1"
        status, _, body = self.up(IMG["jpg"], up)
        self.assertEqual(status, 200)
        name = json.loads(body)["name"]
        self.assertEqual(self.answer({"images": [name], "commentaire": "see"})[0], 200)
        rec = jload(os.path.join(self.state, "qcm", "1-1", "answers", "R1.json"))
        self.assertEqual((rec["images"], rec["commentaire"]), ([name], "see"))
        status, h, got = self.req("GET", "/api/qcm/1-1/images/R1/" + name)
        self.assertEqual((status, got, h["Content-Type"]), (200, IMG["jpg"], "image/jpeg"))
        self.assertEqual(self.req("GET", "/api/qcm/1-1/images/R2/" + name)[0], 404)             # per card
        self.assertEqual(self.answer({"commentaire": ["a"]})[0], 400)
        self.assertEqual(self.answer({"images": ["../x"]})[0], 400)
        self.assertEqual(self.answer({"images": "x"})[0], 400)
        self.assertEqual(jload(os.path.join(self.state, "qcm", "1-1", "answers", "R1.json"))["images"], [name])
        other = json.loads(self.up(png(b"o"), up)[2])["name"]
        self.assertEqual(self.answer({"images": [name]})[0], 200)
        self.assertEqual(os.listdir(os.path.join(self.state, "qcm", "1-1", "images", "R1")), [name])   # the other is removed
        self.assertNotEqual(other, name)
        self.assertEqual(self.answer({"commentaire": "no image now"})[0], 200)                    # the record is the truth
        self.assertNotIn("images", jload(os.path.join(self.state, "qcm", "1-1", "answers", "R1.json")))
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm", "1-1", "images", "R1")))


class TestQcm(ServerBase):
    def setUp(self):
        super().setUp()
        self.assertEqual(self.run_cli("pause", "1").returncode, 0)     # only a form may hold the colour of #1
        os.remove(self.entry)
        self.data = os.path.join(self.tmp.name, "data.js")
        with open(self.data, "w", encoding="utf-8") as f:
            f.write(DATA)

    def pose(self, issue=1, data=None, *extra):
        r = self.run_cli("qcm", "pose", str(issue), data or self.data, "--title", "My <form>", *extra)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout.split()[0]

    def finish_form(self, qid):
        self.assertEqual(self.req("POST", "/api/qcm/%s/finish" % qid, {"X-Suivi": "1"}, {})[0], 200)

    def data_file(self, *titles):
        """A data.js whose cards R1, R2... carry these titles."""
        cards = ",\n".join("{c:'R%d',p:%d,t:%s,is:'a',o:[O('A'),O('B')]}" % (i + 1, i + 1, json.dumps(t))
                           for i, t in enumerate(titles))
        path = os.path.join(self.tmp.name, "d%d.js" % abs(hash(titles)))
        with open(path, "w", encoding="utf-8") as f:
            f.write(DATA.split("const D=[")[0] + "const D=[\n" + cards + ",\n];\n")
        return path

    def withdraw(self, qid, *extra, sid="S1", env=None):
        return self.run_cli("qcm", "withdraw", qid, *extra, env=self.as_session(sid) if env is None else env)

    def archived(self, qid):
        return os.path.join(self.state, "removed-qcm", qid)

    def answer(self, qid, code="R1", body=None, headers="ok"):
        h = {"X-Suivi": "1", "Content-Type": "application/json"} if headers == "ok" else headers
        b = {"code": code, "priorite": 1, "question": "Q1", "issue": 1, "statut": "tranche", "choix": "Yes",
             "recommande": True, "commentaire": "ok then", "date": "2026-11-03T10:00:00"} if body is None else body
        return self.req("POST", "/api/qcm/%s/answer" % qid, h, b)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_injects_the_data_block_and_title(self):
        qid = self.pose()
        self.assertEqual(qid, "1-1")
        self.finish_form(qid)
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
        meta = jload(os.path.join(self.state, "qcm", "1-2", "meta.json"))
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

    # ---- #520: card cap, list card, code words, qcm-log.tsv
    def cards_file(self, name, cards, issues="const ISSUES={a:{n:1,name:'One',colour:'x'}};"):
        path = os.path.join(self.tmp.name, name)
        with open(path, "w", encoding="utf-8") as f:
            f.write("const TITRE='T';const SEC={};%s const INTRO='';\nconst D=[\n%s\n];\n" % (issues, ",\n".join(cards)))
        return path

    def plain(self, n):
        return ["{c:'R%d',p:1,t:'Q%d',is:'a',o:[O('A'),O('B')]}" % (i, i) for i in range(1, n + 1)]

    def log_lines(self):
        p = os.path.join(self.state, "qcm-log.tsv")
        if not os.path.isfile(p):
            return []
        with open(p, encoding="utf-8") as f:
            return [l.rstrip("\n").split("\t") for l in f if l.strip()]

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_refuses_more_than_eight_cards_and_force_overrides(self):
        r = self.run_cli("qcm", "pose", "1", self.cards_file("c9.js", self.plain(9)))
        self.assertEqual(r.returncode, 1)
        self.assertIn("9 cards", r.stderr)
        self.assertIn("limit is 8", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm")))
        self.assertEqual([l[1:] for l in self.log_lines()], [["refused", "1", "", "cap"]])
        self.assertEqual(self.pose(1, self.cards_file("c8.js", self.plain(8))), "1-1")
        self.finish_form("1-1")
        r = self.run_cli("qcm", "pose", "1", self.cards_file("c9b.js", self.plain(9)), "--force")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("--force", r.stderr)
        forced = [l for l in self.log_lines() if l[1] == "forced"]
        self.assertEqual([(l[2], l[3], l[4]) for l in forced], [("1", "1-2", "cap")])

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_list_card_counts_as_one_whatever_its_lines(self):
        lines = ",".join("'Line %d.'" % i for i in range(12))
        cards = self.plain(7) + ["{c:'L1',p:1,t:'Unless you object',k:'list',lines:[%s]}" % lines]
        self.assertEqual(self.pose(1, self.cards_file("l.js", cards)), "1-1")

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_malformed_list_card_is_refused(self):
        for card in ("{c:'L1',p:1,t:'T',k:'list'}", "{c:'L1',p:1,t:'T',k:'list',lines:[]}",
                     "{c:'L1',p:1,t:'T',k:'list',lines:['x'],o:[O('A')]}"):
            r = self.run_cli("qcm", "pose", "1", self.cards_file("bad.js", [card]))
            self.assertEqual(r.returncode, 1, card)
            self.assertIn("card L1", r.stderr)
            self.assertIn("list card", r.stderr)
        self.assertEqual({l[4] for l in self.log_lines()}, {"shape"})

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_code_words_are_refused_in_card_text_but_not_in_sketches(self):
        r = self.run_cli("qcm", "pose", "1", self.cards_file("j.js", [
            "{c:'R1',p:1,t:'Q',is:'a',ctx:['The command commit_edit runs.'],o:[O('A'),O('B')]}"]))
        self.assertEqual(r.returncode, 1)
        self.assertIn("card R1", r.stderr)
        self.assertIn("commit_edit", r.stderr)
        for bad in ("t:'See `foo` now'", "t:'The crate::module'", "t:'Edit main.rs'"):
            r = self.run_cli("qcm", "pose", "1", self.cards_file("j2.js", ["{c:'R1',p:1,%s,o:[O('A')]}" % bad]))
            self.assertEqual(r.returncode, 1, bad)
        r = self.run_cli("qcm", "pose", "1", self.cards_file("j3.js", [
            "{c:'R1',p:1,t:'Q',o:[O('A',['x'],'foo_bar'),{t:'B',pts:[],m:'foo_bar',plus:'',moins:'uses do_it'}]}"]))
        self.assertEqual(r.returncode, 1)
        self.assertIn("do_it", r.stderr)
        self.assertEqual(self.pose(1, self.cards_file("j4.js", [
            "{c:'R1',p:1,t:'Q',av:{pts:['Today it is fine.'],m:'foo_bar'},o:[O('A',['x'],'foo_bar'),O('B')]}"])), "1-1")
        self.assertIn("jargon", {l[4] for l in self.log_lines()})

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_long_bullet_only_warns(self):
        long = " ".join(["word"] * 25)
        qid = self.pose(1, self.cards_file("w.js", ["{c:'R1',p:1,t:'Q',ctx:['%s'],o:[O('A'),O('B')]}" % long]))
        self.assertEqual(qid, "1-1")
        r = self.run_cli("qcm", "pose", "1", self.cards_file("w2.js", ["{c:'R7',p:1,t:'Q',ctx:['%s'],o:[O('A')]}" % long]), "--force")
        self.assertIn("card R7", r.stderr)
        self.assertIn("25 words", r.stderr)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_log_records_the_first_answer_only_and_meta_has_created_ts(self):
        qid = self.pose()
        meta = jload(os.path.join(self.state, "qcm", qid, "meta.json"))
        self.assertGreater(meta["created_ts"], time.time() - 120)
        self.assertEqual([(l[1], l[4]) for l in self.log_lines()], [("posed", "2")])
        self.assertEqual(self.answer(qid)[0], 200)
        self.assertEqual(self.answer(qid, "R2", {"code": "R2", "choix": "A"})[0], 200)
        self.assertEqual(self.answer(qid)[0], 200)
        first = [l for l in self.log_lines() if l[1] == "first-answer"]
        self.assertEqual(len(first), 1)
        self.assertGreaterEqual(int(first[0][4]), 0)
        self.assertEqual(first[0][3], qid)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_first_answer_on_a_form_without_created_ts_logs_a_blank(self):
        qid = self.pose()
        mp = os.path.join(self.state, "qcm", qid, "meta.json")
        meta = jload(mp)
        del meta["created_ts"]
        with open(mp, "w") as f:
            json.dump(meta, f)
        self.assertEqual(self.answer(qid)[0], 200)
        first = [l for l in self.log_lines() if l[1] == "first-answer"]
        self.assertEqual(len(first), 1)
        self.assertEqual(first[0][4], "")

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_an_unwritable_log_never_fails_a_pose(self):
        os.makedirs(os.path.join(self.state, "qcm-log.tsv"))
        r = self.run_cli("qcm", "pose", "1", self.data)
        self.assertEqual(r.returncode, 0, r.stderr)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_answers_print_the_lines_to_discuss(self):
        qid = self.pose()
        body = {"code": "R1", "question": "Q1", "statut": "tranche", "choix": "À discuter (2)", "recommande": False,
                "discuter": "First line.\nSecond line.", "commentaire": ""}
        self.assertEqual(self.answer(qid, body=body)[0], 200)
        out = self.run_cli("qcm", "answers", qid).stdout
        self.assertIn("  to discuss: First line.\n  to discuss: Second line.", out)
        j = json.loads(self.run_cli("qcm", "answers", qid, "--json").stdout)
        self.assertEqual(j["answers"]["R1"]["discuter"], "First line.\nSecond line.")

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
        a, b = self.pose(), self.pose(1, None, "--force")
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

    # ---- #479: one open form per issue, withdrawal
    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_refuses_a_second_open_form_of_the_issue_and_force_overrides(self):
        self.assertEqual(self.pose(), "1-1")
        r = self.run_cli("qcm", "pose", "1", self.data)
        self.assertEqual(r.returncode, 1)
        self.assertIn("1-1", r.stderr)
        self.assertIn("--force", r.stderr)
        self.assertEqual(sorted(os.listdir(os.path.join(self.state, "qcm"))), ["1-1"])
        self.assertEqual(self.pose(2), "2-1")                    # another issue's open form is none of its business
        self.assertEqual(self.pose(1, None, "--force"), "1-2")

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_after_a_finished_form_is_allowed_and_silent(self):
        qid = self.pose()
        self.finish_form(qid)
        r = self.run_cli("qcm", "pose", "1", self.data)
        self.assertEqual((r.returncode, r.stdout.split()[0]), (0, "1-2"), r.stderr)
        self.assertNotIn("withdrawn", r.stderr)
        self.assertNotIn("already", r.stderr)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_withdraw_moves_the_whole_folder_and_frees_the_colour(self):
        self.claim(1)
        qid = self.pose()
        img = os.path.join(self.state, "qcm", qid, "images", "c1")
        os.makedirs(img)
        with open(os.path.join(img, "x.png"), "wb") as f:
            f.write(b"png")
        r = self.withdraw(qid)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(os.path.exists(os.path.join(self.archived(qid), "images", "c1", "x.png")))
        self.assertTrue(os.path.exists(os.path.join(self.archived(qid), "data.js")))
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm", qid)))
        meta = jload(os.path.join(self.archived(qid), "meta.json"))
        self.assertEqual((meta["withdrawn_by"], "withdrawn_at" in meta), ("cli", True))
        self.assertEqual(json.loads(self.req("GET", "/api/state")[2])["qcms"], [])
        self.assertNotIn("qcm", self.held())
        self.assertEqual(self.run_cli("qcm", "list").stdout, "")

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_withdrawn_number_is_never_reused(self):
        self.claim(1)
        self.assertEqual(self.pose(), "1-1")
        self.finish_form("1-1")
        self.assertEqual(self.pose(), "1-2")
        self.assertEqual(self.withdraw("1-2").returncode, 0)
        self.assertEqual(self.pose(), "1-3")

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_withdraw_needs_the_owner_session_of_the_issue_or_its_parent(self):
        qid = self.pose()
        before = sorted(os.listdir(os.path.join(self.state, "qcm")))
        self.assertNotEqual(self.withdraw(qid, env={}).returncode, 0)                        # no session
        os.remove(os.path.join(self.state, "issues", "1.json"))
        self.assertIn("chantier.sh 1", self.withdraw(qid).stderr)                           # no record
        self.put(1, colour=C[0])                                                             # no owner
        self.assertNotEqual(self.withdraw(qid).returncode, 0)
        self.put(1, colour=C[0], owner=self.live_owner("S2"))
        self.assertNotEqual(self.withdraw(qid, sid="S1").returncode, 0)                      # another session
        self.assertNotEqual(self.withdraw(qid, env={}).returncode, 0)
        self.assertEqual(sorted(os.listdir(os.path.join(self.state, "qcm"))), before)
        self.assertFalse(os.path.exists(self.archived(qid)))
        # package: the parent's owner withdraws a sub's form, the sub's owner cannot touch the parent's
        self.put(9, kind="package", colour=C[1], owner=self.live_owner("S1"))
        self.put(1, colour=C[1], parent=9, owner=self.live_owner("S2"))
        self.assertEqual(self.withdraw(qid, sid="S1").returncode, 0)
        q9 = self.pose(9)
        self.assertNotEqual(self.withdraw(q9, sid="S2").returncode, 0)
        self.assertTrue(os.path.isdir(os.path.join(self.state, "qcm", q9)))

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_withdraw_of_a_form_with_answers_needs_the_flag_and_keeps_them_readable(self):
        self.claim(1)
        qid = self.pose()
        self.answer(qid, code="R2", body={"code": "R2", "statut": "plus_tard", "question": "Q2", "choix": "Plus tard"})
        r = self.withdraw(qid)                                   # a `plus_tard` alone is an answer too
        self.assertEqual(r.returncode, 1)
        self.assertIn("--answered", r.stderr)
        self.assertTrue(os.path.isdir(os.path.join(self.state, "qcm", qid)))
        self.assertFalse(os.path.exists(self.archived(qid)))
        self.answer(qid)
        r = self.withdraw(qid, "--answered")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("R1 [recommended] Q1 -> Yes -- ok then", r.stdout)
        self.assertTrue(os.path.exists(os.path.join(self.archived(qid), "answers", "R1.json")))
        r = self.run_cli("qcm", "answers", qid)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("R1 [recommended] Q1 -> Yes -- ok then", r.stdout)
        self.assertIn("withdrawn", r.stdout)
        self.assertEqual(json.loads(self.run_cli("qcm", "answers", qid, "--json").stdout)["answers"]["R1"]["choix"], "Yes")

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_finished_or_already_withdrawn_form_is_never_withdrawn(self):
        self.claim(1)
        qid = self.pose()
        self.finish_form(qid)
        for extra in ((), ("--answered",)):
            r = self.withdraw(qid, *extra)
            self.assertEqual(r.returncode, 1)
            self.assertIn("finished", r.stderr)
        self.assertTrue(os.path.isdir(os.path.join(self.state, "qcm", qid)))
        q2 = self.pose()
        self.assertEqual(self.withdraw(q2).returncode, 0)
        r = self.withdraw(q2)
        self.assertEqual(r.returncode, 1)
        self.assertIn("already withdrawn", r.stderr)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_page_withdraw_button_endpoint(self):
        qid = self.pose()
        self.answer(qid)
        w = "/api/qcm/%s/withdraw" % qid
        self.assertEqual(self.req("POST", w, {}, {})[0], 403)                                # no X-Suivi
        self.assertEqual(self.req("POST", "/api/qcm/9-9/withdraw", {"X-Suivi": "1"}, {})[0], 404)
        done = self.pose(2)
        self.finish_form(done)
        self.assertEqual(self.req("POST", "/api/qcm/%s/withdraw" % done, {"X-Suivi": "1"}, {})[0], 409)
        self.assertTrue(os.path.isdir(os.path.join(self.state, "qcm", done)))
        self.assertEqual(self.req("POST", w, {"X-Suivi": "1"}, {})[0], 200)                  # answers do not matter here
        self.assertTrue(os.path.exists(os.path.join(self.archived(qid), "answers", "R1.json")))
        self.assertEqual(jload(os.path.join(self.archived(qid), "meta.json"))["withdrawn_by"], "page")
        self.assertEqual(self.req("POST", w, {"X-Suivi": "1"}, {})[0], 404)
        self.assertEqual(self.req("GET", "/qcm/" + qid)[0], 410)
        self.assertEqual(self.req("GET", "/api/qcm/%s/answers" % qid)[0], 410)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_withdrawn_form_is_not_recreated_by_a_late_answer_or_finish(self):
        qid = self.pose()
        self.assertEqual(self.req("POST", "/api/qcm/%s/withdraw" % qid, {"X-Suivi": "1"}, {})[0], 200)
        self.assertEqual(self.answer(qid)[0], 410)
        self.assertEqual(self.req("POST", "/api/qcm/%s/finish" % qid, {"X-Suivi": "1"}, {})[0], 410)
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm", qid)))
        gone = self.req("GET", "/qcm/" + qid)
        self.assertEqual(gone[0], 410)
        self.assertIn("retiré", gone[2].decode("utf-8"))                 # the page a reload shows is French
        self.assertEqual(self.answer("9-9")[0], 404)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_withdrawn_form_keeps_its_screenshots_readable_and_takes_no_more(self):
        self.claim(1)
        qid = self.pose()
        up = "/api/qcm/%s/images/R1" % qid
        status, _, body = self.req("POST", up, {"X-Suivi": "1", "Content-Type": "image/png"}, png(b"a"))
        self.assertEqual(status, 200)
        name = json.loads(body)["name"]
        self.assertEqual(self.answer(qid, body={"code": "R1", "statut": "tranche", "question": "Q1", "choix": "Yes", "images": [name]})[0], 200)
        r = self.withdraw(qid, "--answered")
        self.assertEqual(r.returncode, 0, r.stderr)
        path = os.path.join(self.archived(qid), "images", "R1", name)
        self.assertTrue(os.path.exists(path))
        self.assertIn("image: " + path, r.stdout)
        self.assertIn("image: " + path, self.run_cli("qcm", "answers", qid).stdout)
        self.assertEqual(json.loads(self.run_cli("qcm", "answers", qid, "--json").stdout)["answers"]["R1"]["image_paths"], [path])
        self.assertEqual(self.req("POST", up, {"X-Suivi": "1", "Content-Type": "image/png"}, png(b"b"))[0], 410)
        self.assertFalse(os.path.exists(os.path.join(self.state, "qcm", qid)))

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_check_reports_two_open_forms_of_one_issue(self):
        a = self.pose()
        self.pose(1, None, "--force")
        r = self.run_cli("check")
        self.assertEqual(r.returncode, 1)
        self.assertRegex(r.stdout, r"issue #1 has 2 open forms: 1-1, 1-2")
        self.finish_form("1-1")
        self.assertNotIn("open forms", self.run_cli("check").stdout)       # open + finished: nothing to report

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_pose_reminds_of_withdrawn_forms_holding_answers_and_refuses_a_repeated_question(self):
        self.claim(1)
        q = self.pose(1, self.data_file("Which colour?", "Which size?"))
        self.answer(q, code="R1", body={"code": "R1", "statut": "tranche", "question": "Which colour?", "choix": "Red"})
        self.answer(q, code="R2", body={"code": "R2", "statut": "plus_tard", "question": "Which size?", "choix": "Plus tard"})
        self.assertEqual(self.withdraw(q, "--answered").returncode, 0)
        before = sorted(os.listdir(os.path.join(self.state, "qcm"))) if os.path.isdir(os.path.join(self.state, "qcm")) else []
        # identical title answered: refused, nothing created
        r = self.run_cli("qcm", "pose", "1", self.data_file("Which colour?", "Something else"))
        self.assertEqual(r.returncode, 1)
        self.assertIn("1-1", r.stderr)
        self.assertIn("Which colour?", r.stderr)
        self.assertEqual(sorted(os.listdir(os.path.join(self.state, "qcm"))) if os.path.isdir(os.path.join(self.state, "qcm")) else [], before)
        # whitespace and case variants are the same question
        r = self.run_cli("qcm", "pose", "1", self.data_file("  WHICH   colour? ", "Else"))
        self.assertEqual(r.returncode, 1)
        # a title that was only « Plus tard » may be asked again; the reminder line is printed (one answer)
        r = self.run_cli("qcm", "pose", "1", self.data_file("Which size?"))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("withdrawn form 1-1 holds 1 answer", r.stderr)
        self.assertIn("suivi.py qcm answers 1-1", r.stderr)
        self.assertEqual(r.stdout.split()[0], "1-2")
        self.finish_form("1-2")
        # --force overrides the repeat
        r = self.run_cli("qcm", "pose", "1", self.data_file("Which colour?"), "--force")
        self.assertEqual((r.returncode, r.stdout.split()[0]), (0, "1-3"), r.stderr)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_repeat_check_ignores_empty_withdrawn_forms_other_issues_and_finished_forms(self):
        self.claim(1)
        self.claim(2)
        titles = self.data_file("Which colour?")
        # a withdrawn form with no answer: no line, no refusal
        q = self.pose(1, titles)
        self.assertEqual(self.withdraw(q).returncode, 0)
        r = self.run_cli("qcm", "pose", "1", titles)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("withdrawn", r.stderr)
        self.finish_form("1-2")
        # an answered withdrawn form of another issue changes nothing here
        q2 = self.pose(2, titles)
        self.answer(q2, body={"code": "R1", "statut": "tranche", "question": "Which colour?", "choix": "Red"})
        self.assertEqual(self.withdraw(q2, "--answered").returncode, 0)
        r = self.run_cli("qcm", "pose", "1", titles)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("withdrawn", r.stderr)
        # the 463-1 then 463-2 case: a finished (not withdrawn) form with the same title and an answer
        self.answer("1-3", body={"code": "R1", "statut": "tranche", "question": "Which colour?", "choix": "Red"})
        self.finish_form("1-3")
        r = self.run_cli("qcm", "pose", "1", titles)
        self.assertEqual((r.returncode, r.stdout.split()[0]), (0, "1-4"), r.stderr)


@unittest.skipUnless(shutil.which("node"), "node is not installed")
class TestFormTemplate(unittest.TestCase):
    def test_option_clicks_are_bound_to_real_options_only(self):
        # #520 trial: a list card's lines are .opt too; binding choose() to every .opt made a click on
        # « À discuter » bubble to its line and wipe the marks (reproduced in Chromium, then fixed).
        with open(TEMPLATE, encoding="utf-8") as f:
            src = f.read()
        self.assertIn("querySelectorAll('.opt[data-i]').forEach", src)
        self.assertNotIn("querySelectorAll('.opt').forEach", src)

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

    def form_html(self):
        with open(TEMPLATE, encoding="utf-8") as f:
            return f.read()

    def q_js(self, expr):
        src = re.search(r"/\*QIMG-START\*/(.*)/\*QIMG-END\*/", self.form_html(), re.S).group(1)
        r = subprocess.run(["node", "-e", src + "\nprocess.stdout.write(JSON.stringify(%s));" % expr], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    def test_changing_the_option_or_skipping_keeps_the_note_and_the_images(self):
        a = "0123456789abcdef01234567.png"
        prev = {"i": 1, "note": "n", "images": [a, "../x"]}
        for fields in ({"i": 0}, {"i": -1}, {"skip": True}):
            got = self.q_js("mkAns(%s,%s)" % (json.dumps(prev), json.dumps(fields)))
            self.assertEqual((got["note"], got["images"]), ("n", [a]), fields)            # junk names are not carried
        self.assertEqual(self.q_js("mkAns(null,{i:0})"), {"note": "", "images": [], "i": 0})
        self.assertEqual(self.q_js("mkAns({i:2,note:'x'},{skip:true})"), {"note": "x", "images": [], "skip": True})   # the old choice is not carried

    def list_js(self, expr):
        html = self.form_html()
        src = "".join(re.search(r"/\*%s-START\*/(.*)/\*%s-END\*/" % (k, k), html, re.S).group(1) for k in ("QIMG", "LIST"))
        r = subprocess.run(["node", "-e", src + "\nprocess.stdout.write(JSON.stringify(%s));" % expr], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    # lang: fr begin
    def test_list_card_record_follows_the_marked_lines(self):
        d = "{k:'list',lines:['One.','Two.','Three.']}"
        rec = self.list_js("listRecord(%s,{i:0,marks:[]})" % d)
        self.assertEqual(rec, {"choix": "OK", "recommande": True, "discuter": ""})
        rec = self.list_js("listRecord(%s,{i:0,marks:[1]})" % d)
        self.assertEqual(rec, {"choix": "À discuter (1)", "recommande": False, "discuter": "Two."})
        rec = self.list_js("listRecord(%s,{i:0,marks:[2,0,2,9]})" % d)
        self.assertEqual((rec["choix"], rec["discuter"]), ("À discuter (2)", "One.\nThree."))
        self.assertEqual(self.list_js("listLabel(%s,null)" % d), "OK")

    def test_list_card_reload_marks_the_same_lines_by_text(self):
        d = "{k:'list',lines:['One.','Two.','Three.']}"
        self.assertEqual(self.list_js("listRestore(%s,{discuter:'Three.\\nOne.'})" % d), [0, 2])
        self.assertEqual(self.list_js("listRestore(%s,{discuter:'Gone.'})" % d), [])
        got = self.list_js("fromRecord(%s,{statut:'tranche',choix:'À discuter (1)',discuter:'Two.',commentaire:'n'},[])" % d)
        self.assertEqual((got["i"], got["marks"], got["note"]), (0, [1], "n"))
        self.assertEqual(self.list_js("fromRecord(%s,{statut:'tranche',choix:'OK'},[]).marks" % d), [])
        # a lines list that was reordered since: the text still matches
        self.assertEqual(self.list_js("listRestore({lines:['Two.','One.']},{discuter:'Two.'})"), [0])
        self.assertEqual(self.list_js("fromRecord({o:[{t:'A'},{t:'B'}]},{statut:'tranche',choix:'B'},[]).i"), 1)
        self.assertIsNone(self.list_js("fromRecord({o:[{t:'A'}]},{statut:'tranche',choix:'Z'},[])"))

    def test_list_card_label_does_not_read_options(self):
        html = self.form_html()
        src = "".join(re.search(r"/\*%s-START\*/(.*)/\*%s-END\*/" % (k, k), html, re.S).group(1) for k in ("QIMG", "LIST"))
        fn = re.search(r"(function label\(d,a\).*)", html).group(1)
        r = subprocess.run(["node", "-e", src + "\n" + fn + "\nprocess.stdout.write(JSON.stringify([label({k:'list',lines:['a','b']},{i:0,marks:[1]}),label({k:'list',lines:['a']},{i:0,marks:[]})]));"],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(json.loads(r.stdout), ["À discuter (1)", "OK"])
    # lang: fr end

    # lang: fr begin
    def test_form_attach_filter_and_lines_follow_the_page(self):
        f = lambda t, size=100, name="a": {"name": name, "type": t, "size": size}
        out = self.q_js("qFilter(%s, 4)" % json.dumps([f("image/png", name="a.png"), f("image/svg+xml", name="x.svg"),
                                                       f("image/gif", 10 * 1024 * 1024 + 1, "big.gif"), f("image/webp", name="w.webp")]))
        self.assertEqual([x["name"] for x in out["accept"]], ["a.png", "w.webp"])            # no count cap
        self.assertEqual(len(out["lines"]), 2)                                               # 2 refusals, the advice is not a refusal line
        self.assertFalse(any("Plus de" in l for l in out["lines"]))
        self.assertEqual(self.q_js("qFilter([{name:'a',type:'image/png',size:1}], 9).lines"), [])
        adv = "⚠ Plus de 5 captures dans cette réponse"
        self.assertEqual([self.q_js("qAdvice(%d)" % n) for n in (0, 5, 6, 7)], ["", "", adv, adv])   # shown above 5, gone back at 5
        form = self.form_html()
        self.assertRegex(form, r'Captures \(facultatif\)</div><div class="qwarn" id="qadv"[^>]*hidden></div><div class="qstrip"')
        self.assertIn("relance-le", self.q_js("qUploadLine(404)"))
        self.assertIn("10 Mo", self.q_js("qUploadLine(413)"))
        ok = "0123456789abcdef01234567.png"
        self.assertEqual(self.q_js("qUrl('1-1','R1',%s)" % json.dumps(ok)), "/api/qcm/1-1/images/R1/" + ok)
        self.assertIsNone(self.q_js("qUrl('1-1','R1','\"><img src=x onerror=1>')"))
        got = self.q_js("qFromItems([{kind:'file',type:'image/png',getAsFile:()=>({t:1})},{kind:'file',type:'image/svg+xml',getAsFile:()=>({})},"
                        "{kind:'string',type:'text/plain'}])")
        self.assertEqual(got, [{"t": 1}])

    def test_form_never_rebuilds_an_answer_without_its_images_and_always_sends_them(self):
        html = self.form_html()
        self.assertNotIn("note:(ans[", html)
        self.assertNotRegex(html, r"ans\[d\.c\]=\{")                                         # every rebuild goes through mkAns
        self.assertNotIn("note:(a&&a.note)", html)
        self.assertIn("record(d,a),{images:qNames(a)}", html)                                 # sent even empty: absent = delete on the server
        self.assertIn("enqueue(()=>postRecord(d))", html)                                     # saves and uploads are serialized
        self.assertIn("enqueue(async()=>{", html)
        self.assertIn("await postRecord(d)", html)
        self.assertIn("Capture supprimée (limite de 512 Mo)", html)
        self.assertNotIn("innerHTML", re.search(r"/\*QIMG-START\*/(.*)/\*QIMG-END\*/", html, re.S).group(1))

    def test_form_artifact_mode_has_no_controls_listeners_or_image_in_the_record(self):
        html = self.form_html()
        m = re.search(r"const record=\(d,a\)=>\(\{(.*?)\}\);", html)
        self.assertNotIn("images", m.group(1))                                                # the stored record is unchanged
        self.assertIn("store.doc('decisions/'+d.c).set(record(d,a))", html)                  # online: record as it was
        self.assertIn("if(LOCAL)h+='<div><div class=\"lbl\">Captures", html)                  # controls only in local mode
        self.assertIn("else h+='<div class=\"mdim\">Les captures ne sont possibles que sur la page locale du suivi.</div>'", html)
        i = html.index("if(LOCAL){\n  document.addEventListener('paste'")
        self.assertEqual(html.count("addEventListener('paste'"), 1)
        self.assertEqual(html.count("addEventListener('drop'"), 1)
        self.assertLess(i, html.index("addEventListener('paste'"))
        self.assertIn("if(!LOCAL)return;\n  const pk=$('qpick')", html)
    # lang: fr end



FAKE_GLAB = r"""#!/usr/bin/env bash
# Fake glab: behaviour is driven by files in $FAKE_DIR (sha, pipelines, jobs, mode).
echo "$*" >> "$FAKE_DIR/calls"
case "$(cat "$FAKE_DIR/mode" 2>/dev/null)" in
    fail) exit 1 ;;
    sleep) sleep 30 ;;
    empty) exit 0 ;;
esac
case "$*" in
    *repository/commits*ref_name=*)
        ref="$(echo "$*" | sed 's/.*ref_name=\([^&]*\).*/\1/')"
        f="$FAKE_DIR/sha"; [ ! -f "$FAKE_DIR/sha-$ref" ] || f="$FAKE_DIR/sha-$ref"
        echo "[{\"id\": \"$(cat "$f")\"}]" ;;
    *pipelines*)
        sha="$(echo "$*" | sed 's/.*sha=\([0-9a-f]*\).*/\1/')"
        if [ -f "$FAKE_DIR/pipelines-$sha" ]; then cat "$FAKE_DIR/pipelines-$sha"; else cat "$FAKE_DIR/pipelines"; fi ;;
    *graphql*)
        iid="$(echo "$*" | sed 's/.*iid:"\([0-9]*\)".*/\1/')"
        if [ -f "$FAKE_DIR/jobs-$iid" ]; then cat "$FAKE_DIR/jobs-$iid"; else cat "$FAKE_DIR/jobs"; fi ;;
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


class TestCiLine(CiBase):
    def test_red_on_the_line_leaves_the_main_verdict_alone(self):
        nodes = [{"name": "github-actions/" + j, "status": "FAILED" if j == "fmt" else "SUCCESS",
                  "detailedStatus": {"label": "x"}} for j in suivi.CI_JOBS]
        self.put_file("jobs-6", json.dumps({"data": {"project": {"pipeline": {"stages": {"nodes": [
            {"name": "external", "jobs": {"nodes": nodes}}]}}}}}))
        self.put_file("sha-PostV1.1", SHA_B)
        self.put_file("pipelines-" + SHA_B, '[{"iid": 6}]')
        line = suivi.ci_poll(None, ref="PostV1.1")
        main = suivi.ci_poll(None)
        self.assertEqual((line["verdict"], line["sha"], line["names"]), ("red", SHA_B, ["fmt"]))
        self.assertEqual((main["verdict"], main["sha"]), ("green", SHA_A))
        self.assertIsNone(main["last_red"])


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


FAKE_GLAB_LABELS = r"""#!/usr/bin/env python3
# Fake glab for the GitLab labels and tags (#467): state in $FAKE_DIR/state.json, every call logged in
# $FAKE_DIR/calls, every PUT/POST parsed into $FAKE_DIR/writes.jsonl and applied to the state.
import json, os, sys, time, urllib.parse
d = os.environ["FAKE_DIR"]
args = sys.argv[1:]
open(d + "/calls", "a").write(" ".join(args) + "\n")
mode = open(d + "/mode").read().strip() if os.path.exists(d + "/mode") else ""
method, fields, path = "GET", {}, None
i = 1
while i < len(args):
    if args[i] == "--method":
        method = args[i + 1]; i += 2
    elif args[i] == "-f":
        k, v = args[i + 1].split("=", 1); fields[k] = v; i += 2
    else:
        path = args[i]; i += 1
if mode == "sleep":
    time.sleep(30)
if mode == "empty" and method == "GET":
    sys.exit(0)
if mode == "garbage" and method == "GET":
    print("<html>"); sys.exit(0)
if mode == "dict" and method == "GET":
    print("{}"); sys.exit(0)
if mode == "fail" or (mode == "failwrite" and method != "GET"):
    sys.exit(1)
st = json.load(open(d + "/state.json"))
base = "projects/86104040/"
rel, _, query = path[len(base):].partition("?")
q = urllib.parse.parse_qs(query)
if method != "GET":
    n = int(rel.split("/")[1])
    iss = st["issues"][str(n)]
    if rel.endswith("/notes"):
        iss.setdefault("notes", []).append(fields["body"])
    else:
        labs = set(iss["labels"])
        labs |= set(filter(None, fields.get("add_labels", "").split(",")))
        labs -= set(filter(None, fields.get("remove_labels", "").split(",")))
        iss["labels"] = sorted(labs)
        if fields.get("state_event") == "close":
            iss["state"] = "closed"
    open(d + "/writes.jsonl", "a").write(json.dumps({"method": method, "issue": n, "fields": fields, "path": rel}) + "\n")
    json.dump(st, open(d + "/state.json", "w"))
    print("{}")
    sys.exit(0)
def out(x):
    print(json.dumps(x))
if rel == "issues":
    want = [s_ for s_ in q.get("state", ["opened"])]
    rows = []
    for k, v in st["issues"].items():
        if want != ["all"] and v["state"] not in want:
            continue
        if "labels" in q and q["labels"][0] not in v["labels"]:
            continue
        if "iids[]" in q and k not in q["iids[]"]:
            continue
        rows.append({"iid": int(k), "state": v["state"], "labels": v["labels"]})
    out(rows)
elif rel == "repository/tags":
    out(st["tags"])
elif rel.startswith("repository/tags/"):
    out(next(t for t in st["tags"] if t["name"] == rel.split("/")[-1]))
elif rel == "pipelines":
    out(st.get("pipelines", []))
elif rel.startswith("pipelines/") and rel.endswith("/jobs"):
    out(st.get("pipeline_jobs", []))
else:
    out([])
"""
NOW = 1_000_000.0
TAG_SHA = "c" * 40


class GithubStandIn:
    """A local stand-in for the GitHub Actions API: runs and jobs set by the test."""
    def __init__(self):
        self.runs = []                 # [{id, head_sha, created_at, name}]
        self.jobs = {}                 # run id -> [job]
        self.status = 200
        self.hits = []
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_GET(self):
                outer.hits.append(self.path)
                if outer.status != 200:
                    self.send_response(outer.status); self.end_headers(); return
                if "/jobs" in self.path:
                    rid = int(self.path.split("/runs/")[1].split("/")[0])
                    body = {"jobs": outer.jobs.get(rid, [])}
                else:
                    body = {"workflow_runs": outer.runs}
                data = json.dumps(body).encode()
                self.send_response(200); self.send_header("Content-Length", str(len(data))); self.end_headers()
                self.wfile.write(data)
        self.srv = http.server.HTTPServer(("127.0.0.1", 0), H)
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()

    @property
    def url(self):
        return "http://127.0.0.1:%d" % self.srv.server_address[1]

    def close(self):
        self.srv.shutdown(); self.srv.server_close()


def gh_job(name, conclusion="success", status="completed", steps=()):
    return {"name": name, "status": status, "conclusion": conclusion,
            "steps": [{"name": "s", "conclusion": c} for c in steps]}


class LabelBase(Base):
    """The GitLab labels pass with a fake glab on PATH; the real GitLab is never reached."""
    def setUp(self):
        super().setUp()
        self.fake = os.path.join(self.tmp.name, "fake")
        binp = os.path.join(self.tmp.name, "bin")
        os.makedirs(self.fake)
        os.makedirs(binp)
        with open(os.path.join(binp, "glab"), "w") as f:
            f.write(FAKE_GLAB_LABELS)
        os.chmod(os.path.join(binp, "glab"), 0o755)
        patcher = mock.patch.dict(os.environ, {"PATH": binp + os.pathsep + os.environ["PATH"], "FAKE_DIR": self.fake,
                                               "SF_SUIVI_DIR": self.state, "SF_TODAY": "2026-11-03"})
        patcher.start()
        self.addCleanup(patcher.stop)
        for k in ("SF_SUIVI_LABELS_SETTLE", "SF_SUIVI_LABELS_TIMEOUT", "SF_SUIVI_TAGJOBS_API", "SF_SUIVI_TAG_CAP"):
            os.environ.pop(k, None)
        self.fstate = {"issues": {}, "tags": [], "pipelines": [], "pipeline_jobs": []}
        self.save_fake()
        self.gh = GithubStandIn()
        self.addCleanup(self.gh.close)
        os.environ["SF_SUIVI_TAGJOBS_API"] = self.gh.url
        os.environ["SF_SUIVI_TAGJOBS_POLL"] = "0"

    def save_fake(self):
        with open(os.path.join(self.fake, "state.json"), "w") as f:
            json.dump(self.fstate, f)

    def load_fake(self):
        with open(os.path.join(self.fake, "state.json")) as f:
            self.fstate = json.load(f)
        return self.fstate

    def issue(self, n, labels=(), state="opened"):
        self.fstate["issues"][str(n)] = {"state": state, "labels": list(labels)}
        self.save_fake()

    def labels_of(self, n):
        return sorted(self.load_fake()["issues"][str(n)]["labels"])

    def mode(self, m):
        with open(os.path.join(self.fake, "mode"), "w") as f:
            f.write(m)

    def writes(self):
        p = os.path.join(self.fake, "writes.jsonl")
        if not os.path.exists(p):
            return []
        return [json.loads(l) for l in slurp(p).splitlines() if l.strip()]

    def puts(self):
        return [w for w in self.writes() if w["method"] == "PUT"]

    def calls(self):
        p = os.path.join(self.fake, "calls")
        return slurp(p).splitlines() if os.path.exists(p) else []

    def held(self, n, **extra):
        self.put(n, colour=C[0], owner=self.live_owner(), **extra)

    def paused(self, n, **extra):
        self.put(n, colour=None, last_colour=C[0], **extra)

    def run_pass(self, prev=None, now=NOW, dry=False):
        return suivi.label_pass(prev, now, dry)[0]

    def tag(self, name, sha=TAG_SHA):
        self.fstate["tags"].insert(0, {"name": name, "commit": {"id": sha}})
        self.save_fake()

    def gh_run(self, jobs, sha=TAG_SHA, run_id=1, created="2026-11-05T10:00:00Z"):
        self.gh.runs.append({"id": run_id, "head_sha": sha, "created_at": created, "name": "CI"})
        self.gh.jobs[run_id] = jobs

    def declare(self, n, kind, jobs=None, human=""):
        suivi.write_json(suivi.tagwait_path(n), {"issue": n, "kind": kind, "jobs": jobs or [], "human": human, "declared_at": 1})


class TestLabelWanted(Base):
    def setUp(self):
        super().setUp()
        patcher = mock.patch.dict(os.environ, {"SF_SUIVI_DIR": self.state})
        patcher.start()
        self.addCleanup(patcher.stop)

    def dead_owner(self):
        return {"session": "S9", "pid": 2 ** 22 + 12345, "pid_start": 1, "at": time.time()}

    def test_every_hold_reason_is_en_cours_and_the_rest_en_pause(self):
        self.put(1, colour=C[0], owner=self.live_owner())                    # agent
        self.put(2, colour=C[1])                                              # open form, no agent
        os.makedirs(os.path.join(self.state, "qcm", "2-1"))
        with open(os.path.join(self.state, "qcm", "2-1", "meta.json"), "w") as f:
            json.dump({"issue": 2, "finished": False}, f)
        self.put(3, colour=C[2])                                              # trial to tick, no agent
        os.makedirs(os.path.join(self.state, "tests"))
        with open(os.path.join(self.state, "tests", "3-1.json"), "w") as f:
            json.dump({"n": 3, "status": "todo", "try": "scripts/try.sh 3"}, f)
        self.put(4, colour=C[3], legacy=True)                                 # migrated record
        self.put(5, colour=None, last_colour=C[4])                            # holds nothing
        self.put(6, colour=C[5], done_at="2026-11-01")                        # finished
        self.put(7, colour=C[4], owner=self.dead_owner())                     # its session died
        want = suivi.label_wanted(suivi.load_all())
        self.assertEqual(want, {1: "en-cours", 2: "en-cours", 3: "en-cours", 4: "en-cours", 5: "en-pause", 7: "en-pause"})

    def test_package_parent_follows_its_subs_and_unstarted_subs_have_none(self):
        self.put(10, kind="package", colour=C[0], subs=[{"n": 11, "title": "a"}, {"n": 12, "title": "b"}, {"n": 13, "title": "c"}])
        self.put(11, parent=10, colour=C[0], owner=self.live_owner())
        self.put(12, parent=10, colour=C[0])
        self.assertEqual(suivi.label_wanted(suivi.load_all()), {10: "en-cours", 11: "en-cours", 12: "en-pause"})
        # no sub held any more: the parent pauses with them; sub 13 never had a record
        self.put(11, parent=10, colour=C[0])
        self.assertEqual(suivi.label_wanted(suivi.load_all()), {10: "en-pause", 11: "en-pause", 12: "en-pause"})

    def test_reads_records_only_never_git(self):
        self.put(1, colour=C[0], owner=self.live_owner())
        empty = os.path.join(self.tmp.name, "nobin")
        os.makedirs(empty)
        code = ("import importlib.util, sys\n"
                "s = importlib.util.spec_from_file_location('suivi', %r); m = importlib.util.module_from_spec(s)\n"
                "s.loader.exec_module(m)\n"
                "print(m.label_wanted(m.load_all()))\n" % SUIVI)
        r = subprocess.run([sys.executable, "-I", "-c", code], env={"PATH": empty, "SF_SUIVI_DIR": self.state},
                           capture_output=True, text=True)
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "{1: 'en-cours'}"), r.stderr)


class TestLabelPass(LabelBase):
    def test_one_put_per_changed_issue_with_add_and_remove_and_nothing_more(self):
        self.held(1)
        self.paused(2)
        self.issue(1, ["évolution", "en-pause"])
        self.issue(2, ["bug", "en-pause"])           # already right: no PUT
        st = self.run_pass()
        self.assertTrue(st["ok"], st)
        puts = self.puts()
        self.assertEqual([(p["issue"], p["fields"]) for p in puts],
                         [(1, {"add_labels": "en-cours", "remove_labels": "en-pause"})])
        self.assertEqual(self.labels_of(1), ["en-cours", "évolution"])
        self.assertNotIn("labels", puts[0]["fields"])             # `labels=` would replace the user's labels
        self.assertEqual(self.run_pass()["ok"], True)
        self.assertEqual(len(self.puts()), 1, "the second pass wrote again")

    def test_closed_issue_loses_labels_and_is_not_readded_until_reopened(self):
        self.held(1)
        self.held(2)
        self.issue(1, ["en-cours"], state="closed")
        self.issue(2, ["attente-rc"], state="closed")      # a waiting label goes at closure too
        self.issue(3, ["en-pause", "attente-final"], state="closed")   # no record at all
        self.run_pass()
        self.assertEqual((self.labels_of(1), self.labels_of(2), self.labels_of(3)), ([], [], []))
        n = len(self.puts())
        self.run_pass()
        self.assertEqual(len(self.puts()), n, "a closed issue with a live record was labelled again")
        self.fstate["issues"]["1"]["state"] = "opened"
        self.save_fake()
        self.run_pass()
        self.assertEqual(self.labels_of(1), ["en-cours"])

    def test_the_line_label_is_the_users_and_is_never_removed(self):
        # ligne-PostV1.1 (#480) declares an issue's line; the label pass must keep it, even at closure.
        self.assertNotIn("ligne-PostV1.1", suivi.LABEL_ALL)
        self.held(1)
        self.issue(1, ["en-pause", "ligne-PostV1.1"], state="closed")
        self.issue(2, ["ligne-PostV1.1"], state="opened")
        self.paused(2)
        self.run_pass()
        self.assertEqual(self.labels_of(1), ["ligne-PostV1.1"])
        self.assertEqual(self.labels_of(2), ["en-pause", "ligne-PostV1.1"])

    def test_issue_without_label_is_found_by_number_not_by_label(self):
        self.held(5)
        self.issue(5, [])
        self.run_pass()
        self.assertEqual(self.labels_of(5), ["en-cours"])

    def test_failed_timed_out_or_empty_read_writes_nothing(self):
        self.held(1)
        self.issue(1, ["en-pause"])
        os.environ["SF_SUIVI_LABELS_TIMEOUT"] = "1"
        for m in ("fail", "empty", "garbage", "dict", "sleep"):
            self.mode(m)
            st = self.run_pass(now=NOW)
            self.assertFalse(st["ok"], m)
            self.assertTrue(st["reason"], m)
            self.assertEqual(self.writes(), [], "mode %s wrote" % m)
            self.assertEqual(self.labels_of(1), ["en-pause"], m)
        self.mode("")
        self.assertTrue(self.run_pass()["ok"])
        self.assertEqual(self.labels_of(1), ["en-cours"])

    def test_failed_write_is_reported_and_retried(self):
        self.held(1)
        self.issue(1, [])
        self.mode("failwrite")
        st = self.run_pass()
        self.assertFalse(st["ok"])
        self.assertIsNone(st["synced_at"])
        self.mode("")
        self.assertTrue(self.run_pass()["ok"])
        self.assertEqual(self.labels_of(1), ["en-cours"])

    def test_demotion_waits_ten_minutes_promotion_and_done_are_immediate(self):
        self.paused(1)
        self.issue(1, ["en-cours"])
        st = self.run_pass(now=NOW)
        self.assertEqual(self.puts(), [])
        st = self.run_pass(st, now=NOW + 599)
        self.assertEqual(self.puts(), [])
        st = self.run_pass(st, now=NOW + 601)
        self.assertEqual(self.labels_of(1), ["en-pause"])
        # promotion: at once
        self.held(2)
        self.issue(2, ["en-pause"])
        self.run_pass(st, now=NOW + 700)
        self.assertEqual(self.labels_of(2), ["en-cours"])
        # done: at once
        self.put(3, colour=None, done_at="2026-11-02")
        self.issue(3, ["en-cours"])
        self.run_pass(st, now=NOW + 700)
        self.assertEqual(self.labels_of(3), [])

    def test_a_flap_restarts_the_delay(self):
        self.paused(1)
        self.issue(1, ["en-cours"])
        st = self.run_pass(now=NOW)
        self.held(1)                                   # /clear then resume: held again
        st = self.run_pass(st, now=NOW + 300)
        self.assertEqual(st["demote"], {})
        self.paused(1)
        st = self.run_pass(st, now=NOW + 400)
        st = self.run_pass(st, now=NOW + 700)          # 300 s after the new start, 700 after the first
        self.assertEqual(self.puts(), [])
        self.run_pass(st, now=NOW + 1001)
        self.assertEqual(self.labels_of(1), ["en-pause"])

    def test_settle_seam(self):
        os.environ["SF_SUIVI_LABELS_SETTLE"] = "0"
        self.paused(1)
        self.issue(1, ["en-cours"])
        self.run_pass()
        self.assertEqual(self.labels_of(1), ["en-pause"])

    def test_waiting_label_hides_en_pause_and_keeps_en_cours(self):
        self.paused(1)
        self.paused(2)
        self.held(3)
        self.issue(1, ["attente-final"])
        self.issue(2, ["en-pause", "attente-rc"])
        self.issue(3, ["attente-rc"])
        self.run_pass()
        self.assertEqual(self.labels_of(1), ["attente-final"])
        self.assertEqual(self.labels_of(2), ["attente-rc"])
        self.assertEqual(self.labels_of(3), ["attente-rc", "en-cours"])

    def test_tracking_state_wins_over_a_hand_edit(self):
        self.held(1)
        self.issue(1, ["en-pause", "en-cours"])        # both by hand
        self.run_pass()
        self.assertEqual(self.labels_of(1), ["en-cours"])

    def test_declared_wait_adds_its_label_once(self):
        self.paused(1)
        self.declare(1, "final")
        self.issue(1, [])
        self.run_pass()
        self.assertEqual(self.labels_of(1), ["attente-final"])      # and no en-pause: hidden
        self.assertTrue(jload(suivi.tagwait_path(1)).get("added_at"))
        self.fstate["issues"]["1"]["labels"] = []                   # removed by hand
        self.save_fake()
        self.run_pass()
        self.assertNotIn("attente-final", self.labels_of(1), "the label was added twice")

    def test_dry_run_prints_and_writes_nothing(self):
        self.held(1)
        self.issue(1, [])
        self.declare(1, "rc")
        r = self.run_cli("labels", "sync", "--dry-run", env={"PATH": os.environ["PATH"], "FAKE_DIR": self.fake,
                                                             "SF_SUIVI_DIR": self.state})
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("PUT #1 add=attente-rc,en-cours", r.stdout)
        self.assertEqual(self.writes(), [])
        self.assertFalse(os.path.exists(os.path.join(self.state, "labels.json")))
        self.assertFalse(jload(suivi.tagwait_path(1)).get("added_at"))


class TestLabelGate(LabelBase):
    def serve(self, *extra, env=None, poll="0.2"):
        e = dict(self.env, PATH=os.environ["PATH"], FAKE_DIR=self.fake, SF_SUIVI_LABELS_POLL=poll)
        e.update(env or {})
        e.pop("SF_SUIVI_DIR", None) if "--state" in extra else None
        proc = subprocess.Popen([sys.executable, "-I", SUIVI, "serve", "--port", "0", *extra],
                                cwd=self.repo, env=e, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(lambda: (proc.terminate(), proc.wait(5), proc.stdout.close(), proc.stderr.close()))
        m = re.search(r":(\d+)/", proc.stdout.readline())
        return int(m.group(1))

    def issues_called(self):
        return [c for c in self.calls() if "issues" in c]

    def test_state_folder_override_never_reaches_gitlab(self):
        os.makedirs(self.state, exist_ok=True)
        self.serve("--state", self.state)
        self.serve(env={"SF_SUIVI_DIR": self.state})
        time.sleep(1.5)
        self.assertEqual(self.calls(), [])

    def test_seam_on_runs_the_loop_and_off_wins(self):
        os.makedirs(self.state, exist_ok=True)
        self.held(1)
        self.issue(1, [])
        self.serve("--state", self.state, env={"SF_SUIVI_LABELS": "on", "SF_SUIVI_DIR": self.state})
        for _ in range(40):
            if self.puts():
                break
            time.sleep(0.2)
        self.assertEqual(self.labels_of(1), ["en-cours"])

    def test_off_seam(self):
        os.makedirs(self.state, exist_ok=True)
        self.serve("--state", self.state, env={"SF_SUIVI_LABELS": "off"})
        time.sleep(1)
        self.assertEqual(self.calls(), [])

    def test_labels_enabled_is_off_by_default_for_a_test_folder(self):
        os.environ.pop("SF_SUIVI_LABELS", None)
        self.assertFalse(suivi.labels_enabled(False))
        self.assertTrue(suivi.labels_enabled(True))
        with mock.patch.dict(os.environ, {"SF_SUIVI_LABELS": "on"}):
            self.assertTrue(suivi.labels_enabled(False))
        with mock.patch.dict(os.environ, {"SF_SUIVI_LABELS": "off"}):
            self.assertFalse(suivi.labels_enabled(True))

    def test_state_etag_does_not_move_with_the_loop(self):
        os.makedirs(self.state, exist_ok=True)
        self.held(1)
        self.issue(1, [])
        port = self.serve("--state", self.state, env={"SF_SUIVI_LABELS": "on", "SF_SUIVI_DIR": self.state})

        def etag():
            c = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
            c.request("GET", "/api/state")
            r = c.getresponse()
            r.read()
            return r.getheader("ETag")
        first = etag()
        time.sleep(1.2)
        self.assertTrue(first)
        self.assertEqual(etag(), first)


class TestTagReport(LabelBase):
    def seed(self, *names):
        for t in names:
            self.tag(t, sha="a" * 40)
        suivi.tag_pass(NOW)
        self.assertEqual(self.writes(), [])

    def test_first_run_only_seeds(self):
        self.tag("v1.0.1")
        self.issue(1, ["attente-final"])
        self.gh_run([gh_job("release-linux")], sha=TAG_SHA)
        lines = suivi.tag_pass(NOW)
        d = jload(suivi.tags_path())
        self.assertEqual((d["seen"], self.writes()), (["v1.0.1"], []))
        self.assertFalse([c for c in self.calls() if "issues" in c], lines)
        self.assertEqual(self.gh.hits, [])

    def test_rc_tag_treats_attente_rc_only_final_treats_both_and_restart_is_quiet(self):
        self.seed("v1.0.0")
        self.issue(1, ["attente-rc"])
        self.issue(2, ["attente-final"])
        self.gh_run([gh_job("release-linux"), gh_job("test-linux-packages (fedora:42, rpm)")])
        self.tag("v1.1.0-rc1")
        suivi.tag_pass(NOW)
        posts = [w for w in self.writes() if w["path"].endswith("/notes")]
        self.assertEqual([w["issue"] for w in posts], [1])
        self.assertFalse([c for c in self.calls() if "labels=attente-final" in c], "an rc tag read attente-final")
        suivi.tag_pass(NOW + 5000)                                   # restart: same data
        self.assertEqual(len([w for w in self.writes() if w["path"].endswith("/notes")]), 1)
        d = jload(suivi.tags_path())                                 # a crash after the comment, before `seen`
        d["seen"].remove("v1.1.0-rc1")
        suivi.write_json(suivi.tags_path(), d)
        suivi.tag_pass(NOW + 5500)
        self.assertEqual(len([w for w in self.writes() if w["path"].endswith("/notes")]), 1)
        self.gh.runs.clear()
        self.gh_run([gh_job("release-linux")], sha="d" * 40)
        self.tag("v1.1.0", sha="d" * 40)
        suivi.tag_pass(NOW + 6000)
        posts = [(w["issue"]) for w in self.writes() if w["path"].endswith("/notes")]
        self.assertEqual(sorted(posts), [1, 1, 2])                   # a later tag, a new comment
        suivi.tag_pass(NOW + 9000)
        self.assertEqual(len([w for w in self.writes() if w["path"].endswith("/notes")]), 3)

    def test_runs_of_another_sha_are_ignored(self):
        self.seed("v1.0.0")
        self.issue(1, ["attente-rc"])
        self.declare(1, "rc", jobs=["release-linux"])
        self.gh_run([gh_job("release-linux", "success")], sha="e" * 40, run_id=7, created="2026-11-05T12:00:00Z")
        self.gh_run([gh_job("release-linux", "failure")], sha=TAG_SHA, run_id=8, created="2026-11-05T09:00:00Z")
        self.tag("v1.0.1-rc1")
        suivi.tag_pass(NOW)
        note = self.load_fake()["issues"]["1"]["notes"][0]
        self.assertIn("`release-linux`: failed", note)

    def test_verdicts(self):
        v = suivi.job_verdict
        self.assertEqual(v("a", []), "missing")
        self.assertEqual(v("a", [gh_job("b")]), "missing")
        self.assertEqual(v("a", [gh_job("a", None, "in_progress")]), "pending")
        self.assertEqual(v("a", [gh_job("a", "skipped")]), "skipped")
        self.assertEqual(v("a", [gh_job("a", "cancelled")]), "failed")
        self.assertEqual(v("a", [gh_job("a")]), "ok")
        legs = [gh_job("a (x, rpm)"), gh_job("a (y, deb)"), gh_job("a (z, deb)")]
        self.assertEqual(v("a", legs), "ok")
        self.assertEqual(v("a", legs[:2] + [gh_job("a (z, deb)", "failure")]), "failed")
        self.assertEqual(v("a", legs[:2] + [gh_job("a (z, deb)", None, "queued")]), "pending")
        self.assertEqual(v("a", [gh_job("ab")]), "missing")               # a prefix of another name is not a match
        # a `continue-on-error` job reads green although its step failed
        self.assertEqual(v("a", [gh_job("a", "success", steps=("success", "failure"))]), "failed")

    def test_unreadable_or_running_github_defers_without_any_comment(self):
        self.seed("v1.0.0")
        self.issue(1, ["attente-rc"])
        self.declare(1, "rc", jobs=["release-linux"])
        self.tag("v1.0.1-rc1")
        self.gh.status = 403
        suivi.tag_pass(NOW)
        self.assertEqual(self.writes(), [])
        self.gh.status = 200
        self.gh_run([gh_job("release-linux", None, "in_progress")])
        suivi.tag_pass(NOW + 1)
        self.assertEqual(self.writes(), [])
        self.assertNotIn("v1.0.1-rc1", jload(suivi.tags_path())["seen"])
        self.gh.jobs[1] = [gh_job("release-linux")]                   # finished: now it is reported
        suivi.tag_pass(NOW + 2)
        self.assertEqual(len(self.writes()), 1)
        self.assertIn("v1.0.1-rc1", jload(suivi.tags_path())["seen"])

    def test_a_declared_issue_with_every_job_ok_gets_a_card_that_closes_it(self):
        self.seed("v1.0.0")
        self.paused(1)
        self.issue(1, ["attente-final"])
        self.declare(1, "final", jobs=["release-linux", "pages"], human="Ouvre le manuel en ligne.")
        self.gh_run([gh_job("release-linux")])
        self.fstate["pipelines"] = [{"id": 9, "sha": TAG_SHA, "source": "push", "status": "success"}]
        self.fstate["pipeline_jobs"] = [{"name": "pages", "status": "success"}]
        self.tag("v1.1.0")
        suivi.tag_pass(NOW)
        card = jload(os.path.join(self.state, "tests", "1-1.json"))
        self.assertEqual((card["status"], card["n"]), ("todo", 1))
        self.assertNotIn("try", card)
        suivi.check_entry({k: v for k, v in card.items() if k not in ("n", "status")})
        self.assertIn("Ouvre le manuel en ligne.", card["steps"])
        self.assertFalse(suivi.trial_pending(1), "the card must not hold a colour")
        self.assertEqual(suivi.build_state()["pending"], 1)
        suivi.tag_pass(NOW + 1)                                        # untouched card: nothing closes
        self.assertEqual(self.puts(), [])
        card["status"] = "retour"
        suivi.write_json(os.path.join(self.state, "tests", "1-1.json"), card)
        suivi.tag_pass(NOW + 2)
        self.assertEqual(self.puts(), [])                              # Retour keeps it open
        card["status"] = "ok"
        suivi.write_json(os.path.join(self.state, "tests", "1-1.json"), card)
        suivi.tag_pass(NOW + 3)
        suivi.tag_pass(NOW + 4)
        puts = self.puts()
        self.assertEqual([(p["issue"], p["fields"]) for p in puts],
                         [(1, {"remove_labels": "attente-final,attente-rc", "state_event": "close"})])
        self.assertEqual(self.load_fake()["issues"]["1"]["state"], "closed")

    def test_settling_a_tag_card_does_not_close_the_issue(self):
        self.seed("v1.0.0")
        self.paused(1)
        self.issue(1, ["attente-final"])
        self.declare(1, "final", jobs=["release-linux"], human="Ouvre le manuel en ligne.")
        self.gh_run([gh_job("release-linux")])
        self.tag("v1.1.0")
        suivi.tag_pass(NOW)
        path = os.path.join(self.state, "tests", "1-1.json")
        card = jload(path)
        card["status"] = "retour"
        suivi.write_json(path, card)
        r = self.run_cli("test", "settle", "1-1")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(jload(path)["status"], "retour")
        suivi.tag_pass(NOW + 2)
        suivi.tag_pass(NOW + 3)
        self.assertEqual(self.puts(), [])

    def test_a_failed_job_comments_keeps_the_label_and_makes_no_card(self):
        self.seed("v1.0.0")
        self.issue(1, ["attente-rc"])
        self.declare(1, "rc", jobs=["release-linux", "test-linux-packages"])
        self.gh_run([gh_job("release-linux"), gh_job("test-linux-packages (fedora:42, rpm)", "failure")])
        self.tag("v1.0.1-rc1")
        suivi.tag_pass(NOW)
        self.assertEqual(len(self.writes()), 1)
        self.assertIn("failed", self.load_fake()["issues"]["1"]["notes"][0])
        self.assertEqual(self.labels_of(1), ["attente-rc"])
        self.assertFalse(os.path.exists(os.path.join(self.state, "tests")))

    def test_a_missing_job_is_no_proof_so_no_card(self):
        self.seed("v1.0.0")
        self.issue(1, ["attente-rc"])
        self.declare(1, "rc", jobs=["test-linux-packages"])
        self.gh_run([gh_job("release-linux")])
        self.tag("v1.0.1-rc1")
        suivi.tag_pass(NOW)
        self.assertIn("missing (no proof)", self.load_fake()["issues"]["1"]["notes"][0])
        self.assertFalse(os.path.exists(os.path.join(self.state, "tests")))

    def test_hand_placed_label_reports_all_release_jobs_with_a_generic_card(self):
        self.seed("v1.0.0")
        self.issue(1, ["attente-rc"])
        self.gh_run([gh_job(j) for j in suivi.RELEASE_JOBS])
        self.tag("v1.0.1-rc1")
        suivi.tag_pass(NOW)
        note = self.load_fake()["issues"]["1"]["notes"][0]
        for j in suivi.RELEASE_JOBS:
            self.assertIn("`%s`: ok" % j, note)
        card = jload(os.path.join(self.state, "tests", "1-1.json"))
        self.assertIn("Relis l'issue #1", card["steps"][0])

    def test_tag_check_dry_run_reads_and_never_writes(self):
        self.tag("v1.0.1", sha=TAG_SHA)
        self.issue(1, ["attente-rc"])
        self.declare(1, "rc", jobs=["release-linux"])
        self.gh_run([gh_job("release-linux")])
        env = {"PATH": os.environ["PATH"], "FAKE_DIR": self.fake, "SF_SUIVI_DIR": self.state,
               "SF_SUIVI_TAGJOBS_API": self.gh.url}
        r = self.run_cli("tag-check", "v1.0.1", "--dry-run", env=env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("#1: comment on v1.0.1: release-linux=ok", r.stdout)
        self.assertEqual(self.writes(), [])
        self.assertFalse(os.path.exists(os.path.join(self.state, "tags.json")))
        self.assertNotEqual(self.run_cli("tag-check", "v1.0.1", env=env).returncode, 0)   # --dry-run is required

    def test_status_shows_the_sync_state(self):
        suivi.write_json(suivi.labels_path(), {"checked_at": time.time(), "ok": False, "reason": "glab failed (exit 1)",
                                                "waiting": [{"issue": 345, "label": ["attente-final"]}]})
        r = self.run_cli("status", env={"SF_SUIVI_DIR": self.state})
        self.assertIn("FAILED (glab failed (exit 1))", r.stdout)
        self.assertIn("#345 waits (attente-final)", r.stdout)

    def test_tag_wait_command_writes_the_declaration(self):
        r = self.run_cli("tag-wait", "389", "rc", "--jobs", "a,b", "--human", "Vérifie.", env={"SF_SUIVI_DIR": self.state})
        self.assertEqual(r.returncode, 0, r.stderr)
        d = jload(suivi.tagwait_path(389))
        self.assertEqual((d["kind"], d["jobs"], d["human"]), ("rc", ["a", "b"], "Vérifie."))
        self.assertNotEqual(self.run_cli("tag-wait", "389", "soon", env={"SF_SUIVI_DIR": self.state}).returncode, 0)


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

    def line_row(self, name, row):
        src = re.search(r"/\*LINEROW-START\*/(.*)/\*LINEROW-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(lineRow(%s,%s)));" % (json.dumps(name), json.dumps(row))
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_line_row_shows_its_ci_and_its_issues_apart_from_main(self):
        # lang: fr begin
        red = self.line_row("PostV1.1", {"verdict": "red", "names": ["fmt"], "issues": [{"issue": 7, "delivered": True}, {"issue": 8, "delivered": False}]})
        self.assertEqual(red["cls"], "red")
        self.assertEqual(red["text"], "PostV1.1 : CI rouge : fmt — #7 livrée · #8 en cours")
        self.assertNotEqual(self.line_row("PostV1.1", {"verdict": "weird"})["cls"], "green")
        self.assertNotIn("verte", self.line_row("PostV1.1", {"verdict": "unknown"})["text"])
        self.assertIn("aucune issue", self.line_row("PostV1.1", {"verdict": "green"})["text"])
        # lang: fr end
        self.assertIsNone(self.line_row("PostV1.1", None))
        html = self.page()
        self.assertIn('id="lines"', html)

    def merge_label(self, c):
        src = re.search(r"/\*CARDLINE-START\*/(.*)/\*CARDLINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(mergeLabel(%s)));" % json.dumps(c)
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_merge_button_names_the_line_of_the_issue(self):
        # lang: fr begin
        self.assertEqual(self.merge_label({"issue": 7, "line": "PostV1.1"}), "Fusionner dans PostV1.1")
        self.assertEqual(self.merge_label({"issue": 7}), "Fusionner")
        self.assertEqual(self.merge_label({"issue": 7, "line": "main"}), "Fusionner")
        # lang: fr end
        self.assertIn("mergeLabel(c)", self.page())

    def test_ci_line_sits_above_the_tabs_outside_the_banner(self):
        html = self.page()
        self.assertLess(html.index('id="ciline"'), html.index('class="tabs"'))
        self.assertNotIn("'ciline'", html.split("function renderTests")[1].split("function render()")[0])

    def build_line(self, bq, now=1000.0):
        src = re.search(r"/\*BUILDLINE-START\*/(.*)/\*BUILDLINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(buildLine(%s,%s)));" % (json.dumps(bq), now)
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_build_line(self):
        cq = {"full": {"slots": 2, "cpu_weight": 100, "cpu_quota_pct": None, "cpus": None, "jobs": None},
              "dev": {"slots": 2, "cpu_weight": 20, "cpu_quota_pct": None, "cpus": None, "jobs": 11},
              "background": {"slots": 1, "cpu_weight": 10, "cpu_quota_pct": None, "cpus": None, "jobs": 6}}
        bq = {"installed": True, "stale": False, "selected": "background", "effective": "background", "auto": False,
              "why": "selected", "modes": cq, "running": [], "waiting": []}
        # lang: fr begin
        l = self.build_line(bq)
        self.assertEqual([(m["label"], m["on"]) for m in l["modes"]],
                         [("FullIA", False), ("Développement", False), ("Arrière-plan", True)])
        self.assertEqual(l["desc"], "1 build à la fois, 6 cœurs, priorité très basse")
        self.assertEqual(l["now"], "Aucun build en cours.")
        self.assertEqual(l["auto_note"], "")
        self.assertEqual(self.build_line(dict(bq, effective="full"))["desc"], "2 builds à la fois, tous les cœurs, priorité normale")
        linked = dict(cq, full=dict(cq["full"], profile="performance"))
        self.assertEqual(self.build_line(dict(bq, modes=linked, effective="full"))["desc"],
                         "2 builds à la fois, tous les cœurs, priorité normale, profil performance sur secteur")
        self.assertEqual(self.build_line(dict(bq, modes=linked))["desc"], "1 build à la fois, 6 cœurs, priorité très basse")
        self.assertEqual(self.build_line(bq)["profile_note"], "")
        self.assertEqual(self.build_line(dict(bq, profile_note="write-failed"))["profile_note"],
                         "Profil d’énergie refusé : laissé tel quel.")
        self.assertIn("non rétabli", self.build_line(dict(bq, profile_note="restore-failed"))["profile_note"])
        self.assertEqual(self.build_line(dict(bq, profile_note="made-up"))["profile_note"], "")
        self.assertEqual(self.build_line(dict(bq, effective="dev"))["desc"], "2 builds à la fois, 11 cœurs chacun, priorité basse")
        capped = dict(cq, background=dict(cq["background"], cpu_quota_pct=50))
        self.assertEqual(self.build_line(dict(bq, modes=capped))["desc"], "1 build à la fois, 6 cœurs, priorité très basse, plafond 50 %")
        a = self.build_line(dict(bq, auto=True, effective="full", why="away"))
        self.assertEqual(a["auto_note"], "Auto : FullIA (absent)")
        self.assertTrue(a["auto"])
        self.assertEqual(self.build_line(dict(bq, auto=True, why="battery"))["auto_note"], "")
        run = [{"pid": 1, "cwd": "/w/_stokesforge/feature476", "command": "cargo test -p x", "start": 1000 - 190}]
        w = self.build_line(dict(bq, running=run, waiting=[{}, {}]))
        self.assertEqual(w["now"], "Build en cours : feature476 · cargo test -p x · 3 min\n2 en attente")
        two = run + [{"pid": 2, "cwd": "/w/_stokesforge/feature453", "command": "cargo clippy", "start": 1000 - 70}]
        self.assertEqual(self.build_line(dict(bq, running=two))["now"],
                         "Build en cours : feature476 · cargo test -p x · 3 min\nBuild en cours : feature453 · cargo clippy · 1 min")
        self.assertIn("moins d’1 min", self.build_line(dict(bq, running=run), now=1000 - 190 + 20)["now"])
        self.assertIn("périmée", self.build_line(dict(bq, stale=True))["warn"])
        off = self.build_line({"installed": False})
        self.assertFalse(off["installed"])
        self.assertIn("scripts/cargo-queue-install.sh", off["note"])
        self.assertIsNone(self.build_line(None))
        # lang: fr end

    def test_build_row_sits_under_the_ci_line(self):
        html = self.page()
        self.assertLess(html.index('id="ciline"'), html.index('id="buildline"'))
        self.assertLess(html.index('id="buildline"'), html.index('class="tabs"'))
        self.assertIn("/api/build-mode", html)

    def img_js(self, expr):
        src = re.search(r"/\*IMAGES-START\*/(.*)/\*IMAGES-END\*/", self.page(), re.S).group(1)
        r = subprocess.run(["node", "-e", src + "\nprocess.stdout.write(JSON.stringify(%s));" % expr], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    # lang: fr begin
    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_attach_filter_checks_type_size_and_count_with_a_line_per_refusal(self):
        f = lambda t, size=100, name="a": {"name": name, "type": t, "size": size}
        out = self.img_js("imgFilter(%s, 0)" % json.dumps([
            f("image/png", name="ok.png"), f("image/svg+xml", name="x.svg"), f("text/plain", name="n.txt"),
            f("image/webp", 10 * 1024 * 1024 + 1, "big.webp"), f("image/gif", 0, "empty.gif"), f("image/jpeg", name="j.jpg")]))
        self.assertEqual([a["name"] for a in out["accept"]], ["ok.png", "j.jpg"])             # a mixed drop keeps its images
        self.assertEqual(len(out["lines"]), 4)
        self.assertTrue(all(l.split(" : ")[0] in ("x.svg", "n.txt", "big.webp", "empty.gif") for l in out["lines"]))
        self.assertEqual(self.img_js("imgFilter([%s], 0).accept.length" % json.dumps(f("image/png", 10 * 1024 * 1024))), 1)   # exactly 10 Mo
        full = self.img_js("imgFilter(%s, 4)" % json.dumps([f("image/png", name="a"), f("image/png", name="b")]))
        self.assertEqual((len(full["accept"]), full["lines"]), (2, []))                      # no cap; the advice is not a refusal line
        adv = "⚠ Plus de 5 captures dans ce retour"
        self.assertEqual([self.img_js("imgAdvice(%d)" % n) for n in (0, 5, 6, 7)], ["", "", adv, adv])   # shown above 5, gone back at 5
        page = self.page()
        self.assertLess(page.index("b.appendChild(ad);"), page.index("b.appendChild(adv);b.appendChild(st);"))   # the slot sits above the strip
        self.assertIn("imgAdvice(d.names.length+d.busy)", page)                              # recomputed on every strip redraw (add and remove)
        few = self.img_js("imgFilter(%s, 3)" % json.dumps([f("image/png", name="a"), f("image/png", name="b")]))
        self.assertEqual((len(few["accept"]), few["lines"]), (2, []))                        # 5 images: no warning

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_what_a_box_button_sends_keeps_the_images_and_refuses_only_an_empty_retour(self):
        P = lambda kind, text, names: self.img_js("imgPatch(%s,%s,%s)" % tuple(json.dumps(x) for x in (kind, text, names)))
        self.assertIsNone(P("retour", "  ", []))                                              # an empty box refuses Retour
        self.assertEqual(P("retour", "", ["a.png"]), {"status": "retour", "comment": "", "images": ["a.png"]})   # the screenshot is enough
        self.assertEqual(P("retour", " vu ", []), {"status": "retour", "comment": "vu", "images": []})
        self.assertEqual(P("ok", "tout", ["a.png", "b.png"]), {"status": "ok", "comment": "tout", "images": ["a.png", "b.png"]})   # C'est bon sends the box
        self.assertEqual(P("ok", "", []), {"status": "ok", "comment": "", "images": []})
        self.assertEqual(P("note", "", ["a.png"]), {"comment": "", "images": ["a.png"]})         # image-only comment
        self.assertEqual(P("note", "", []), {"comment": "", "images": []})                       # removing the last one is sent

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_the_idea_tick_sends_ok_with_its_text_and_refuses_an_empty_box(self):
        P = lambda text, names: self.img_js("imgPatch('idea',%s,%s)" % (json.dumps(text), json.dumps(names)))
        self.assertIsNone(P("", []))
        self.assertIsNone(P("  ", ["a.png"]))
        self.assertEqual(P(" more ", ["a.png"]), {"status": "ok", "idea": "more", "images": ["a.png"]})

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_failed_send_from_the_box_keeps_the_box_its_text_and_its_images(self):
        st = '{openBack:{id:"7-1",mode:"retour"},drafts:{"7-1":"vu","7-2":"x"},imgDrafts:{"7-1":{names:["a.png"]},"7-2":{names:[]}}}'
        kept = self.img_js("boxAfterSend(false,%s,'7-1')" % st)
        self.assertEqual(kept["openBack"]["id"], "7-1")
        self.assertEqual(kept["drafts"]["7-1"], "vu")
        self.assertEqual(kept["imgDrafts"]["7-1"]["names"], ["a.png"])
        done = self.img_js("boxAfterSend(true,%s,'7-1')" % st)
        self.assertIsNone(done["openBack"])
        self.assertNotIn("7-1", done["drafts"])
        self.assertNotIn("7-1", done["imgDrafts"])
        self.assertIn("7-2", done["drafts"])                                                   # another entry's draft stays
        body = re.search(r"async function boxSend\(.*?\n}\n", self.page(), re.S).group(0)
        self.assertNotIn("delete drafts", body)                                                # nothing is cleared before the answer
        self.assertNotIn("openBack=null", body)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_image_urls_encode_the_id_and_refuse_a_name_outside_the_pattern(self):
        ok = "0123456789abcdef01234567.png"
        self.assertEqual(self.img_js("imgUrl('7-1', %s)" % json.dumps(ok)), "/api/tests/7-1/images/" + ok)
        self.assertEqual(self.img_js("imgUrl('a/b', %s)" % json.dumps(ok)), "/api/tests/a%2Fb/images/" + ok)
        for bad in ('"><img src=x onerror=1>', "../x.png", "0123456789abcdef01234567.svg", "x.png", None):
            self.assertIsNone(self.img_js("imgUrl('7-1', %s)" % json.dumps(bad)), bad)
        self.assertEqual(self.img_js("imgListed({images:[%s,'../x','y',3]})" % json.dumps(ok)), [ok])

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_upload_failures_and_pastes_have_their_own_lines(self):
        for st in (400, 404):
            self.assertIn("relance-le", self.img_js("imgUploadLine(%d)" % st))                   # an old server
        self.assertIn("10 Mo", self.img_js("imgUploadLine(413)"))
        self.assertIn("format", self.img_js("imgUploadLine(415)"))
        self.assertNotIn("relance-le", self.img_js("imgUploadLine(500)"))
        items = [{"kind": "string", "type": "text/plain"}, {"kind": "file", "type": "image/png"},
                 {"kind": "file", "type": "application/pdf"}, {"kind": "file", "type": "image/svg+xml"}]
        got = self.img_js("imgFromItems(%s.map(i=>Object.assign(i,{getAsFile:()=>({type:i.type})})))" % json.dumps(items))
        self.assertEqual(got, [{"type": "image/png"}])

    def drag_run(self, html_path, start, end, setup, events, wrap=("", "")):
        """Run the drag handlers cut out of a page against a fake document; return what each event did."""
        with open(html_path, encoding="utf-8") as f:
            html = f.read()
        kind = re.search(r"/\*(?:IMAGES|QIMG)-START\*/.*?(function dragKind.*?\n}\n)", html, re.S).group(1)
        seg = html[html.index(start):html.index(end, html.index(start))]
        code = ("const L={};const document={addEventListener:(n,f)=>{L[n]=f;}};let notes=[],got=[];\n" + setup + "\n" + kind + wrap[0] + seg + wrap[1] + "\n"
                "const out=%s.map(([n,types,files])=>{const e={dataTransfer:{types,files},p:false,preventDefault(){this.p=true;}};"
                "if(L[n])L[n](e);return e.p;});"
                "process.stdout.write(JSON.stringify({out,notes,got,lines:typeof qLines==='undefined'?null:qLines}));") % json.dumps(events)
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_an_empty_drag_is_accepted_and_says_nothing_came_through_while_files_and_text_behave(self):
        ev = [["dragover", [], []], ["dragenter", [], []], ["dragover", ["Files"], []], ["dragover", ["text/plain"], []],
              ["drop", [], []], ["drop", ["Files"], [{"n": 1}]], ["drop", ["text/plain"], []]]
        r = self.drag_run(os.path.join(HERE, "suivi-page.html"), "const dragOf", "function testBox",
                          "const imgNote=t=>notes.push(t);const dropToBox=f=>{got.push(f.length);return true;};", ev)
        self.assertEqual(r["out"], [True, True, True, False, True, True, False])          # a text drag is never cancelled
        self.assertEqual(r["got"], [1])                                                    # the Files drop still attaches
        self.assertEqual(len(r["notes"]), 1)                                               # only the empty drop speaks
        self.assertIn("rien transmis", r["notes"][0])
        self.assertIn("Ctrl+V", r["notes"][0])

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_the_form_accepts_an_empty_drag_only_in_a_card(self):
        ev = [["dragover", [], []], ["drop", [], []], ["dragover", ["text/plain"], []], ["drop", ["Files"], [{"n": 1}]]]
        path = TEMPLATE
        setup = "let view='card',cur=0;const ORDER=[{c:'R1'}];const qLines={};const qStrip=()=>{};const qAttach=(d,f)=>got.push(f.length);"
        r = self.drag_run(path, "const dragOf", "if(/[?&]view=sum", setup, ev, ("if(true){", ""))
        self.assertEqual(r["out"], [True, True, False, True])
        self.assertEqual(r["got"], [1])
        self.assertIn("rien transmis", r["lines"]["R1"][0])                                # the empty drop says so on the card
        r = self.drag_run(path, "const dragOf", "if(/[?&]view=sum", setup.replace("'card'", "'sum'"), ev, ("if(true){", ""))
        self.assertEqual(r["out"], [False, False, False, True])                            # outside a card: as before
        self.assertEqual(r["lines"], {})

    def test_the_trial_page_wires_the_screenshots_safely(self):
        html = self.page()
        self.assertNotIn("innerHTML", html)                                                       # no name ever goes into markup
        for needle in ("addEventListener('paste'", "addEventListener('dragover'", "addEventListener('drop'", "im.loading='lazy'",
                       "const imgDrafts={}", "im.onerror=gone", "Capture supprimée (limite de 512 Mo)", "await imgIdle(d)",
                       "Ouvre d’abord la boîte"):
            self.assertIn(needle, html)
        self.assertIn("(x.comment||sent.length)?'Modifier le commentaire'", html)                # image-only entries can be edited
    # lang: fr end

    def card_line(self, c, tests=(), qcms=()):
        src = re.search(r"/\*CARDLINE-START\*/(.*)/\*CARDLINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(cardLine(%s,%s,%s)));" % (
            json.dumps(c), json.dumps(list(tests)), json.dumps(list(qcms)))
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_withdraw_control_asks_first_and_states_how_many_answers_exist(self):
        src = re.search(r"/\*CARDLINE-START\*/(.*)/\*CARDLINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify([withdrawUi({progress:{answered:2,total:3}},false),withdrawUi({progress:{answered:2,total:3}},true)," \
                     "withdrawUi({progress:{answered:1,total:3},recorded:5},true),withdrawUi({progress:{answered:0,total:3},recorded:1},true)]));"
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        idle, two, one, none = json.loads(r.stdout)
        # lang: fr begin
        self.assertEqual((idle["asking"], idle["button"]), (False, "Retirer"))
        self.assertEqual((two["asking"], two["question"], two["yes"], two["no"]),
                         (True, "Retirer ce formulaire ? 2 réponses déjà données.", "Oui, retirer", "Annuler"))
        self.assertEqual(one["question"], "Retirer ce formulaire ? 1 réponse déjà donnée.")
        self.assertEqual(none["question"], "Retirer ce formulaire ? Aucune réponse donnée.")
        # lang: fr end

    def test_withdraw_confirmation_state_lives_outside_the_dom(self):
        html = self.page()
        self.assertIn("askWithdraw=null", html)
        self.assertIn("askWithdraw===q.id", html)
        self.assertIn("/withdraw'", html)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_card_line_says_whose_turn_it_is(self):
        card = {"issue": 7, "issues": [7], "package": None, "held_by": ["agent"], "waiting": "", "wait_kind": "", "agent_idle_min": 1}
        todo = lambda n, st="todo": {"n": n, "status": st}
        L = self.card_line
        self.assertEqual(L(card, [], [{"issue": 7}])["text"], "À toi : répondre au questionnaire")        # lang: fr
        l = L(card, [todo(7), todo(7), todo(7), todo(7, "ok"), todo(8)])
        self.assertEqual((l["user"], l["text"], l["also"]), (True, "À toi : cocher 3 essais", "l’agent travaille"))   # lang: fr
        self.assertEqual(L(card, [todo(7)])["text"], "À toi : cocher 1 essai")                              # lang: fr
        l = L(card, [todo(7, "ok")])
        self.assertEqual({k: l[k] for k in ("user", "text", "also", "note")},
                         {"user": False, "text": "L’agent travaille", "also": "", "note": ""})   # lang: fr
        paused = dict(card, held_by=[])
        self.assertEqual(L(paused)["text"], "En pause")                                                     # lang: fr
        l = L(paused, [], [{"issue": 7}])
        self.assertEqual((l["user"], l["also"]), (True, "rien ne bouge sans toi"))                   # lang: fr
        # the open form or the pending trial is itself what holds the colour: still nobody at work
        l = L(dict(card, held_by=["qcm"]), [], [{"issue": 7}])
        self.assertEqual((l["user"], l["also"]), (True, "rien ne bouge sans toi"))                   # lang: fr
        l = L(dict(card, held_by=["essai"]), [todo(7)])
        self.assertEqual((l["user"], l["also"]), (True, "rien ne bouge sans toi"))                   # lang: fr
        self.assertEqual(L(dict(card, held_by=["legacy"]), [todo(7)])["also"], "")
        l = L(card, [], [{"issue": 7}])
        self.assertEqual((l["text"], l["also"]), ("À toi : répondre au questionnaire", "l’agent travaille"))   # lang: fr
        # a note of kind `you` is the user's turn, alone or joined; other kinds stay muted text
        l = L(dict(card, wait_kind="you", waiting="run make"))
        self.assertEqual((l["user"], l["text"], l["note"]), (True, "À toi : run make", ""))                 # lang: fr
        l = L(dict(card, wait_kind="you", waiting="run make"), [], [{"issue": 7}])
        self.assertEqual(l["text"], "À toi : répondre au questionnaire · run make")                         # lang: fr
        for kind in ("dep", "ag"):
            l = L(dict(card, wait_kind=kind, waiting="my note"))
            self.assertEqual((l["user"], l["text"], l["note"]), (False, "L’agent travaille", "my note"))    # lang: fr

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_card_line_listening_and_merge_words(self):
        L = self.card_line
        # lang: fr begin
        base = {"issue": 7, "issues": [7], "held_by": ["agent"], "waiting": "", "wait_kind": ""}
        todo = {"n": 7, "status": "todo", "try": "x"}
        ok = {"n": 7, "status": "ok", "try": "x"}
        # an old server (no `listening`) keeps today's texts
        self.assertEqual(L(dict(base, agent_idle_min=9))["text"], "Agent à l’arrêt depuis 9 min")
        self.assertEqual(L(dict(base, agent_idle_min=9), [todo])["also"], "agent à l’arrêt, rien ne bouge sans toi")
        # working: unchanged whatever `listening` says
        self.assertEqual(L(dict(base, agent_idle_min=1, listening=True))["text"], "L’agent travaille")
        # listening and not visibly at work
        for idle in (5, None):
            self.assertEqual(L(dict(base, agent_idle_min=idle, listening=True))["text"], "L’agent écoute")
            l = L(dict(base, agent_idle_min=idle, listening=True), [todo])
            self.assertEqual(l["also"], "l’agent écoute, il repart à ta réponse")
        # not listening, stopped
        self.assertEqual(L(dict(base, agent_idle_min=9, listening=False))["text"], "Agent à l’arrêt depuis 9 min")
        self.assertEqual(L(dict(base, agent_idle_min=9, listening=False), [todo])["also"],
                         "agent à l’arrêt, écris-lui après ta réponse")
        # paused with an unread answer
        paused = {"issue": 7, "held_by": [], "waiting": "", "wait_kind": "", "unread": 2}
        l = L(paused)
        self.assertEqual((l["user"], l["text"]), (True, "Réponse reçue : lance #7"))
        self.assertEqual(L(dict(paused, unread=0))["text"], "En pause")
        # all `try` entries OK: write to the agent to merge
        l = L(dict(base, agent_idle_min=1, listening=True), [ok, dict(ok)])
        self.assertEqual((l["user"], l["text"], l["also"]), (True, "À toi : lancer la fusion", ""))
        l = L(dict(base, agent_idle_min=1), [ok], [{"issue": 7}])
        self.assertEqual(l["text"], "À toi : répondre au questionnaire · lancer la fusion")
        self.assertNotIn("fusion", L(base, [ok, todo])["text"])
        self.assertNotIn("fusion", L(base, [{"n": 7, "status": "ok"}])["text"])      # merged entry: no `try`
        self.assertEqual((L(base, [todo])["mergeReady"], L(base, [{"n": 7, "status": "ok"}])["mergeReady"]), (False, False))
        # the request: grey note, button gone; "nothing listens" adds the reading line
        l = L(dict(base, agent_idle_min=1, listening=False, merge_requested=True), [ok])
        self.assertEqual((l["mergeReady"], l["mergeRequested"], l["mergeNote"]),
                         (True, True, "Fusion demandée · l’agent la lira à sa reprise"))
        self.assertNotIn("fusion", l["text"].lower())
        l = L(dict(base, agent_idle_min=9, listening=True, merge_requested=True), [ok])
        self.assertEqual(l["mergeNote"], "Fusion demandée")
        # a request the server voided (flag false) or an entry not OK any more: no request
        self.assertFalse(L(dict(base, merge_requested=True), [ok, todo])["mergeRequested"])
        self.assertFalse(L(dict(base, merge_requested=False), [ok])["mergeRequested"])
        # lang: fr end

    def split_node(self, expr):
        page = self.page()
        card = re.search(r"/\*CARDLINE-START\*/(.*)/\*CARDLINE-END\*/", page, re.S).group(1)
        split = re.search(r"/\*SPLIT-START\*/(.*)/\*SPLIT-END\*/", page, re.S).group(1)
        r = subprocess.run(["node", "-e", card + split + "\nprocess.stdout.write(JSON.stringify(%s));" % expr],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_a_settled_retour_moves_to_its_own_section(self):
        st = {"at": "2026-11-09T10:00:00", "by": "agent"}
        tests = [{"id": "1-1", "status": "todo"}, {"id": "1-2", "status": "retour"},
                 {"id": "1-3", "status": "retour", "settled": st}, {"id": "1-4", "status": "ok"},
                 {"id": "1-5", "status": "ok", "idea": "i"}, {"id": "1-6", "status": "ok", "idea": "j", "settled": st}]
        got = self.split_node("(()=>{const s=splitTests(%s);return Object.fromEntries(Object.entries(s).map(([k,v])=>[k,v.map(x=>x.id)]));})()" % json.dumps(tests))
        self.assertEqual(got, {"todo": ["1-1"], "back": ["1-2", "1-5"], "settled": ["1-3", "1-6"], "done": ["1-4"]})
        html = self.page()
        self.assertIn('id="n-settled"', html)
        self.assertLess(html.index('id="back"'), html.index('id="settled-wrap"'))
        self.assertLess(html.index('id="settled-wrap"'), html.index('id="done-wrap"'))
        self.assertIn("$('n-settled').textContent=settled.length", html)
        # lang: fr begin
        self.assertIn("En traitement", html)
        self.assertIn("Traités (", html)
        self.assertNotIn("C’est bon", html)
        text = self.split_node("[settledText({settled:{by:'user',at:'x'}},'9 nov.'),"
                               "settledText({settled:{by:'agent',commit:'0123456789abcdef',note:'#12'}},'9 nov.')]")
        self.assertEqual(text, ["classé par toi le 9 nov.", "traité le 9 nov. · commit 01234567 · #12"])
        # lang: fr end

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_the_trial_frame_and_the_idea_tick(self):
        got = self.split_node("[trialFrame({goal:' g ',around:'a'}),trialFrame({}),trialFrame({goal:'',around:'  '})]")
        # lang: fr begin
        self.assertEqual(got, [{"goal": "On teste : g", "around": "Essaie autour : a"}] + [{"goal": "", "around": ""}] * 2)
        page = self.page()
        self.assertIn("OK + idée", page)
        self.assertIn("Envoyer l’idée", page)
        self.assertIn("Ton idée", page)
        # lang: fr end
        c = {"issue": 7, "issues": [7], "held_by": ["agent"], "waiting": "", "wait_kind": "", "agent_idle_min": 1}
        self.assertTrue(self.card_line(c, [{"n": 7, "status": "ok", "idea": "more", "try": "x"}])["mergeReady"])

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_the_classer_button_is_for_an_unsettled_retour_only(self):
        st = {"at": "t", "by": "user"}
        got = self.split_node("[{status:'todo'},{status:'ok'},{status:'retour'},{status:'retour',settled:%s}].map(canSettle)" % json.dumps(st))
        self.assertEqual(got, [False, False, True, False])
        self.assertIn("if(canSettle(x)){", self.page())
        self.assertIn("'Classer'", self.page())              # lang: fr

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_the_merge_button_counts_a_settled_retour_as_done(self):
        c = {"issue": 7, "issues": [7], "held_by": ["agent"], "waiting": "", "wait_kind": "", "agent_idle_min": 1}
        ok = {"n": 7, "status": "ok", "try": "x"}
        done = {"n": 7, "status": "retour", "try": "x", "settled": {"at": "t", "by": "agent"}}
        open_ = {"n": 7, "status": "retour", "try": "x"}
        self.assertTrue(self.merge_ui(c, [ok, done])["button"])
        self.assertFalse(self.merge_ui(c, [ok, open_])["button"])
        self.assertTrue(self.card_line(c, [ok, done])["mergeReady"])
        self.assertFalse(self.card_line(c, [ok, open_])["mergeReady"])

    def merge_ui(self, c, tests):
        src = re.search(r"/\*CARDLINE-START\*/(.*)/\*CARDLINE-END\*/", self.page(), re.S).group(1)
        code = src + "\nprocess.stdout.write(JSON.stringify(mergeUi(cardLine(%s,%s,[]))));" % (json.dumps(c), json.dumps(tests))
        r = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_the_merge_button_shows_by_state(self):
        c = {"issue": 7, "issues": [7], "held_by": ["agent"], "waiting": "", "wait_kind": "", "agent_idle_min": 1}
        ok, todo = {"n": 7, "status": "ok", "try": "x"}, {"n": 7, "status": "todo", "try": "x"}
        self.assertIn("'Fusionner'", self.page())
        self.assertTrue(self.merge_ui(c, [ok])["button"])
        self.assertFalse(self.merge_ui(c, [ok, todo])["button"])
        self.assertFalse(self.merge_ui(c, [])["button"])
        self.assertFalse(self.merge_ui(c, [{"n": 7, "status": "ok"}])["button"])           # merged entry, no `try`
        u = self.merge_ui(dict(c, merge_requested=True, listening=True), [ok])
        self.assertEqual((u["button"], u["note"]), (False, "Fusion demandée"))             # lang: fr

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_card_line_tells_a_working_agent_from_a_stopped_one(self):
        base = {"issue": 7, "issues": [7], "held_by": ["agent"], "waiting": "", "wait_kind": ""}
        L = self.card_line
        # lang: fr begin
        for idle, text in ((4, "L’agent travaille"), (5, "Agent à l’arrêt depuis 5 min"), (59, "Agent à l’arrêt depuis 59 min"),
                           (125, "Agent à l’arrêt depuis 2 h"), (None, "Session ouverte")):
            self.assertEqual(L(dict(base, agent_idle_min=idle))["text"], text, idle)
        self.assertEqual(L(base)["text"], "Session ouverte")                                  # old server: no field   # lang: fr
        mine = [{"issue": 7}]
        for idle, also in ((4, "l’agent travaille"), (5, "agent à l’arrêt, rien ne bouge sans toi"),
                           (0, "l’agent travaille"), (None, "session ouverte")):
            l = L(dict(base, agent_idle_min=idle), [], mine)
            self.assertEqual((l["user"], l["also"]), (True, also), idle)
        # lang: fr end
        l = L(dict(base, agent_idle_min=4), [], mine)
        self.assertEqual(l["text"], "À toi : répondre au questionnaire")                       # lang: fr
        self.assertEqual(L(dict(base, held_by=["legacy"]), [], mine)["also"], "")

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


class TestLine(Base):
    """Integration line (#480): the record field, its inheritance, the merge check, the line worktree."""
    def gout(self, *a, cwd=None):
        return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *a], cwd=cwd or self.repo,
                              check=True, capture_output=True, text=True).stdout.strip()

    def line_of(self, n):
        return self.run_cli("line", str(n)).stdout.strip()

    def put_test(self, tid, **fields):
        os.makedirs(os.path.join(self.state, "tests"), exist_ok=True)
        with open(os.path.join(self.state, "tests", tid + ".json"), "w") as f:
            json.dump(dict({"title": tid, "status": "todo", "n": int(tid.split("-")[0])}, **fields), f)

    def test_claim_writes_the_line_and_each_claim_rewrites_it(self):
        self.claim(7, "--line", "PostV1.1")
        self.assertEqual((self.rec(7)["line"], self.line_of(7)), ("PostV1.1", "PostV1.1"))
        self.claim(7)                               # the declaration is gone: back to main
        self.assertNotIn("line", self.rec(7))
        self.assertEqual(self.line_of(7), "main")
        self.assertEqual(self.line_of(99), "main")  # no record
        self.assertNotEqual(self.run_cli("claim", "8", "--line", "../x").returncode, 0)

    def test_a_sub_issue_inherits_the_line_of_its_parent(self):
        self.claim(10, "--package", "--line", "PostV1.1")
        self.claim(11, "--parent", "10")            # the parent claimed for it keeps its line
        self.assertEqual(self.rec(10)["line"], "PostV1.1")
        self.assertNotIn("line", self.rec(11))
        self.assertEqual(self.line_of(11), "PostV1.1")
        self.assertEqual(suivi.line_of({1: {"issue": 1, "parent": 2}, 2: {"issue": 2, "line": "L"}}, 1), "L")

    def test_merge_commit_is_checked_on_the_line_of_the_issue(self):
        self.gout("branch", "PostV1.1", "main")
        self.gout("checkout", "-q", "PostV1.1")
        self.gout("commit", "-q", "--allow-empty", "-m", "Merge #71: on the line")
        c71 = self.gout("rev-parse", "HEAD")
        self.gout("commit", "-q", "--allow-empty", "-m", "Merge #72: on the line")
        c72 = self.gout("rev-parse", "HEAD")
        self.gout("checkout", "-q", "main")
        self.put(71, line="PostV1.1")
        self.put(72)
        ok = self.run_cli("test", "merged", "71", c71)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        bad = self.run_cli("test", "merged", "72", c72)
        self.assertNotEqual(bad.returncode, 0)
        self.assertIn("is not on main", bad.stderr)

    def test_a_declared_line_without_a_branch_is_refused(self):
        self.put(73, line="Gone")
        c = self.gout("rev-parse", "HEAD")
        r = self.run_cli("test", "merged", "73", c)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("no local branch", r.stderr)

    def test_line_trial_wants_one_entry_that_tries_the_line_and_all_of_them_ok(self):
        k = 480
        r = self.run_cli("line-trial", str(k), "PostV1.1")
        self.assertEqual(r.returncode, 1)
        self.assertIn("no trial entry", r.stdout)
        self.put_test("480-1", **{"try": "scripts/try.sh 480", "status": "ok"})      # the issue's own entry does not count
        self.assertEqual(self.run_cli("line-trial", str(k), "PostV1.1").returncode, 1)
        self.put_test("480-2", **{"try": "scripts/try.sh PostV1.1", "status": "todo"})
        r = self.run_cli("line-trial", str(k), "PostV1.1")
        self.assertEqual((r.returncode, "480-2" in r.stdout), (1, True))
        self.put_test("480-2", **{"try": "scripts/try.sh PostV1.1", "status": "retour"})
        self.assertEqual(self.run_cli("line-trial", str(k), "PostV1.1").returncode, 1)
        self.put_test("480-2", **{"try": "scripts/try.sh PostV1.1", "status": "ok"})
        self.assertEqual(self.run_cli("line-trial", str(k), "PostV1.1").returncode, 0)
        self.put_test("480-3", **{"try": "scripts/try.sh PostV1.1", "status": "todo"})
        self.assertEqual(self.run_cli("line-trial", str(k), "PostV1.1").returncode, 1)

    def test_line_clear_drops_the_line_of_every_record(self):
        self.put(1, line="PostV1.1")
        self.put(2, line="Other")
        self.put(3)
        self.assertIn("1 record cleared", self.run_cli("line-clear", "PostV1.1").stdout)
        self.assertNotIn("line", self.rec(1))
        self.assertEqual(self.rec(2)["line"], "Other")

    def test_line_worktrees_are_not_work_packages(self):
        self.git("worktree", "add", "-q", os.path.join(self.root, "line-PostV1.1"), "-b", "PostV1.1", "main")
        r = self.run_cli("check")
        self.assertEqual((r.returncode, r.stdout), (0, ""), r.stderr)
        st = json.loads(self.run_cli("status", "--json").stdout)
        self.assertEqual(st["colourless"], [])


class TestHelp(Base):
    def test_help_runs_isolated(self):
        r = self.run_cli("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("claim", r.stdout)


if __name__ == "__main__":
    unittest.main()
