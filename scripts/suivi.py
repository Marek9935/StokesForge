#!/usr/bin/env python3
"""Work-package colours and who holds them (#441). Standard library only.

    python3 -I scripts/suivi.py <command> ...

The single source of truth is ../_stokesforge/.suivi/ (override: SF_SUIVI_DIR):

    issues/<n>.json        one record per issue: colour, last_colour, kind
                           (issue|package), parent, subs, name, state, waiting,
                           wait_kind, what, since, owner{session,pid,pid_start,at},
                           done_at
    ended/<session_id>     empty files touched by the SessionEnd hook
    qcm/<id>/data.js       the decisions (injected into the form template when served)
    qcm/<id>/meta.json     {issue, title, created, cards, finished, finished_at}
    qcm/<id>/answers/<code>.json   one record per card (written by the page's POST)
    tests/<n>-<k>.json     trial entry: title, intro, where, steps, expect, try, commit, date,
                           status (todo|ok|retour), comment, reviewed (written by the
                           page's POST and by `test add|merged`)
    removed-tests/<n>-<k>.json   an entry withdrawn by `test remove` (archive, with removed_at;
                           outside tests/ so nothing reads it, and its number is never reused)
    control.json           result of the last `check`

THE RULE. An issue holds its colour if, and only if, (a) the Claude session that
owns it is alive, or (b) one of its QCMs is open, or (c) one of its trial entries
(with a `try` field) is still todo. Otherwise it is PAUSED: the colour is free,
the worktree stays. On resume (`claim`) the issue gets its previous colour back if
nobody took it, else the first free one. "Held" is computed when read, never
stored: a crash pauses an issue with no process left to write.

Writes go through a temporary file and os.replace. Colour allocation takes the
directory lock ../_stokesforge/.colour.lock (same lock as chantier.sh). Reads take
no lock. Test seams: SF_SUIVI_DIR, SF_CLAUDE_PROJECTS (default ~/.claude/projects), SF_TODAY=YYYY-MM-DD, SF_SUIVI_LOCK_WAIT (seconds),
SF_SUIVI_SLOW (seconds slept inside a claim's critical section), SF_SUIVI_CI=off (no CI watcher),
SF_SUIVI_CI_POLL / SF_SUIVI_CI_TIMEOUT (seconds).

CI WATCHER (#452). `serve` runs a daemon thread that polls the CI of the newest commit of `main`
every ~120 s through `scripts/ci-status.sh --head` (glab, one PROJECT_ID): the Linux and Windows jobs
all come back as GitLab statuses (scripts/gitlab-status.py JOBS). It writes ci.json; /api/state carries
`ci` = {verdict: green|red|pending|unknown, names, reason, sha, last_red}. Green needs every blocking
job present and SUCCESS (`robustness` is non-blocking); any doubt (CANCELED, glab or network failure,
timeout, unparsable, stale file older than 3 polls) is `unknown`, never green. A red stays in
`last_red` until a newer head is green. The colour rule and the
`pending` count do not depend on it.

WAIT (#454). `wait <n> [--timeout S]` blocks, polling local files every 5 s, until an event the
agent must read: a trial entry of the issue's package answered (status or comment changed) or a
decision form finished. It prints one line per event and a last line (what stays open, whether to
re-arm), then exits 0 so a background run wakes the agent. It also exits 0 with one line when there
is nothing left to wait for (done, paused, taken, session gone, nothing open) or after 72 h. Exit 1:
no record, or the session is not the owner. `.suivi/heard/<root>.json` {session, seen} remembers what
was reported (only suivi.py writes it, just before printing; another session starts afresh).
`.suivi/waits/<root>.json` {pid, pid_start, session, started} registers the wait (stale = pid_start
mismatch), `waits/log` records SIGTERM/SIGHUP. When to wake is ONE function, `should_wake`. A wait never
holds a colour and never writes issues/ or tests/. `news <n>` prints and records the unseen events.
Seams: SF_SUIVI_WAIT_POLL (seconds), SF_SUIVI_WAIT_SETTLE (delay for a Retour or comment, default 180 s).
`hook-wait` (PostToolUse hook, asyncRewake) arms that same wait after a `suivi.py` call on an issue the
session owns: exit 2 with the text on stderr for an event, exit 0 silent otherwise. `hook-news` (SessionStart / UserPromptSubmit hooks) prints and records an owning session's unseen events,
or tells a new session which issues hold unread answers; it always exits 0.

A record migrated from the old layout carries `legacy: true`: it counts as held
until the issue is claimed, paused or done (its session is unknown).

Commands: claim, status, set, pause, done, release-line, check, migrate,
members, is-package, session-end, hook-wait, hook-news, wait, news, serve (the local page, 127.0.0.1 only),
test add|merged|remove|list, qcm pose|answers|list, import-online.
"""
import argparse
import datetime
import errno
import glob
import hashlib
import html
import http.server
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

COLOURS = ["🟦", "🟩", "🟧", "🟪", "🟥", "🟨"]
STATES = ["questions", "code", "essai", "fusion"]
WAIT_KINDS = ["you", "dep", "ag"]


class Fail(Exception):
    pass


# --------------------------------------------------------------- fixed lines
# The lines printed by pause, done and release-line: the user reads them in the
# orchestrator's report, copied as they are.
# lang: fr begin
def colour_line(kind, *a):
    if kind == "freed":
        return "Couleur %s libérée." % a[0]
    if kind == "kept":
        return "Couleur %s reste prise : %s" % (a[0], a[1])
    if kind == "nocolour":
        return "Ce dossier ne tenait aucune couleur."
    if kind == "paused":
        return "Paquet #%s en pause : couleur %s libérée." % (a[0], a[1])
    if kind == "paused-issue":
        return "Dossier #%s en pause : couleur %s libérée." % (a[0], a[1])
    if kind == "paused-already":
        return "#%s déjà en pause (aucune couleur prise)." % a[0]
    if kind == "done-paused":
        return "Paquet #%s terminé (il n'avait plus de couleur)." % a[0]
    if kind == "done-issue":
        return "Dossier #%s terminé (il n'avait plus de couleur)." % a[0]
    if kind == "reason-package":
        return "paquet #%s, prochaine sous-issue ; après la dernière : scripts/chantier.sh --done %s" % (a[0], a[0])
    if kind == "reason-qcm":
        return "un QCM de l'issue est ouvert"
    if kind == "reason-legacy":
        return "état à confirmer par sa session"
    if kind == "reason-trial":
        return "un essai de l'issue est en attente"
    raise ValueError(kind)
# lang: fr end


# ------------------------------------------------------------------- paths
def git(*args, cwd=None):
    r = subprocess.run(["git", *args], capture_output=True, text=True, cwd=cwd)
    return r.stdout.strip() if r.returncode == 0 else ""


def git_rc(*args, cwd=None):
    """(exit code, stdout) of a git command: git() hides the exit code."""
    r = subprocess.run(["git", *args], capture_output=True, text=True, cwd=cwd)
    return r.returncode, r.stdout


def main_dir():
    common = git("rev-parse", "--path-format=absolute", "--git-common-dir")
    return os.path.dirname(common) if common else ""


def suivi_dir():
    env = os.environ.get("SF_SUIVI_DIR")
    if env:
        return os.path.abspath(env)
    m = main_dir()
    if not m:
        raise Fail("not inside the StokesForge repository (and SF_SUIVI_DIR is not set)")
    return os.path.abspath(os.path.join(os.path.dirname(m), "_stokesforge", ".suivi"))


def lock_dir():
    return os.path.join(os.path.dirname(suivi_dir()), ".colour.lock")


def worktrees():
    """Paths of the linked worktrees (every one but the main checkout)."""
    m = main_dir()
    if not m:
        return []
    out = git("-C", m, "worktree", "list", "--porcelain")
    paths = [l[len("worktree "):] for l in out.splitlines() if l.startswith("worktree ")]
    return [p for p in paths if os.path.realpath(p) != os.path.realpath(m)]


def issue_of(path):
    """Issue number of a worktree: folder feature<n><letters>, else the branch."""
    mt = re.match(r"^feature(\d+)[a-z]*$", os.path.basename(path))
    if mt:
        return int(mt.group(1))
    br = git("-C", path, "symbolic-ref", "--short", "HEAD")
    mt = re.match(r"^feature/(\d+)-", br)
    return int(mt.group(1)) if mt else None


def today():
    t = os.environ.get("SF_TODAY")
    return datetime.date.fromisoformat(t) if t else datetime.date.today()


# ------------------------------------------------------------------ storage
def write_json(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = os.path.join(os.path.dirname(path), ".%s.tmp.%d" % (os.path.basename(path), os.getpid()))
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=1)
        f.write("\n")
    os.replace(tmp, path)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def load_all():
    recs = {}
    for p in sorted(glob.glob(os.path.join(suivi_dir(), "issues", "*.json"))):
        r = read_json(p)
        if isinstance(r, dict) and str(r.get("issue", "")).isdigit():
            r["issue"] = int(r["issue"])
            recs[r["issue"]] = r
    return recs


def save(r):
    write_json(os.path.join(suivi_dir(), "issues", "%d.json" % r["issue"]), r)


class Lock:
    """The colour lock (mkdir is atomic; waits up to 10 s)."""

    def __enter__(self):
        d = lock_dir()
        os.makedirs(os.path.dirname(d), exist_ok=True)
        for _ in range(int(float(os.environ.get("SF_SUIVI_LOCK_WAIT", "10")) * 10)):
            try:
                os.mkdir(d)
                return self
            except FileExistsError:
                time.sleep(0.1)
        try:
            age = int(time.time() - os.stat(d).st_mtime)
        except OSError:
            age = -1
        raise Fail("lock %s is held (age %d s); remove it if no script is running" % (d, age))

    def __exit__(self, *exc):
        try:
            os.rmdir(lock_dir())
        except OSError:
            pass


# -------------------------------------------------------------- held / owner
def pid_start(pid):
    """Start time (field 22 of /proc/<pid>/stat), None if gone or a zombie."""
    try:
        with open("/proc/%d/stat" % int(pid)) as f:
            s = f.read()
        rest = s[s.rindex(")") + 2:].split()
        if rest[0] == "Z":
            return None
        return int(rest[19])
    except (OSError, ValueError, IndexError):
        return None


def owner_alive(r):
    o = r.get("owner")
    if not o:
        return False
    st = pid_start(o.get("pid", -1))
    if st is None or st != o.get("pid_start"):
        return False
    sid = str(o.get("session", ""))
    if sid and "/" not in sid:
        try:
            if os.stat(os.path.join(suivi_dir(), "ended", sid)).st_mtime > float(o.get("at", 0)):
                return False
        except OSError:
            pass
    return True


def qcm_open(n):
    for p in glob.glob(os.path.join(suivi_dir(), "qcm", "*", "meta.json")):
        m = read_json(p)
        if isinstance(m, dict) and str(m.get("issue")) == str(n) and not m.get("finished"):
            return True
    return False


def trial_pending(n):
    for p in glob.glob(os.path.join(suivi_dir(), "tests", "%s-*.json" % n)):
        t = read_json(p)
        if isinstance(t, dict) and t.get("status") == "todo" and t.get("try"):
            return True
    return False


def held_by(r):
    """Why the issue holds its colour: a list of 'agent', 'qcm', 'essai' (empty = paused)."""
    if r.get("done_at") or not r.get("colour"):
        return []
    why = []
    if owner_alive(r):
        why.append("agent")
    if qcm_open(r["issue"]):
        why.append("qcm")
    if trial_pending(r["issue"]):
        why.append("essai")
    if r.get("legacy"):
        why.append("legacy")
    return why


def claude_projects():
    return os.environ.get("SF_CLAUDE_PROJECTS") or os.path.expanduser("~/.claude/projects")


def agent_idle_min(r):
    """Whole minutes since the owner session last wrote (newest mtime of its transcript and
    its subagents' transcripts), None if the owner is not alive or no file is found. Stat
    only: the transcripts are private and never read. Whole minutes keep the ETag steady."""
    if not owner_alive(r):
        return None
    sid = str((r.get("owner") or {}).get("session", ""))
    if not sid or "/" in sid or sid.startswith("."):
        return None
    base = claude_projects()
    paths = glob.glob(os.path.join(glob.escape(base), "*", glob.escape(sid) + ".jsonl"))
    paths += glob.glob(os.path.join(glob.escape(base), "*", glob.escape(sid), "subagents", "*.jsonl"))
    newest = None
    for p in paths:
        try:
            m = os.stat(p).st_mtime
        except OSError:
            continue
        newest = m if newest is None else max(newest, m)
    return None if newest is None else max(0, int((time.time() - newest) // 60))


def group_of(r):
    return r.get("parent") or r["issue"]


def pick_colour(recs, r, avoid=()):
    """Previous colour if free, else the first free; None if all six are held."""
    taken = {x["colour"] for x in recs.values() if group_of(x) != group_of(r) and held_by(x)}
    taken |= set(avoid)
    prev = r.get("colour") or r.get("last_colour")
    if prev in COLOURS and prev not in taken:
        return prev
    return next((c for c in COLOURS if c not in taken), None)


def all_taken():
    return Fail("6 work packages in progress (%s): finish or remove one first (git worktree list)" % "".join(COLOURS))


# ------------------------------------------------------------------- claim
def find_claude_pid():
    pid = os.getppid()
    for _ in range(12):
        try:
            with open("/proc/%d/comm" % pid) as f:
                comm = f.read().strip()
            with open("/proc/%d/stat" % pid) as f:
                ppid = int(f.read().rsplit(")", 1)[1].split()[1])
        except (OSError, ValueError, IndexError):
            return None
        if "claude" in comm.lower():
            return pid
        if ppid <= 1:
            return None
        pid = ppid
    return None


def current_owner():
    sid = os.environ.get("CLAUDE_CODE_SESSION_ID")
    if not sid:
        return None
    pid = os.environ.get("CLAUDE_PID")
    pid = int(pid) if pid and pid.isdigit() else find_claude_pid()
    start = pid_start(pid) if pid else None
    if start is None:
        return None
    return {"session": sid, "pid": pid, "pid_start": start, "at": time.time()}


def new_record(n):
    return {"issue": n, "kind": "issue", "parent": None, "subs": [], "name": "", "colour": None,
            "last_colour": None, "state": "questions", "waiting": "", "wait_kind": "", "what": "",
            "since": None, "owner": None, "done_at": None, "legacy": False}


def claim_into(recs, n, owner, name=None, parent=None, subs=None, package=False, avoid=()):
    """Claim issue n in `recs` (memory only). Returns the changed records."""
    changed = []
    if parent is not None:
        p, pc = claim_into(recs, parent, owner, package=True, avoid=avoid)
        changed += pc
        r = recs.get(n) or new_record(n)
        r["parent"] = parent
        colour = p["colour"]
    else:
        r = recs.get(n) or new_record(n)
        colour = pick_colour(recs, r, avoid)
        if colour is None:
            raise all_taken()
    if r.get("colour") != colour or not held_by(r):
        r["since"] = today().isoformat()
    r["colour"] = colour
    r["done_at"] = None
    r["legacy"] = False
    if package or subs is not None:
        r["kind"] = "package"
    if subs is not None:
        r["subs"] = subs
    if name:
        r["name"] = name
    if owner:
        r["owner"] = owner
    recs[n] = r
    return r, changed + [r]


def cmd_claim(a):
    owner = current_owner()
    if owner is None and not a.dry_run:
        sys.stderr.write("suivi: no Claude session (CLAUDE_CODE_SESSION_ID / CLAUDE_PID): "
                         "#%d will not be held by an agent\n" % a.issue)
    subs = None
    if a.sub:
        subs = []
        for s in a.sub:
            m, _, t = s.partition("=")
            subs.append({"n": int(m), "title": t})
    avoid = a.avoid.split() if a.avoid else []
    if a.dry_run:
        recs = load_all()
        r, _ = claim_into(recs, a.issue, owner, a.name, a.parent, subs, a.package or bool(subs), avoid)
        print(r["colour"])
        return 0
    with Lock():
        recs = load_all()
        time.sleep(float(os.environ.get("SF_SUIVI_SLOW", "0")))   # test seam: widens the race window
        r, changed = claim_into(recs, a.issue, owner, a.name, a.parent, subs, a.package or bool(subs), avoid)
        for x in changed:
            save(x)
    print(r["colour"])
    return 0


# --------------------------------------------------------------------- set
def need(recs, n):
    if n not in recs:
        raise Fail("no work package #%d (run scripts/chantier.sh %d first)" % (n, n))
    return recs[n]


def cmd_set(a):
    with Lock():
        recs = load_all()
        r = need(recs, a.issue)
        # state and what are kept when not passed. waiting and wait_kind are an agent note:
        # the next `set` erases them unless it restates them (E1, #449).
        for key, val in (("state", a.state), ("what", a.what)):
            if val is not None:
                r[key] = val
        r["waiting"] = a.waiting or ""
        r["wait_kind"] = a.wait or ""
        save(r)
    return 0


# ------------------------------------------------------------ pause / done
def free_colour(r):
    r["last_colour"] = r.get("colour") or r.get("last_colour")
    r["colour"] = None


def cmd_pause(a):
    with Lock():
        recs = load_all()
        r = need(recs, a.issue)
        was = r.get("colour")
        r["owner"] = None
        r["legacy"] = False
        if not held_by(r):
            free_colour(r)
        save(r)
        label = "paused" if r.get("kind") == "package" else "paused-issue"
        if r.get("colour"):
            print(colour_line("kept", r["colour"], why_text(r)))
        elif was:
            print(colour_line(label, a.issue, was))
        else:
            print(colour_line("paused-already", a.issue))
    return 0


def why_text(r):
    return ", ".join({"agent": "agent", "qcm": colour_line("reason-qcm"), "essai": colour_line("reason-trial"),
                      "legacy": colour_line("reason-legacy")}[w]
                     for w in held_by(r))


def cmd_done(a):
    with Lock():
        recs = load_all()
        r = need(recs, a.issue)
        had = bool(held_by(r))
        colour = r.get("colour")
        r["done_at"] = today().isoformat()
        r["owner"] = None
        r["legacy"] = False
        free_colour(r)
        save(r)
        if had and colour:
            print(colour_line("freed", colour))
        else:
            print(colour_line("done-paused" if r.get("kind") == "package" else "done-issue", a.issue))
    return 0


def cmd_is_package(a):
    r = load_all().get(a.issue)
    return 0 if r and r.get("kind") == "package" else 1


def members_of(recs, n):
    ids = [n] + [x["issue"] for x in recs.values() if x.get("parent") == n]
    ids += [s["n"] for s in (recs.get(n) or {}).get("subs", [])]
    return sorted(set(ids))


def cmd_members(a):
    print(" ".join(str(i) for i in members_of(load_all(), a.issue)))
    return 0


def cmd_release_line(a):
    """The line printed after a worktree of issue n was removed (call it AFTER the removal)."""
    with Lock():
        recs = load_all()
        r = recs.get(a.issue)
        if r is None or not (r.get("colour") or r.get("last_colour")):
            print(colour_line("nocolour"))
            return 0
        colour = r.get("colour") or r.get("last_colour")
        sibs = [os.path.basename(w) for w in worktrees() if issue_of(w) == a.issue]
        reason = None
        blocking = False
        if sibs:
            reason, blocking = ", ".join(sibs), True
        elif qcm_open(a.issue):
            reason, blocking = colour_line("reason-qcm"), True
        elif trial_pending(a.issue):
            reason, blocking = colour_line("reason-trial"), True
        elif r.get("parent") and r["parent"] in recs and held_by(recs[r["parent"]]):
            reason = colour_line("reason-package", r["parent"])
        if a.mark_done and not blocking:
            r["done_at"] = today().isoformat()
            r["owner"] = None
            r["legacy"] = False
            free_colour(r)
            save(r)
        print(colour_line("kept", colour, reason) if reason else colour_line("freed", colour))
    return 0


# ------------------------------------------------------------------ status
def days_since(iso):
    try:
        d = datetime.date.fromisoformat(iso)
    except (TypeError, ValueError):
        return iso, 0
    return iso, (today() - d).days


def build_status():
    recs = load_all()
    wts = worktrees()
    by_issue = {}
    colourless = []
    for w in wts:
        n = issue_of(w)
        if n in recs:
            by_issue.setdefault(n, []).append(os.path.basename(w))
        else:
            colourless.append(os.path.basename(w))
    members = {}
    for r in recs.values():
        members.setdefault(group_of(r), []).append(r)

    def group_wts(g):
        return sorted(w for m in members.get(g, []) for w in by_issue.get(m["issue"], []))

    colours = []
    for c in COLOURS:
        held = [r for r in recs.values() if r.get("colour") == c and held_by(r)]
        if not held:
            colours.append({"colour": c, "free": True})
            continue
        groups = sorted({group_of(r) for r in held})
        g = groups[0]
        gh = [r for r in held if group_of(r) == g]
        parent = recs.get(g)
        pkg = parent if parent and parent.get("kind") == "package" else None
        lead = next((r for r in gh if r["issue"] != g), gh[0])
        since, days = days_since(min(r.get("since") or "9999" for r in gh))
        why = sorted({w for r in gh for w in held_by(r)})
        holder = next((r for r in gh if "agent" in held_by(r)), None)
        colours.append({
            "colour": c, "free": False, "issue": g, "issues": groups,
            "package": {"parent": g, "subs": pkg.get("subs", [])} if pkg else None,
            "name": (pkg or lead).get("name", ""), "state": lead.get("state", ""),
            "what": lead.get("what", ""), "waiting": lead.get("waiting", ""), "wait_kind": lead.get("wait_kind", ""),
            "held_by": why, "agent_idle_min": agent_idle_min(holder) if holder else None,
            **wait_view(recs, g),
            "worktrees": group_wts(g), "since": since, "days": days})
    paused, done = [], []
    for r in recs.values():
        if r.get("done_at"):
            done.append({"issue": r["issue"], "name": r.get("name", ""), "done_at": r["done_at"]})
        elif not held_by(r) and r.get("parent") is None:
            since, days = days_since(r.get("since"))
            paused.append({"issue": r["issue"], "kind": r.get("kind", "issue"), "name": r.get("name", ""),
                           "last_colour": r.get("colour") or r.get("last_colour"), "subs": r.get("subs", []),
                           "what": r.get("what", ""), "agent_idle_min": agent_idle_min(r),
                           **wait_view(recs, r["issue"]),
                           "worktrees": group_wts(r["issue"]), "since": since, "days": days})
        elif not held_by(r):
            since, days = days_since(r.get("since"))
            paused.append({"issue": r["issue"], "kind": "issue", "parent": r["parent"], "name": r.get("name", ""),
                           "last_colour": r.get("colour") or r.get("last_colour"), "subs": [],
                           "what": r.get("what", ""), "agent_idle_min": agent_idle_min(r),
                           **wait_view(recs, group_of(r)),
                           "worktrees": by_issue.get(r["issue"], []), "since": since, "days": days})
    done.sort(key=lambda d: (d["done_at"], d["issue"]), reverse=True)
    return {"colours": colours, "paused": sorted(paused, key=lambda p: p["issue"]),
            "colourless": sorted(colourless), "done": done}


def cmd_status(a):
    s = build_status()
    if a.json:
        json.dump(s, sys.stdout, ensure_ascii=False, indent=1)
        print()
        return 0
    for e in s["colours"]:
        if e["free"]:
            print(e["colour"], "free")
            continue
        if e["package"]:
            who = "package #%d (%s)" % (e["package"]["parent"], ", ".join("#%d" % x["n"] for x in e["package"]["subs"]))
        else:
            who = "#%d" % e["issue"]
        flag = "  CONFLICT: " + ", ".join("#%d" % i for i in e["issues"]) if len(e["issues"]) > 1 else ""
        parts = [e["colour"], who, e["name"], "[%s]" % e["state"] if e["state"] else "",
                 "held by " + "+".join(e["held_by"]), " ".join(e["worktrees"]),
                 "since", e["since"], "(%d d)" % e["days"] + flag]
        print(" ".join(p for p in parts if p))
    if s["colourless"]:
        print("no colour:", " ".join(s["colourless"]))
    for p in s["paused"]:
        what = "package #%d" % p["issue"] if p["kind"] == "package" else "#%d" % p["issue"]
        print("paused: %s %s since %s (%d d) %s" % (what, p["name"], p["since"], p["days"], " ".join(p["worktrees"])))
    return 0


# ------------------------------------------------------------------- check
def cmd_check(a):
    recs = load_all()
    wts = worktrees()
    problems = []
    seen = {}
    for r in recs.values():
        if held_by(r):
            seen.setdefault(r["colour"], set()).add(group_of(r))
    for c, groups in seen.items():
        if len(groups) > 1:
            problems.append("colour %s is held by %s" % (c, ", ".join("#%d" % g for g in sorted(groups))))
    wt_issues = {issue_of(w) for w in wts}
    for r in recs.values():
        if r.get("kind") != "package" and not r.get("done_at") and r["issue"] not in wt_issues:
            problems.append("issue #%d has a record but no worktree and is not done" % r["issue"])
    for w in wts:
        if issue_of(w) not in recs:
            problems.append("worktree %s has no record" % os.path.basename(w))
    owner = os.environ.get("CLAUDE_CODE_SESSION_ID") or "cli"
    write_json(os.path.join(suivi_dir(), "control.json"),
               {"at": datetime.datetime.now().isoformat(timespec="seconds"), "by": owner, "problems": problems})
    for p in problems:
        print(p)
    return 1 if problems else 0


# ----------------------------------------------------------------- migrate
def cmd_migrate(a):
    """Create the records of the old layout (git-dir colour files, .packages), once."""
    with Lock():
        recs = load_all()
        made = []
        pdir = os.path.join(os.path.dirname(suivi_dir()), ".packages")
        member_of = {}
        for p in sorted(glob.glob(os.path.join(pdir, "*"))):
            if not os.path.isfile(p) or not os.path.basename(p).isdigit():
                continue
            n = int(os.path.basename(p))
            lines = open(p, encoding="utf-8").read().split("\n")
            colour = lines[0].strip() or None
            subs = []
            for l in lines[1:]:
                m, _, t = l.partition("\t")
                if m.isdigit():
                    subs.append({"n": int(m), "title": t})
                    member_of[int(m)] = n
            since = datetime.date.fromtimestamp(os.path.getmtime(p)).isoformat()
            if n not in recs:
                r = new_record(n)
                r.update(kind="package", subs=subs, colour=colour, last_colour=colour, since=since,
                         legacy=bool(colour))
                recs[n] = r
                save(r)
                made.append(n)
        for w in worktrees():
            n = issue_of(w)
            if n is None or n in recs:
                continue
            f = os.path.join(git("-C", w, "rev-parse", "--absolute-git-dir"), "stokesforge-colour")
            if not os.path.isfile(f):
                continue
            colour = open(f, encoding="utf-8").read().strip() or None
            r = new_record(n)
            # Other sessions may be running with no owner recorded: hold the colour until
            # the issue is claimed, paused or done (the worktree is live, we just listed it).
            r.update(parent=member_of.get(n), colour=colour, last_colour=colour, legacy=bool(colour),
                     since=datetime.date.fromtimestamp(os.path.getmtime(f)).isoformat())
            recs[n] = r
            save(r)
            made.append(n)
    print("migrated: %s" % (" ".join("#%d" % n for n in made) or "nothing to do"))
    return 0


# ------------------------------------------------------------- session-end
def cmd_session_end(a):
    """SessionEnd hook: touch ended/<session_id>. Never fails, takes no lock."""
    try:
        data = json.loads(sys.stdin.read())
        sid = str(data["session_id"])
        if not re.fullmatch(r"[A-Za-z0-9._-]+", sid) or sid in (".", ".."):
            return 0
        d = os.path.join(suivi_dir(), "ended")
        if not os.path.isdir(os.path.dirname(d)):
            return 0
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, sid)
        with open(p, "a"):
            pass
        os.utime(p, None)
    except Exception:
        pass
    return 0


# -------------------------------------------------------------------- wait
WAIT_TIMEOUT = 72 * 3600     # wall clock, then the wait says so and the agent re-arms
BASE_FP = "todo|"            # fingerprint of a trial entry nobody answered: not an event


def wait_poll():
    try:
        return max(0.01, float(os.environ.get("SF_SUIVI_WAIT_POLL", "5")))
    except ValueError:
        return 5.0


def wait_settle():
    try:
        return max(0.0, float(os.environ.get("SF_SUIVI_WAIT_SETTLE", "180")))
    except ValueError:
        return 180.0


def should_wake(events, todos, last_change, now, settle):
    """THE wake policy (#454), the only place that decides. `events` are the unseen ones
    (kind, status, comment), `todos` the watched `try` entries still `todo`, `last_change` the time
    of the latest change of any watched entry, `settle` the delay in seconds.
    Wake only when the agent has something to do: a finished form, or no `todo` left (at once);
    a Retour or a comment while entries remain `todo` (settle after the last change of any entry,
    each change restarting it). A plain OK with `todo` left wakes nothing and stays unseen."""
    if not events:
        return False
    if any(e["kind"] == "form" for e in events) or todos == 0:
        return True
    if any(e["status"] == "retour" or e["comment"] for e in events):
        return now - last_change >= settle
    return False


def waits_dir():
    return os.path.join(suivi_dir(), "waits")


def heard_path(root):
    return os.path.join(suivi_dir(), "heard", "%d.json" % root)


def load_heard(root, sid):
    d = read_json(heard_path(root))
    if isinstance(d, dict) and sid and d.get("session") == sid and isinstance(d.get("seen"), dict):
        return d["seen"]
    return {}


def save_heard(root, sid, seen):
    write_json(heard_path(root), {"session": sid, "seen": seen})


def scan_group(members):
    """(items, open_forms, open_todos) for the issues in `members`. items: key -> {id, fp, line}."""
    items, forms, todos = {}, [], []
    for m in members:
        for p in sorted(glob.glob(os.path.join(tests_dir(), "%s-*.json" % m)), key=natural):
            tid = os.path.basename(p)[:-5]
            t = read_json(p)
            if not safe_id(tid) or not isinstance(t, dict) or "try" not in t:
                continue
            status, comment = t.get("status", "todo"), str(t.get("comment") or "").strip()
            if status == "todo":
                todos.append(tid)
            try:
                mtime = os.stat(p).st_mtime
            except OSError:
                mtime = 0.0
            items[tid] = {"id": tid, "kind": "test", "status": status, "comment": comment, "mtime": mtime,
                          "fp": "%s|%s" % (status, comment), "line": "%s %s%s" % (tid, status, ": " + comment if comment else "")}
        for p in glob.glob(os.path.join(suivi_dir(), "qcm", "%s-*" % m, "meta.json")):
            qid = os.path.basename(os.path.dirname(p))
            meta = read_json(p)
            if not safe_id(qid) or not isinstance(meta, dict) or not re.fullmatch(r"%d-\d+" % m, qid):
                continue
            if not meta.get("finished"):
                forms.append(qid)
                continue
            ans = load_answers(qid)
            digest = hashlib.sha1(json.dumps(ans, sort_keys=True).encode("utf-8")).hexdigest()[:16]
            pr = qcm_progress(qid, meta)
            items["qcm:" + qid] = {"id": "qcm:" + qid, "kind": "form", "status": "", "comment": "", "mtime": 0.0,
                                   "fp": "finished|" + digest,
                                   "line": "form %s finished, %d/%d answered, read with: suivi.py qcm answers %s"
                                           % (qid, pr["answered"], pr["total"], qid)}
    return items, forms, todos


def unseen_events(items, seen):
    out = []
    for key in sorted(items, key=natural):
        it = items[key]
        if seen.get(key, None if key.startswith("qcm:") else BASE_FP) != it["fp"]:
            out.append(it)
    return out


def owner_session(b):
    return str(((b or {}).get("owner") or {}).get("session", ""))


def wait_context(n, sid=None):
    """(session id, root, records, base record). Fails when there is no record or the session is not the owner."""
    sid = os.environ.get("CLAUDE_CODE_SESSION_ID") or "" if sid is None else sid
    recs = load_all()
    r = recs.get(n)
    if r is None:
        raise Fail("no record for #%d (run scripts/chantier.sh %d first)" % (n, n))
    root = group_of(r)
    b = recs.get(root) or r
    if not sid or owner_session(b) != sid:
        raise Fail("this session does not own #%d: scripts/chantier.sh %d claims it" % (n, root))
    return sid, root, recs, b


def live_wait(root):
    """The live registration of a wait on `root`, or None (stale = pid_start mismatch)."""
    reg = read_json(os.path.join(waits_dir(), "%d.json" % root))
    if not isinstance(reg, dict):
        return None
    st = pid_start(reg.get("pid", -1))
    return reg if st is not None and st == reg.get("pid_start") else None


def listening(recs, root):
    sid = owner_session(recs.get(root))
    reg = live_wait(root)
    return bool(sid and reg and reg.get("session") == sid)


def wait_view(recs, root):
    """Computed `listening` and `unread` of a group: steady values, no timestamp (ETag)."""
    items, _, _ = scan_group(members_of(recs, root))
    sid = owner_session(recs.get(root))
    return {"listening": listening(recs, root), "unread": len(unseen_events(items, load_heard(root, sid)))}


def exit_reason(b, sid):
    """The word and sentence of an exit decided from the record alone, or None."""
    end = "nothing to do, end the turn without a message."
    if b is None:
        return "paused: the record is gone; " + end
    if b.get("done_at"):
        return "done: the issue is done; " + end
    if not b.get("owner"):
        return "paused: the issue is paused; " + end
    if owner_session(b) != sid:
        return "taken: another session owns the issue; " + end
    if not owner_alive(b):
        return "session gone: the owner session has ended; " + end
    return None


def open_summary(n, forms, todos, hook=False):
    parts = []
    if forms:
        parts.append("%d open form%s" % (len(forms), "" if len(forms) == 1 else "s"))
    if todos:
        parts.append("%d trial entr%s to tick" % (len(todos), "y" if len(todos) == 1 else "ies"))
    if not parts:
        return "Nothing else is open."
    if hook:
        return ("Still open: %s. The wait resumes by itself at your next suivi.py call on #%d." % (", ".join(parts), n))
    return "Still open: %s. Re-arm: python3 scripts/suivi.py wait %d" % (", ".join(parts), n)


def record_events(root, sid, events):
    seen = load_heard(root, sid)
    for e in events:
        seen[e["id"]] = e["fp"]
    save_heard(root, sid, seen)


def report_events(n, root, sid, events, forms, todos):
    record_events(root, sid, events)
    for e in events:
        print(e["line"])
    print(open_summary(n, forms, todos), flush=True)


def log_wait(text):
    try:
        os.makedirs(waits_dir(), exist_ok=True)
        with open(os.path.join(waits_dir(), "log"), "a", encoding="utf-8") as f:
            f.write("%s %s\n" % (datetime.datetime.now().isoformat(timespec="seconds"), text))
    except Exception:
        pass


def cmd_wait(a):
    os.environ["SF_SUIVI_DIR"] = suivi_dir()      # frozen: no git fork per tick, survives a removed cwd
    kind, text = wait_session(a.issue, os.environ.get("CLAUDE_CODE_SESSION_ID") or "", a.timeout, False)
    print(text, flush=True)
    return 0


def wait_session(n, sid, timeout, hook):
    """Register, wait, unregister. Returns (kind, text): kind `event` (text = the lines to report,
    already recorded) or `end` (an exit line), or `already`. Fails when the session is not the owner."""
    sid, root, recs, _ = wait_context(n, sid)
    me = os.getpid()
    mine = os.path.join(waits_dir(), "%d.json" % root)
    with Lock():
        reg = live_wait(root)
        if reg is not None and reg.get("pid") != me:
            if reg.get("session") == sid:
                return "already", ("already listening: a wait on #%d is running (pid %s); nothing to do."
                                   % (root, reg.get("pid")))
            raise Fail("another session already waits on #%d" % root)
        write_json(mine, {"pid": me, "pid_start": pid_start(me), "session": sid, "started": time.time()})

    def on_signal(signum, frame):
        log_wait("%s pid=%d #%d session=%s" % (signal.Signals(signum).name, me, root, sid))
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGHUP, on_signal)
    try:
        return wait_loop(n, sid, root, timeout, hook)
    finally:
        reg = read_json(mine)
        if isinstance(reg, dict) and reg.get("pid") == me:
            try:
                os.unlink(mine)
            except OSError:
                pass


def wait_loop(n, sid, root, timeout, hook):
    started = time.time()
    poll, settle = wait_poll(), wait_settle()
    timeout = WAIT_TIMEOUT if timeout is None else timeout
    while True:
        recs = load_all()
        b = recs.get(root) or recs.get(n)
        line = exit_reason(b, sid)
        if line:
            return "end", line
        items, forms, todos = scan_group(members_of(recs, root))
        events = unseen_events(items, load_heard(root, sid))
        now = time.time()
        last_change = max([it["mtime"] for it in items.values() if it["kind"] == "test"], default=0.0)
        if should_wake(events, len(todos), last_change, now, settle):
            record_events(root, sid, events)          # just before reporting, never earlier
            return "event", "\n".join([e["line"] for e in events] + [open_summary(n, forms, todos, hook)])
        if not events and not forms and not todos:
            return "end", ("nothing open: no open form, no trial entry to tick, nothing unread; "
                           "nothing to do, end the turn without a message.")
        if now - started >= timeout:
            return "end", ("timeout: nothing happened for %d h. Re-arm: python3 scripts/suivi.py wait %d"
                           % (max(1, int(timeout // 3600)), n))
        time.sleep(min(poll, max(0.01, started + timeout - now)))


HOOK_CMD_RE = re.compile(r"suivi\.py\s+(?:(?:qcm\s+(?:pose|answers)|test\s+(?:add|remove|merged)|set|news)\s+(\d+)"
                         r"|test\s+list\s+--issue\s+(\d+))")


def hook_issue(command):
    """The issue number of a `suivi.py` call in a shell command, or None."""
    m = HOOK_CMD_RE.search(command or "")
    return int(m.group(1) or m.group(2)) if m else None


def cmd_hook_wait(a):
    """PostToolUse (Bash, asyncRewake) hook: arm the wait after a `suivi.py` call on an issue this
    session owns. Exit 2 only for an event, the text on STDERR (that wakes the model); every other
    outcome is exit 0 and silent, so no turn is wasted. Never exits 2 by accident."""
    try:
        data = json.loads(sys.stdin.read())
        sid = str(data["session_id"])
        n = hook_issue(data["tool_input"]["command"])
        d = fast_state_dir()
        if n is None or not sid or not d or not os.path.isdir(os.path.join(d, "issues")):
            return 0
        os.environ["SF_SUIVI_DIR"] = d
        sid, root, recs, _ = wait_context(n, sid)
        items, forms, todos = scan_group(members_of(recs, root))
        if not (forms or todos or unseen_events(items, load_heard(root, sid))) or live_wait(root) is not None:
            return 0
        kind, text = wait_session(n, sid, None, True)
        if kind == "event":
            sys.stderr.write(text + "\n")
            sys.stderr.flush()
            return 2
    except BaseException:
        pass
    return 0


def cmd_news(a):
    sid, root, recs, _ = wait_context(a.issue)
    items, forms, todos = scan_group(members_of(recs, root))
    events = unseen_events(items, load_heard(root, sid))
    if not events:
        print("no news. " + open_summary(a.issue, forms, todos))
        return 0
    report_events(a.issue, root, sid, events, forms, todos)
    return 0


def hint_not_listening(n):
    recs = load_all()
    r = recs.get(n)
    root = group_of(r) if r else n
    if not listening(recs, root):
        sys.stderr.write("suivi: nobody is listening for #%d yet: the PostToolUse hook arms the wait after "
                         "this command (no hook in this session: run python3 scripts/suivi.py wait %d "
                         "in the background)\n" % (n, n))


def fast_state_dir():
    """The state folder without a git call, from $CLAUDE_PROJECT_DIR (main checkout or a worktree
    beside it); None when it cannot be told."""
    if os.environ.get("SF_SUIVI_DIR"):
        return os.environ["SF_SUIVI_DIR"]
    proj = os.environ.get("CLAUDE_PROJECT_DIR")
    if not proj:
        return None
    proj = os.path.abspath(proj)
    if re.fullmatch(r"feature\d+[a-z]*", os.path.basename(proj)) and os.path.basename(os.path.dirname(proj)) == "_stokesforge":
        return os.path.join(os.path.dirname(proj), ".suivi")
    return os.path.join(os.path.dirname(proj), "_stokesforge", ".suivi")


def cmd_hook_news(a):
    """SessionStart / UserPromptSubmit hook: reads {session_id, ...} on stdin. Never fails (exit 2
    would block the user's message), takes no lock, spends no git call."""
    try:
        sid = str(json.loads(sys.stdin.read())["session_id"])
        d = fast_state_dir()
        if not sid or not d or not os.path.isdir(os.path.join(d, "issues")):
            return 0
        os.environ["SF_SUIVI_DIR"] = d
        recs = load_all()
        roots = {}
        for r in recs.values():
            if not r.get("done_at"):
                roots.setdefault(group_of(r), recs.get(group_of(r)) or r)
        mine = sorted(g for g, b in roots.items() if owner_session(b) == sid)
        if mine:
            for g in mine:
                items, forms, todos = scan_group(members_of(recs, g))
                events = unseen_events(items, load_heard(g, sid))
                if events:
                    report_events(g, g, sid, events, forms, todos)
                if (forms or todos) and live_wait(g) is None:
                    print("no wait running for #%d: any suivi.py call on the issue re-arms it, for example "
                          "suivi.py news %d" % (g, g))
        else:
            late = []
            for g, b in sorted(roots.items()):
                if b.get("owner") and owner_alive(b):
                    continue
                items, _, _ = scan_group(members_of(recs, g))
                if unseen_events(items, load_heard(g, owner_session(b))):
                    late.append("#%d" % g)
            if late:
                print("Answers wait for %s (no live session owns them): lance <n> reads them." % ", ".join(late))
    except Exception:
        pass
    return 0


# ------------------------------------------------------------ trial entries
ID_RE = re.compile(r"[A-Za-z0-9_.-]+")
TEST_FIELDS = {"title": str, "intro": str, "where": list, "steps": list, "expect": str, "try": str, "commit": str,
               "date": str, "colour": str, "name": str}
TEST_STATUSES = ("todo", "ok", "retour")
MAX_UNMERGED = 5      # E2: trial entries with `try` per issue, unless `test add --force`


def safe_id(s):
    """An id that is one plain file name: no slash, no leading dot."""
    return bool(s) and ID_RE.fullmatch(s) is not None and not s.startswith(".")


def tests_dir():
    return os.path.join(suivi_dir(), "tests")


def test_path(tid):
    return os.path.join(tests_dir(), tid + ".json")


def load_tests(issue=None):
    out = []
    for p in sorted(glob.glob(os.path.join(tests_dir(), "*.json"))):
        tid = os.path.basename(p)[:-5]
        t = read_json(p)
        if not safe_id(tid) or not isinstance(t, dict):
            continue
        if issue is not None and not tid.startswith("%s-" % issue):
            continue
        out.append(dict(t, id=tid))
    return out


def natural(tid):
    return [int(x) if x.isdigit() else x for x in re.split(r"(\d+)", tid)]


def check_entry(e):
    if not isinstance(e, dict):
        raise Fail("a trial entry must be a JSON object")
    for k, v in e.items():
        if k not in TEST_FIELDS:
            raise Fail("unknown field %r (allowed: %s)" % (k, ", ".join(sorted(TEST_FIELDS))))
        if not isinstance(v, TEST_FIELDS[k]):
            raise Fail("field %r must be a %s" % (k, TEST_FIELDS[k].__name__))
        if isinstance(v, list) and not all(isinstance(x, str) for x in v):
            raise Fail("field %r must be a list of strings" % k)


def removed_dir():
    return os.path.join(suivi_dir(), "removed-tests")


def used_numbers(issue):
    """Numbers already taken for the issue: live entries and the archive of removed ones."""
    out = []
    for t in load_tests(issue):
        out.append(t["id"])
    for p in glob.glob(os.path.join(removed_dir(), "%d-*.json" % issue)):
        out.append(os.path.basename(p)[:-5])
    return [int(m.group(1)) for tid in out for m in [re.fullmatch(r"%d-(\d+)" % issue, tid)] if m]


def cmd_test_add(a):
    try:
        with open(a.file, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise Fail("cannot read %s: %s" % (a.file, e))
    entries = data if isinstance(data, list) else [data]
    for e in entries:
        check_entry(e)
    rec = load_all().get(a.issue) or {}
    ids = []
    with Lock():
        unmerged = sum(1 for t in load_tests(a.issue) if "try" in t)
        incoming = sum(1 for e in entries if "try" in e)
        if unmerged + incoming > MAX_UNMERGED:
            if not a.force:
                raise Fail("#%d would have %d entries to try (%d already + %d new); the limit is %d: group them "
                           "into fewer guided walk-throughs, or pass --force and justify it in the report"
                           % (a.issue, unmerged + incoming, unmerged, incoming, MAX_UNMERGED))
            sys.stderr.write("suivi: --force: %d entries to try on #%d (limit %d); the report must justify it\n"
                             % (unmerged + incoming, a.issue, MAX_UNMERGED))
        k = max(used_numbers(a.issue), default=0)
        for e in entries:
            k += 1
            t = dict(e)
            t.setdefault("colour", rec.get("colour") or rec.get("last_colour") or "")
            t.setdefault("name", rec.get("name", ""))
            t.setdefault("date", today().isoformat())
            t.update(n=a.issue, status="todo")
            write_json(test_path("%d-%d" % (a.issue, k)), t)
            ids.append("%d-%d" % (a.issue, k))
    print(" ".join(ids))
    hint_not_listening(a.issue)
    return 0


HEX_RE = re.compile(r"[0-9a-fA-F]{4,40}")


def same_commit(stored, ref):
    """Hex values compare by prefix (either way), case-insensitive; anything else, exactly."""
    stored = str(stored)
    if HEX_RE.fullmatch(stored) and HEX_RE.fullmatch(ref):
        a, b = stored.lower(), ref.lower()
        return a.startswith(b) or b.startswith(a)
    return stored == ref


def check_merge_commit(issue, commit):
    """L2: the full hash of `commit`, or Fail. Real commit, on main, containing every
    feature/<issue>-* branch still there, message citing #<issue>."""
    rc, out = git_rc("rev-parse", "--verify", "--quiet", commit + "^{commit}")
    if rc != 0:
        raise Fail("no such commit in this repository: %s" % commit)
    full = out.strip()
    if git_rc("merge-base", "--is-ancestor", full, "main")[0] != 0:
        raise Fail("commit %s is not on main" % commit)
    rc, out = git_rc("for-each-ref", "--format=%(refname:short)", "refs/heads/feature/%d-*" % issue)
    for br in out.split():
        if git_rc("merge-base", "--is-ancestor", br, full)[0] != 0:
            raise Fail("commit %s does not contain the branch %s" % (commit, br))
    rc, msg = git_rc("log", "-1", "--format=%B", full)
    if not re.search(r"#%d(?!\d)" % issue, msg):
        raise Fail("the message of commit %s does not cite #%d" % (commit, issue))
    return full


def cmd_test_merged(a):
    if not HEX_RE.fullmatch(a.commit):
        raise Fail("not a commit hash: %r" % a.commit)
    full = check_merge_commit(a.issue, a.commit)
    with Lock():
        ts = load_tests(a.issue)
        if a.replace is not None:
            hit = [t for t in ts if "commit" in t and same_commit(t["commit"], a.replace)]
            if not hit:
                raise Fail("no entry of #%d carries the commit %r" % (a.issue, a.replace))
            for t in hit:
                tid = t.pop("id")
                t["commit"] = full
                write_json(test_path(tid), t)
            print("%d entr%s replaced" % (len(hit), "y" if len(hit) == 1 else "ies"))
            return 0
        tries = [t for t in ts if "try" in t]
        if tries:
            for t in tries:
                tid = t.pop("id")
                t.pop("try")
                t["commit"] = full
                write_json(test_path(tid), t)
            print("%d entr%s merged" % (len(tries), "y" if len(tries) == 1 else "ies"))
            return 0
        others = [str(t["commit"]) for t in ts if t.get("commit") and not same_commit(t["commit"], full)]
        if others:
            raise Fail("the entries of #%d already carry the commit %s, not %s; to correct it: "
                       "suivi.py test merged %d %s --replace %s" % (a.issue, others[0], full, a.issue, a.commit, others[0]))
        if any(t.get("commit") for t in ts):
            print("already recorded")
        else:
            print("no entries")
    return 0


def is_answered(t):
    return t.get("status", "todo") != "todo" or bool(str(t.get("comment") or "").strip())


def cmd_test_remove(a):
    owner = current_owner()
    recs = load_all()
    rec = recs.get(a.issue)
    if owner is None or rec is None:
        raise Fail("only the session that owns #%d may remove its trial entries (run scripts/chantier.sh %d first)"
                   % (a.issue, a.issue))
    sessions = {str((rec.get("owner") or {}).get("session", ""))}
    if rec.get("parent") in recs:
        sessions.add(str((recs[rec["parent"]].get("owner") or {}).get("session", "")))
    sessions.discard("")
    if owner["session"] not in sessions:
        raise Fail("#%d belongs to another session (or to none): scripts/chantier.sh %d claims it" % (a.issue, a.issue))
    with Lock():
        picked = []
        for tid in a.ids:
            if not re.fullmatch(r"%d-\d+" % a.issue, tid):
                raise Fail("%r is not an entry of #%d" % (tid, a.issue))
            t = read_json(test_path(tid))
            if not isinstance(t, dict):
                raise Fail("no entry %s" % tid)
            if "try" not in t:
                raise Fail("%s is merged (no `try`): a merged entry is never removed" % tid)
            picked.append((tid, t))
        answered = [(tid, t) for tid, t in picked if is_answered(t)]
        if answered and not a.answered:
            raise Fail("answered entries need --answered: " + "; ".join(
                "%s [%s] %r" % (tid, t.get("status", "todo"), t.get("comment", "")) for tid, t in answered))
        stamp = datetime.datetime.now().isoformat(timespec="seconds")
        for tid, t in picked:
            write_json(os.path.join(removed_dir(), tid + ".json"), dict(t, removed_at=stamp))
            os.unlink(test_path(tid))
    for tid, t in answered:
        print("answered %s [%s] %s" % (tid, t.get("status", "todo"), t.get("comment", "")))
    print("%d entr%s removed" % (len(picked), "y" if len(picked) == 1 else "ies"))
    return 0


def cmd_test_list(a):
    for t in sorted(load_tests(a.issue), key=lambda t: natural(t["id"])):
        print("%s\t%s\t%s%s" % (t["id"], t.get("status", "todo"), t.get("title", ""),
                                "\t(try)" if t.get("try") else ""))
    return 0


def cmd_import_online(a):
    """Copy the `tests/*` documents of an ArtifactData export into the local state, once."""
    src = os.path.join(a.out_dir, "tests")
    if not os.path.isdir(src):
        raise Fail("no tests/ folder in %s" % a.out_dir)
    new = same = kept = 0
    bad = []
    with Lock():
        for p in sorted(glob.glob(os.path.join(src, "*.json"))):
            doc = read_json(p)
            stem = os.path.basename(p)[:-5]
            if not isinstance(doc, dict):
                bad.append(stem)
                continue
            tid = str(doc["id"]) if "id" in doc else stem
            data = doc["data"] if isinstance(doc.get("data"), dict) else doc
            if not safe_id(tid) or not safe_id(stem):
                bad.append(tid)
                continue
            data = {k: v for k, v in data.items() if k != "id" and not k.startswith("_")}
            if isinstance(data.get("try"), dict):
                data.pop("try")
            dest = test_path(tid)
            have = read_json(dest)
            if have is None:
                write_json(dest, data)
                new += 1
            elif have == data:
                same += 1
            else:
                kept += 1      # a local entry wins: it may hold a newer review
    for b in bad:
        sys.stderr.write("suivi: skipped unsafe or unreadable document %r\n" % b)
    print("imported %d, unchanged %d, kept local %d, rejected %d" % (new, same, kept, len(bad)))
    return 1 if bad else 0


# ------------------------------------------------------------------- QCM
TEMPLATE = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, ".claude", "skills",
                        "stokesforge-bug-report", "references", "decision-form.html")
# lang: fr
DATA_START = "/* DONNÉES — début"
DATA_END = "/* DONNÉES — fin */"


def read_template():
    try:
        with open(TEMPLATE, encoding="utf-8") as f:
            return f.read()
    except OSError as e:
        raise Fail("cannot read the form template %s: %s" % (os.path.normpath(TEMPLATE), e))


def inject_qcm(data, title):
    """The template with `data` between the DONNÉES markers and `title` in <title>."""
    t = read_template()
    i = t.find(DATA_START)
    j = t.find(DATA_END)
    k = t.find("*/", i)
    if i < 0 or j < 0 or k < 0 or j < i:
        raise Fail("the template has lost its DONNÉES markers")
    if "</script" in data.lower():
        raise Fail("data.js must not contain </script")
    t = t[:i] + DATA_START + " */\n" + data.strip("\n") + "\n" + t[j:]
    if title:
        t = re.sub(r"<title>.*?</title>", lambda m: "<title>%s</title>" % html.escape(title, quote=False), t, count=1)
    return t


def node_run(code, label):
    if not shutil.which("node"):
        raise Fail("node is required to check a form but is not installed")
    with tempfile.TemporaryDirectory() as tmp:
        p = os.path.join(tmp, "check.js")
        with open(p, "w", encoding="utf-8") as f:
            f.write(code)
        r = subprocess.run(["node", p], capture_output=True, text=True, timeout=30)
    if r.returncode != 0:
        raise Fail("%s does not run:\n%s" % (label, r.stderr.strip()))
    return r.stdout


def check_form(data):
    """Syntax-check the page script built with `data`, evaluate the data, return the number of cards."""
    page = inject_qcm(data, "")
    m = re.search(r"<script>(.*)</script>", page, re.S)
    if not m:
        raise Fail("the template has no script")
    with tempfile.TemporaryDirectory() as tmp:
        p = os.path.join(tmp, "page.js")
        with open(p, "w", encoding="utf-8") as f:
            f.write(m.group(1))
        if not shutil.which("node"):
            raise Fail("node is required to check a form but is not installed")
        r = subprocess.run(["node", "--check", p], capture_output=True, text=True, timeout=30)
        if r.returncode != 0:
            raise Fail("the form script has a syntax error:\n%s" % r.stderr.strip())
    head = read_template()
    head = head[head.index("<script>") + len("<script>"):head.index(DATA_START)]
    out = node_run(head + "\n" + data + "\nprocess.stdout.write(String(D.length));", "data.js")
    try:
        n = int(out)
    except ValueError:
        raise Fail("data.js defines no D array")
    if n < 1:
        raise Fail("data.js defines no card")
    return n


def cmd_qcm_pose(a):
    try:
        with open(a.data, encoding="utf-8") as f:
            data = f.read()
    except OSError as e:
        raise Fail("cannot read %s: %s" % (a.data, e))
    cards = check_form(data)
    base = os.path.join(suivi_dir(), "qcm")
    ks = [int(x.rsplit("-", 1)[1]) for x in (os.listdir(base) if os.path.isdir(base) else [])
          if re.fullmatch(r"%d-\d+" % a.issue, x)]
    qid = "%d-%d" % (a.issue, max(ks, default=0) + 1)
    d = qcm_dir(qid)
    os.makedirs(os.path.join(d, "answers"))
    with open(os.path.join(d, "data.js"), "w", encoding="utf-8") as f:
        f.write(data)
    write_json(os.path.join(d, "meta.json"), {"issue": a.issue, "title": a.title or "", "created": today().isoformat(),
                                              "cards": cards, "finished": False})
    print(qid)
    print("http://127.0.0.1:%d/qcm/%s" % (int(os.environ.get("SF_SUIVI_PORT", "8441")), qid))
    hint_not_listening(a.issue)
    return 0


def cmd_qcm_answers(a):
    meta = read_json(os.path.join(qcm_dir(a.id), "meta.json")) if safe_id(a.id) else None
    if not isinstance(meta, dict):
        raise Fail("no such QCM: %s" % a.id)
    answers = load_answers(a.id)
    if a.json:
        print(json.dumps({"id": a.id, "meta": meta, "answers": answers}, ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    pr = qcm_progress(a.id, meta)
    print("QCM %s (issue #%s) %s: %d/%d answered, %s" % (a.id, meta.get("issue"), meta.get("title", ""), pr["answered"],
                                                          pr["total"], "finished" if meta.get("finished") else "open"))
    for code in sorted(answers, key=natural):
        r = answers[code]
        status = "later" if r.get("statut") == "plus_tard" else ("recommended" if r.get("recommande") else "chosen")
        line = "%s [%s] %s -> %s" % (code, status, r.get("question", ""), r.get("choix", ""))
        if r.get("commentaire"):
            line += " -- " + r["commentaire"]
        print(line)
    return 0


def cmd_qcm_list(a):
    for q in all_qcms():
        if (a.open and q["finished"]) or (a.issue is not None and str(q["issue"]) != str(a.issue)):
            continue
        print("%s\tissue #%s\t%d/%d\t%s\t%s" % (q["id"], q["issue"], q["progress"]["answered"], q["progress"]["total"],
                                                 "finished" if q["finished"] else "open", q["title"]))
    return 0


# ------------------------------------------------------------------ server
MAX_BODY = 256 * 1024
PAGE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "suivi-page.html")
# img-src data: lets the inline SVG tab icon load; nothing else is fetched as an image.
CSP = ("default-src 'none'; img-src data:; script-src 'unsafe-inline'; style-src 'unsafe-inline' https://fonts.googleapis.com; "
       "font-src https://fonts.gstatic.com; connect-src 'self'; base-uri 'none'; form-action 'none'; "
       "frame-ancestors 'none'")


def qcm_dir(qid):
    return os.path.join(suivi_dir(), "qcm", qid)


def load_answers(qid):
    out = {}
    for p in sorted(glob.glob(os.path.join(qcm_dir(qid), "answers", "*.json"))):
        code = os.path.basename(p)[:-5]
        r = read_json(p)
        if safe_id(code) and isinstance(r, dict):
            out[code] = r
    return out


def qcm_progress(qid, meta):
    answered = sum(1 for r in load_answers(qid).values() if r.get("statut") != "plus_tard")
    return {"answered": answered, "total": meta.get("cards") or 0}


def all_qcms():
    out = []
    for p in sorted(glob.glob(os.path.join(suivi_dir(), "qcm", "*", "meta.json")), key=natural):
        qid = os.path.basename(os.path.dirname(p))
        m = read_json(p)
        if not safe_id(qid) or not isinstance(m, dict):
            continue
        out.append({"id": qid, "issue": m.get("issue"), "title": m.get("title", ""), "finished": bool(m.get("finished")),
                    "progress": qcm_progress(qid, m), "since": m.get("created", "")})
    return out


def open_qcms():
    return [q for q in all_qcms() if not q["finished"]]


# ------------------------------------------------------------ CI watcher (#452)
HERE = os.path.dirname(os.path.abspath(__file__))
CI_STATUS = os.path.join(HERE, "ci-status.sh")
# Same list as JOBS of scripts/gitlab-status.py (a test compares them).
CI_JOBS = ("fmt", "licenses", "clippy", "test", "robustness", "test-python", "manual", "linux-packages",
           "test-windows", "test-python-windows", "dist-windows")
CI_NON_BLOCKING = ("robustness",)      # a failure is posted as skipped, not failed
CI_PENDING_MAX = 3600                  # seconds a head may stay pending before it is `unknown`
CI_VERDICTS = ("green", "red", "pending", "unknown")


def ci_enabled():
    return os.environ.get("SF_SUIVI_CI") != "off"


def ci_poll_seconds():
    try:
        return max(0.05, float(os.environ.get("SF_SUIVI_CI_POLL", "120")))
    except ValueError:
        return 120.0


def ci_path():
    return os.path.join(suivi_dir(), "ci.json")


def ci_parse_jobs(text):
    """(sha, {job: STATUS}) from `ci-status.sh --head` output; sha is None if absent."""
    sha, jobs = None, {}
    for line in text.splitlines():
        m = re.match(r"HEAD ([0-9a-f]{7,64})$", line)
        if m:
            sha = m.group(1)
            continue
        if line.startswith("== commit comments"):
            break
        m = re.match(r"(\S+)\s+([A-Za-z_]+)\s+\|", line)
        if m:
            jobs[m.group(1).split("/")[-1]] = m.group(2).upper()
    return sha, jobs


def ci_job_verdict(jobs):
    """(verdict, names) for the blocking jobs (Linux and Windows); only SUCCESS everywhere is green."""
    blocking = [j for j in CI_JOBS if j not in CI_NON_BLOCKING]
    failed = [j for j in blocking if jobs.get(j) == "FAILED"]
    if failed:
        return "red", failed
    odd = [j for j in blocking if jobs.get(j) in ("CANCELED", "SKIPPED")]
    if odd:
        return "unknown", odd
    waiting = [j for j in blocking if jobs.get(j) != "SUCCESS"]
    if waiting:
        return "pending", waiting
    return "green", []


def ci_run(cmd, timeout):
    """Run cmd in its own process group; on timeout kill the whole group (glab included)."""
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                         start_new_session=True, cwd=os.path.dirname(HERE))
    try:
        out, err = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, 9)
        except OSError:
            pass
        p.communicate()
        raise
    return p.returncode, out, err


def ci_poll(prev, now=None):
    """One reading. Never raises: any doubt is `unknown`."""
    now = time.time() if now is None else now
    prev = prev if isinstance(prev, dict) else {}
    out = {"checked_at": now, "sha": None, "verdict": "unknown", "names": [], "reason": "",
           "last_red": prev.get("last_red"), "since": prev.get("since")}
    try:
        timeout = float(os.environ.get("SF_SUIVI_CI_TIMEOUT", "60"))
        rc, text, err = ci_run(["bash", CI_STATUS, "--head"], timeout)
        sha, jobs = ci_parse_jobs(text)
        out["sha"] = sha
        if sha is None:
            out["reason"] = "no head read from GitLab (exit %s)" % rc
            return out
        if prev.get("sha") == sha and prev.get("verdict") == "green":   # only green is final: a red CI may be re-run
            for k in ("verdict", "names", "reason"):
                out[k] = prev.get(k, out[k])
            out["last_red"] = prev.get("last_red")
            return out
        if prev.get("sha") != sha:
            out["since"] = now
        if rc == 2:
            verdict, names, reason = "pending", [], "no GitLab pipeline for this commit yet"
        elif rc != 0 or not jobs:
            verdict, names, reason = "unknown", [], "ci-status.sh failed (exit %s)" % rc
        else:
            verdict, names = ci_job_verdict(jobs)
            reason = ""
        if verdict == "pending" and now - (out["since"] or now) > CI_PENDING_MAX:
            verdict, reason = "unknown", "still pending after %d min" % (CI_PENDING_MAX // 60)
        out.update(verdict=verdict, names=names, reason=reason)
        if verdict == "red":
            out["last_red"] = {"sha": sha, "names": names}
        elif verdict == "green":
            out["last_red"] = None
    except Exception as e:
        out.update(verdict="unknown", names=[], reason="watcher error: %s" % type(e).__name__)
    return out


def ci_view():
    """What /api/state shows: computed when read; missing, corrupt or stale is `unknown`."""
    d = read_json(ci_path())
    view = {"verdict": "unknown", "names": [], "reason": "no reading yet", "sha": None, "last_red": None}
    if not isinstance(d, dict):
        return view
    lr = d.get("last_red")
    if isinstance(lr, dict) and isinstance(lr.get("names"), list):
        view["last_red"] = {"sha": str(lr.get("sha", ""))[:8], "names": [str(n) for n in lr["names"]]}
    if isinstance(d.get("sha"), str):
        view["sha"] = d["sha"][:8]
    at = d.get("checked_at")
    if not isinstance(at, (int, float)) or isinstance(at, bool) or time.time() - at > 3 * ci_poll_seconds():
        view["reason"] = "reading too old"
        return view
    if d.get("verdict") in CI_VERDICTS:
        view["verdict"] = d["verdict"]
        view["names"] = [str(n) for n in d.get("names", [])] if isinstance(d.get("names"), list) else []
        view["reason"] = str(d.get("reason") or "")
    return view


def ci_loop(path):
    while True:
        try:
            write_json(path, ci_poll(read_json(path)))
        except Exception:
            pass
        time.sleep(ci_poll_seconds())


def build_state():
    s = build_status()
    tests = sorted(load_tests(), key=lambda t: natural(t["id"]))
    qcms = open_qcms()
    return {"colours": s["colours"], "paused": s["paused"], "colourless": s["colourless"],
            "done": s["done"][:10], "qcms": qcms, "tests": tests,
            "control": read_json(os.path.join(suivi_dir(), "control.json")), "ci": ci_view(),
            "pending": len(qcms) + sum(1 for t in tests if t.get("status", "todo") == "todo")}


class Handler(http.server.BaseHTTPRequestHandler):
    server_version = "suivi"
    protocol_version = "HTTP/1.0"      # one request per connection: a single thread never waits on an idle client
    timeout = 5

    def log_message(self, *args):
        pass

    # -- plumbing
    def send(self, code, body=b"", ctype="text/plain; charset=utf-8", extra=()):
        if isinstance(body, str):
            body = body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Content-Security-Policy", CSP)
        for k, v in extra:
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def fail(self, code, msg):
        self.send(code, msg + "\n")

    def allowed_origins(self):
        port = self.server.server_address[1]
        return ("http://127.0.0.1:%d" % port, "http://localhost:%d" % port)

    def guard(self, write=False):
        """Host (DNS rebinding), Origin (cross-site pages) and the custom header on writes."""
        port = self.server.server_address[1]
        if self.headers.get("Host") not in ("127.0.0.1:%d" % port, "localhost:%d" % port):
            self.fail(403, "forbidden host")
            return False
        origin = self.headers.get("Origin")
        if origin is not None and origin not in self.allowed_origins():
            self.fail(403, "forbidden origin")
            return False
        if write and self.headers.get("X-Suivi") != "1":
            self.fail(403, "missing X-Suivi header")
            return False
        return True

    def route(self):
        return self.path.split("?", 1)[0]

    # -- methods
    def do_OPTIONS(self):
        if not self.guard():
            return
        self.send(204, extra=[("Allow", "GET, POST, OPTIONS")])

    def do_GET(self):
        if not self.guard():
            return
        path = self.route()
        if path == "/":
            try:
                with open(PAGE, "rb") as f:
                    page = f.read()
            except OSError:
                return self.fail(500, "page missing: %s" % PAGE)
            return self.send(200, page, "text/html; charset=utf-8")
        if path == "/api/state":
            try:
                body = json.dumps(build_state(), ensure_ascii=False, sort_keys=True).encode("utf-8")
            except Exception as e:     # a half-written folder must not kill the server
                return self.fail(500, "state unreadable: %s" % e)
            etag = '"%s"' % hashlib.sha1(body).hexdigest()[:20]
            if self.headers.get("If-None-Match") == etag:
                return self.send(304, extra=[("ETag", etag)])
            return self.send(200, body, "application/json; charset=utf-8", [("ETag", etag)])
        if path.startswith("/qcm/"):
            if not safe_id(path[len("/qcm/"):]):
                return self.fail(400, "bad id")
            return self.qcm_page(path[len("/qcm/"):])
        m = re.fullmatch(r"/api/qcm/([^/]+)/answers", path)
        if m:
            qid = m.group(1)
            meta = read_json(os.path.join(qcm_dir(qid), "meta.json")) if safe_id(qid) else None
            if not isinstance(meta, dict):
                return self.fail(404, "no such QCM")
            body = {"finished": bool(meta.get("finished")), "answers": load_answers(qid)}
            return self.send(200, json.dumps(body, ensure_ascii=False), "application/json; charset=utf-8")
        self.fail(404, "not found")

    def qcm_page(self, qid):
        meta = read_json(os.path.join(qcm_dir(qid), "meta.json"))
        if not isinstance(meta, dict):
            return self.fail(404, "no such QCM")
        try:
            with open(os.path.join(qcm_dir(qid), "data.js"), encoding="utf-8") as f:
                data = f.read()
            html = inject_qcm(data, meta.get("title", ""))
        except (OSError, Fail) as e:
            return self.fail(500, "QCM unreadable: %s" % e)
        self.send(200, html, "text/html; charset=utf-8")

    def do_HEAD(self):
        self.do_GET()

    def read_body(self):
        """The JSON body, or None after an error reply."""
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self.fail(400, "bad Content-Length")
            return None
        if length < 0:
            self.fail(400, "bad Content-Length")
            return None
        if length > MAX_BODY:
            left = min(length, 4 * MAX_BODY)       # drain, or the client sees a reset instead of the 413
            while left > 0:
                chunk = self.rfile.read(min(65536, left))
                if not chunk:
                    break
                left -= len(chunk)
            self.fail(413, "body larger than %d bytes" % MAX_BODY)
            return None
        try:
            return json.loads(self.rfile.read(length) or b"null")
        except ValueError:
            self.fail(400, "body is not JSON")
            return None

    def do_POST(self):
        if not self.guard(write=True):
            return
        path = self.route()
        if path.startswith("/api/tests/"):
            return self.post_test(path[len("/api/tests/"):])
        m = re.fullmatch(r"/api/qcm/([^/]+)/(answer|finish)", path)
        if m:
            return self.post_qcm(m.group(1), m.group(2))
        self.fail(404, "not found")

    def post_qcm(self, qid, what):
        if not safe_id(qid):
            return self.fail(400, "bad id")
        mp = os.path.join(qcm_dir(qid), "meta.json")
        body = self.read_body()
        if body is None:
            return
        meta = read_json(mp)
        if not isinstance(meta, dict):
            return self.fail(404, "no such QCM")
        if what == "finish":
            meta["finished"] = True
            meta["finished_at"] = datetime.datetime.now().isoformat(timespec="seconds")
            write_json(mp, meta)
            return self.send(200, json.dumps({"ok": True}), "application/json")
        if not isinstance(body, dict) or not isinstance(body.get("code"), str) or not safe_id(body["code"]):
            return self.fail(400, "expected an object with a safe string code")
        if not all(isinstance(v, (str, int, bool, type(None))) for v in body.values()):
            return self.fail(400, "answer fields must be plain values")
        write_json(os.path.join(qcm_dir(qid), "answers", body["code"] + ".json"), body)
        self.send(200, json.dumps({"ok": True}), "application/json")

    def post_test(self, tid):
        if not safe_id(tid):
            return self.fail(400, "bad id")
        body = self.read_body()
        if body is None:
            return
        if not isinstance(body, dict) or not ({"status", "comment"} & set(body)):
            return self.fail(400, "expected an object with status and/or comment")
        if "status" in body and body["status"] not in TEST_STATUSES:
            return self.fail(400, "status must be one of %s" % ", ".join(TEST_STATUSES))
        if "comment" in body and not isinstance(body["comment"], str):
            return self.fail(400, "comment must be a string")
        t = read_json(test_path(tid))
        if not isinstance(t, dict):
            return self.fail(404, "no such trial entry")
        for k in ("status", "comment"):
            if k in body:
                t[k] = body[k]
        t["reviewed"] = datetime.datetime.now().isoformat(timespec="seconds")
        write_json(test_path(tid), t)
        self.send(200, json.dumps({"ok": True}), "application/json")

    def other(self):
        if self.guard():
            self.send(405, "method not allowed\n", extra=[("Allow", "GET, POST, OPTIONS")])

    do_PUT = do_DELETE = do_PATCH = other


def cmd_serve(a):
    port = a.port if a.port is not None else int(os.environ.get("SF_SUIVI_PORT", "8441"))
    if a.state:
        os.environ["SF_SUIVI_DIR"] = os.path.abspath(a.state)
    state = suivi_dir()
    try:
        srv = http.server.HTTPServer(("127.0.0.1", port), Handler)
    except OSError as e:
        if e.errno == errno.EADDRINUSE:
            raise Fail("port %d is already in use (another suivi.py serve? see `ss -ltnp`)" % port)
        raise Fail("cannot listen on 127.0.0.1:%d: %s" % (port, e))
    if ci_enabled():
        import threading
        threading.Thread(target=ci_loop, args=(ci_path(),), daemon=True).start()
    print("suivi: listening on http://127.0.0.1:%d/ (state %s)" % (srv.server_address[1], state), flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


# -------------------------------------------------------------------- main
def parser():
    ap = argparse.ArgumentParser(prog="suivi.py", description=__doc__.split("\n\n")[0], allow_abbrev=False)
    sp = ap.add_subparsers(dest="cmd", required=True)

    def add(name, fn, help_):
        p = sp.add_parser(name, help=help_, allow_abbrev=False)
        p.set_defaults(fn=fn)
        return p

    p = add("claim", cmd_claim, "take (or resume) an issue for this session; prints its colour")
    p.add_argument("issue", type=int)
    p.add_argument("--parent", type=int)
    p.add_argument("--name")
    p.add_argument("--package", action="store_true")
    p.add_argument("--sub", action="append", metavar="N=TITLE", help="a sub-issue (makes the issue a package)")
    p.add_argument("--dry-run", action="store_true", help="print the colour it would take, write nothing")
    p.add_argument("--avoid", help="colours to treat as taken (dry run promises)")
    p = add("status", cmd_status, "the six colours, paused issues, worktrees without a record")
    p.add_argument("--json", action="store_true")
    p = add("set", cmd_set, "state of an issue")
    p.add_argument("issue", type=int)
    p.add_argument("--state", choices=STATES)
    p.add_argument("--what")
    p.add_argument("--wait", choices=WAIT_KINDS)
    p.add_argument("--waiting")
    p = add("pause", cmd_pause, "drop the owner (the colour is freed unless a QCM or a trial holds it)")
    p.add_argument("issue", type=int)
    p = add("done", cmd_done, "the issue is finished: colour freed")
    p.add_argument("issue", type=int)
    p = add("release-line", cmd_release_line, "the line to report after a worktree of the issue was removed")
    p.add_argument("issue", type=int)
    p.add_argument("--mark-done", action="store_true", help="also mark the issue done if nothing holds it")
    add("check", cmd_check, "list inconsistencies (exit 1 if any)")
    add("migrate", cmd_migrate, "create records from the old colour files, once")
    p = add("members", cmd_members, "the issue and its sub-issues")
    p.add_argument("issue", type=int)
    p = add("is-package", cmd_is_package, "exit 0 if the issue is a package")
    p.add_argument("issue", type=int)
    add("session-end", cmd_session_end, "SessionEnd hook: reads {session_id, reason} on stdin")
    add("hook-wait", cmd_hook_wait, "PostToolUse hook: arm the wait; exit 2 (text on stderr) only for an event")
    add("hook-news", cmd_hook_news, "SessionStart/UserPromptSubmit hook: reads {session_id} on stdin, always exits 0")
    p = add("wait", cmd_wait, "block until a trial entry is answered or a form finished (background; #454)")
    p.add_argument("issue", type=int)
    p.add_argument("--timeout", type=float, metavar="S", help="seconds before giving up (default 72 h)")
    p = add("news", cmd_news, "print and record the events not yet reported to this session")
    p.add_argument("issue", type=int)
    p = add("serve", cmd_serve, "serve the local tracking page on 127.0.0.1 only")
    p.add_argument("--port", type=int, help="default 8441 (env SF_SUIVI_PORT); 0 = any free port")
    p.add_argument("--state", help="state folder (default: ../_stokesforge/.suivi)")
    t = add("test", None, "trial entries (tests/<n>-<k>.json)").add_subparsers(dest="tcmd", required=True)
    for name, fn, h in (("add", cmd_test_add, "add entries from a JSON file (one object or a list)"),
                        ("merged", cmd_test_merged, "set the merge commit and drop `try` on the issue's entries"),
                        ("remove", cmd_test_remove, "withdraw the session's own unmerged entries (archived)"),
                        ("list", cmd_test_list, "list the entries")):
        q = t.add_parser(name, help=h, allow_abbrev=False)
        q.set_defaults(fn=fn)
        if name == "add":
            q.add_argument("issue", type=int)
            q.add_argument("file")
            q.add_argument("--force", action="store_true", help="exceed the limit of %d entries to try" % MAX_UNMERGED)
        elif name == "merged":
            q.add_argument("issue", type=int)
            q.add_argument("commit")
            q.add_argument("--replace", metavar="OLD", help="correct entries that carry the commit OLD")
        elif name == "remove":
            q.add_argument("issue", type=int)
            q.add_argument("ids", nargs="+")
            q.add_argument("--answered", action="store_true", help="also remove entries ticked or commented")
        else:
            q.add_argument("--issue", type=int)
    q = add("qcm", None, "decision forms (qcm/<id>/)").add_subparsers(dest="qcmd", required=True)
    for name, fn, h in (("pose", cmd_qcm_pose, "check data.js, create the form qcm/<n>-<k>, print its id and URL"),
                        ("answers", cmd_qcm_answers, "the answers recorded so far (readable, or --json)"),
                        ("list", cmd_qcm_list, "list the forms")):
        x = q.add_parser(name, help=h, allow_abbrev=False)
        x.set_defaults(fn=fn)
        if name == "pose":
            x.add_argument("issue", type=int)
            x.add_argument("data")
            x.add_argument("--title", default="")
        elif name == "answers":
            x.add_argument("id")
            x.add_argument("--json", action="store_true")
        else:
            x.add_argument("--open", action="store_true")
            x.add_argument("--issue", type=int)
    p = add("import-online", cmd_import_online, "import the tests/* documents of an ArtifactData export, once")
    p.add_argument("out_dir")
    return ap


def main(argv=None):
    try:
        a = parser().parse_args(argv)
    except SystemExit:
        if (argv if argv is not None else sys.argv[1:])[:1] == ["hook-wait"]:
            return 0           # an exit 2 from argparse would wake the model for nothing
        raise
    try:
        return a.fn(a)
    except Fail as e:
        print("suivi: %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
