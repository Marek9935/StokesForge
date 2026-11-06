#!/usr/bin/env python3
"""Contracts of scripts/time-report.py, on synthetic session logs (no real log is read).

One test per defect of the first measurement (#452); each one fails on the old logic.
"""

import contextlib
import datetime as dt
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SRC = Path(os.environ.get("TIME_REPORT_SRC", Path(__file__).with_name("time-report.py")))
spec = importlib.util.spec_from_file_location("time_report", SRC)
tr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tr)

T0 = dt.datetime(2026, 11, 2, 10, 0, 0, tzinfo=dt.timezone.utc).timestamp()
SINCE = T0 - 3600
UNTIL = T0 + 20 * 3600
CANARY = "CANARY-7f3a91"


def iso(t):
    return dt.datetime.fromtimestamp(T0 + t, dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")


def human(t, text="go"):
    return {"type": "user", "timestamp": iso(t), "origin": {"kind": "human"}, "message": {"content": text}}


def notif(t, task):
    return {"type": "user", "timestamp": iso(t), "origin": {"kind": "task-notification"},
            "message": {"content": "<task-notification><task-id>%s</task-id></task-notification>" % task}}


def enqueue(t, task):
    return {"type": "queue-operation", "operation": "enqueue", "timestamp": iso(t),
            "content": "<task-notification><task-id>%s</task-id></task-notification>" % task}


def asst(t, text="", tools=(), stop="end_turn", mid=None):
    content = [{"type": "text", "text": text}] if text else []
    for i, name, inp in tools:
        content.append({"type": "tool_use", "id": i, "name": name, "input": inp})
    return {"type": "assistant", "timestamp": iso(t),
            "message": {"id": mid or "m%d" % t, "model": "m", "stop_reason": stop, "content": content}}


def result(t, tid, tur=None):
    return {"type": "user", "timestamp": iso(t),
            "message": {"content": [{"type": "tool_result", "tool_use_id": tid, "content": "x"}]},
            "toolUseResult": tur or {}}


def bash(t, tid, cmd, **extra):
    return asst(t, tools=[(tid, "Bash", dict(command=cmd, **extra))], stop="tool_use")


class Logs(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)
        self.n = 0

    def tearDown(self):
        self._tmp.cleanup()

    def session(self, rows, cost=None, raw=None):
        self.n += 1
        path = self.dir / ("s%d.jsonl" % self.n)
        rows = list(rows) + ([cost] if cost else [])
        path.write_text("\n".join(json.dumps(r) for r in rows) + "\n" + (raw or ""))
        return path

    def run_report(self, power=None):
        return tr.analyze(str(self.dir), SINCE, UNTIL, power)

    def h(self, r, key):
        return r["main_h"].get(key, 0)


class Defects(Logs):
    def test_background_task_without_notification_is_not_ci(self):
        self.session([human(0), bash(5, "a", "scripts/ci-status.sh", run_in_background=True),
                      result(6, "a", {"backgroundTaskId": "b1"}), asst(10, "done"), human(7210, "ok")])
        r = self.run_report()
        self.assertAlmostEqual(self.h(r, "wait:nothing"), 2.0, delta=0.02)
        self.assertLess(self.h(r, "ci"), 0.01)

    def test_background_task_with_notification_is_ci(self):
        self.session([human(0), bash(5, "a", "scripts/ci-status.sh", run_in_background=True),
                      result(6, "a", {"backgroundTaskId": "b1"}), asst(10, "done"),
                      enqueue(1810, "b1"), notif(1810, "b1"), human(3600, "ok")])
        r = self.run_report()
        self.assertAlmostEqual(self.h(r, "ci"), 0.5, delta=0.02)

    def test_redirection_digit_is_not_an_issue(self):
        self.session([human(0), bash(5, "a", "scripts/chantier.sh 452 2>&1"), result(6, "a"),
                      asst(100, "ok"), human(160)])
        r = self.run_report()
        self.assertEqual(sorted(r["issues"]), ["452"])

    def test_wait_cut_by_a_suspend_is_one_episode(self):
        self.session([human(0), asst(10, "done"), human(10810, "ok")])
        down = dict(suspend=[[T0 + 3000, T0 + 6600]], off=[])
        r = self.run_report(down)
        self.assertEqual(sum(e["n"] for e in r["episodes"]), 1)
        self.assertAlmostEqual(self.h(r, "wait:nothing"), 2.0, delta=0.01)
        self.assertAlmostEqual(self.h(r, "pc_down"), 1.0, delta=0.01)

    def test_stopped_agent_woken_by_event_is_not_user_wait(self):
        self.session([human(0), asst(10, "done"), enqueue(6850, "zz"), notif(6850, "zz"),
                      asst(6860, "back")])
        r = self.run_report()
        self.assertAlmostEqual(self.h(r, "agent_stuck"), 1.9, delta=0.02)
        self.assertEqual(sum(v for k, v in r["main_h"].items() if k.startswith("wait:")), 0)

    def test_session_without_cost_state_is_counted_apart(self):
        self.session([human(0), asst(10, "a", stop="tool_use"), human(300)])
        cost = {"type": "cost-state", "totalAPIDuration": 600000}
        self.session([human(0), asst(10, "a"), human(30)], cost=cost)
        c = self.run_report()["cost_state"]
        self.assertEqual((c["compared"], c["without"]), (1, 1))
        self.assertAlmostEqual(c["api_h"], 600 / 3600, delta=0.01)

    def test_trial_opened_turns_ago_still_pending(self):
        self.session([human(0), bash(5, "a", "scripts/suivi.py test add 9"), result(6, "a"), asst(10, "ok"),
                      human(20, "unrelated"), asst(30, "", stop="end_turn"), human(7230, "hello")])
        self.assertAlmostEqual(self.h(self.run_report(), "wait:trial"), 2.0, delta=0.02)

    def test_form_plus_trial_reads_as_trial(self):
        self.session([human(0), bash(5, "a", "scripts/suivi.py test add 9"), result(6, "a"),
                      bash(7, "b", "scripts/suivi.py qcm pose f.json"), result(8, "b"), asst(10, "see the form"),
                      human(3610, "hello")])
        r = self.run_report()
        self.assertGreater(self.h(r, "wait:trial"), 0.9)
        self.assertEqual(self.h(r, "wait:form"), 0)

    def test_withdrawn_form_is_not_a_wait_on_a_form(self):
        self.session([human(0), bash(7, "b", "scripts/suivi.py qcm pose 9 f.js"), result(8, "b"),
                      bash(9, "c", "scripts/suivi.py qcm withdraw 9-1"), result(10, "c"), asst(12, "withdrawn"),
                      human(3610, "hello")])
        r = self.run_report()
        self.assertGreater(self.h(r, "wait:nothing"), 0.9)
        self.assertEqual(self.h(r, "wait:form"), 0)

    def test_answered_form_and_merged_trial_are_cleared(self):
        self.session([human(0), bash(5, "a", "scripts/suivi.py test add 9"), result(6, "a"),
                      bash(7, "b", "scripts/suivi.py test merged 9"), result(8, "b"), asst(10, "all done"),
                      human(3610, "hello")])
        self.assertGreater(self.h(self.run_report(), "wait:nothing"), 0.9)

    def test_session_open_wait_is_its_own_category(self):
        self.session([{"type": "user", "timestamp": iso(0), "message": {"content": "<command-name>/clear</command-name>"}},
                      human(1800, "lance #77"), asst(1810, "ok", stop="end_turn"), human(1900, "x")])
        r = self.run_report()
        self.assertAlmostEqual(self.h(r, "wait:session_open"), 0.5, delta=0.01)
        self.assertEqual(r["issues"]["77"].get("wait:session_open", 0), 0)

    def test_wait_counts_whole_for_each_issue_active_time_is_shared(self):
        tag = "\U0001F7E6 #10 \U0001F7E9 #11 "
        self.session([human(0), asst(100, tag + "start", stop="end_turn"), human(3700, "go")])
        i = self.run_report()["issues"]
        self.assertAlmostEqual(i["10"]["wait:nothing"], 1.0, delta=0.01)
        self.assertAlmostEqual(i["11"]["wait:nothing"], 1.0, delta=0.01)
        self.assertAlmostEqual(i["10"]["model"], i["11"]["model"], delta=0.001)

    def test_agent_spawned_before_first_tag_takes_the_later_issue(self):
        agent = self.dir / "s1" / "subagents"
        rows = [human(0), asst(5, tools=[("a", "Agent", {"description": "d"})], stop="tool_use"),
                result(6, "a", {"isAsync": True, "agentId": "ag1"}), asst(20, "\U0001F7E6 #55 x", stop="end_turn"), human(100, "more")]
        self.session(rows)
        agent.mkdir(parents=True)
        (agent / "agent-ag1.jsonl").write_text("\n".join(json.dumps(r) for r in [
            human(7, "task"), asst(8, "work", stop="tool_use"), asst(600, "end", stop="end_turn")]) + "\n")
        r = self.run_report()
        self.assertNotIn("0", r["issues"])
        self.assertGreater(sum(v for k, v in r["issues"]["55"].items() if k.startswith("agent_")), 0.1)
        self.assertGreater(r["agents_h"].get("model", 0) + r["agents_h"].get("tools", 0), 0.1)

    def test_heredoc_body_is_not_a_test_run(self):
        cmd = "git commit -F - <<'EOF'\nfix: cargo test --workspace passes\nEOF"
        self.assertNotEqual(tr.bash_cat(cmd), "test")
        self.assertEqual(tr.bash_cat("cargo test -p a"), "test")

    def test_reading_images_is_not_a_driver_session(self):
        self.session([human(0), asst(5, tools=[("a", "Read", {"file_path": "/x/shot.png"})], stop="tool_use"),
                      result(8, "a"), asst(10, "seen", stop="end_turn")])
        self.session([human(0), bash(5, "a", "python3 .claude/skills/run-stokesforge/driver.py start demo"),
                      result(9, "a"), asst(10, "up", stop="end_turn")])
        self.assertEqual(self.run_report()["sessions"]["driver"], 1)

    def test_model_is_a_range_and_session_end_is_kept(self):
        req = {"type": "api-request", "timestamp": iso(100), "querySource": "repl_main_thread"}
        self.session([human(0), req, asst(800, "text", stop="end_turn"),
                      {"type": "user", "timestamp": iso(900), "message": {"content": "<command-name>/x</command-name>"}}])
        r = self.run_report()
        lo, hi = r["model_range_h"]
        self.assertGreater(lo, 0)
        self.assertLess(lo, hi)
        self.assertGreater(self.h(r, "session_end"), 0)


class Privacy(Logs):
    def test_nothing_from_the_conversation_is_printed(self):
        c = CANARY
        rows = [
            human(0, "lance #31 " + c),
            asst(5, "\U0001F7E6 #31 " + c, tools=[("a", "Bash", {"command": "echo " + c, "description": c})], stop="tool_use"),
            result(6, "a", {"stdout": c}),
            {"type": "ai-title", "aiTitle": c},
            asst(10, "reply " + c, stop="end_turn"),
            human(500, c),
        ]
        self.session(rows, raw="{not json " + c + "\n")
        power = self.dir / "p.json"
        power.write_text(json.dumps(dict(suspend=[], off=[])))
        outs = [subprocess.run([sys.executable, str(SRC), "--logs", str(self.dir), "--power", str(power),
                                "--since", "2026-11-02", "--until", "2026-11-03"] + extra,
                               capture_output=True, text=True) for extra in ([], ["--json"])]
        for p in outs:
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertNotIn(c, p.stdout + p.stderr)

    def test_report_runs_and_stays_clean(self):
        c = CANARY
        self.session([human(0, c), asst(5, c, stop="end_turn"), human(500, c)], raw="{broken " + c + "\n")
        power = self.dir / "p.json"
        power.write_text(json.dumps(dict(suspend=[], off=[])))
        outs = [subprocess.run([sys.executable, str(SRC), "--logs", str(self.dir), "--power", str(power),
                                "--since", "2026-11-02", "--until", "2026-11-03"] + extra,
                               capture_output=True, text=True) for extra in ([], ["--json"])]
        for p in outs:
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertNotIn(c, p.stdout + p.stderr)
        json.loads(outs[1].stdout)
        self.assertIn("User wait", outs[0].stdout)

    def test_bad_arguments_exit_2(self):
        p = subprocess.run([sys.executable, str(SRC), "--since", "yesterday", "--logs", str(self.dir)],
                           capture_output=True, text=True)
        self.assertEqual(p.returncode, 2)
        p = subprocess.run([sys.executable, str(SRC), "--logs", str(self.dir / "missing")], capture_output=True, text=True)
        self.assertEqual(p.returncode, 2)


if __name__ == "__main__":
    unittest.main()
