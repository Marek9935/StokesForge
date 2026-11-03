#!/usr/bin/env python3
"""Work-package colours and who holds them (#441). Standard library only.

    python3 -I scripts/suivi.py <command> ...

The single source of truth is ../_stokesforge/.suivi/ (override: SF_SUIVI_DIR):

    issues/<n>.json        one record per issue: colour, last_colour, kind
                           (issue|package), parent, subs, name, state, waiting,
                           wait_kind, what, since, owner{session,pid,pid_start,at},
                           done_at
    ended/<session_id>     empty files touched by the SessionEnd hook
    qcm/<id>/meta.json     {issue, finished}   (written by the QCM server)
    tests/<n>-<k>.json     {status, try}       (written by the To-test page)
    control.json           result of the last `check`

THE RULE. An issue holds its colour if, and only if, (a) the Claude session that
owns it is alive, or (b) one of its QCMs is open, or (c) one of its trial entries
(with a `try` field) is still todo. Otherwise it is PAUSED: the colour is free,
the worktree stays. On resume (`claim`) the issue gets its previous colour back if
nobody took it, else the first free one. "Held" is computed when read, never
stored: a crash pauses an issue with no process left to write.

Writes go through a temporary file and os.replace. Colour allocation takes the
directory lock ../_stokesforge/.colour.lock (same lock as chantier.sh). Reads take
no lock. Test seams: SF_SUIVI_DIR, SF_TODAY=YYYY-MM-DD, SF_SUIVI_LOCK_WAIT (seconds),
SF_SUIVI_SLOW (seconds slept inside a claim's critical section).

Commands: claim, status, set, pause, done, release-line, check, migrate,
members, is-package, session-end.
"""
import argparse
import datetime
import glob
import json
import os
import re
import subprocess
import sys
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
    if kind == "reason-trial":
        return "un essai de l'issue est en attente"
    raise ValueError(kind)
# lang: fr end


# ------------------------------------------------------------------- paths
def git(*args, cwd=None):
    r = subprocess.run(["git", *args], capture_output=True, text=True, cwd=cwd)
    return r.stdout.strip() if r.returncode == 0 else ""


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
    return why


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
            "since": None, "owner": None, "done_at": None}


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
        for key, val in (("state", a.state), ("what", a.what), ("waiting", a.waiting), ("wait_kind", a.wait)):
            if val is not None:
                r[key] = val
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
    return ", ".join({"agent": "agent", "qcm": colour_line("reason-qcm"), "essai": colour_line("reason-trial")}[w]
                     for w in held_by(r))


def cmd_done(a):
    with Lock():
        recs = load_all()
        r = need(recs, a.issue)
        had = bool(held_by(r))
        colour = r.get("colour")
        r["done_at"] = today().isoformat()
        r["owner"] = None
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


def cmd_members(a):
    recs = load_all()
    ids = [a.issue] + [x["issue"] for x in recs.values() if x.get("parent") == a.issue]
    ids += [s["n"] for s in (recs.get(a.issue) or {}).get("subs", [])]
    print(" ".join(str(i) for i in sorted(set(ids))))
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
        colours.append({
            "colour": c, "free": False, "issue": g, "issues": groups,
            "package": {"parent": g, "subs": pkg.get("subs", [])} if pkg else None,
            "name": (pkg or lead).get("name", ""), "state": lead.get("state", ""),
            "what": lead.get("what", ""), "waiting": lead.get("waiting", ""), "wait_kind": lead.get("wait_kind", ""),
            "held_by": why, "worktrees": group_wts(g), "since": since, "days": days})
    paused, done = [], []
    for r in recs.values():
        if r.get("done_at"):
            done.append({"issue": r["issue"], "name": r.get("name", ""), "done_at": r["done_at"]})
        elif not held_by(r) and r.get("parent") is None:
            since, days = days_since(r.get("since"))
            paused.append({"issue": r["issue"], "kind": r.get("kind", "issue"), "name": r.get("name", ""),
                           "last_colour": r.get("colour") or r.get("last_colour"), "subs": r.get("subs", []),
                           "worktrees": group_wts(r["issue"]), "since": since, "days": days})
        elif not held_by(r):
            since, days = days_since(r.get("since"))
            paused.append({"issue": r["issue"], "kind": "issue", "parent": r["parent"], "name": r.get("name", ""),
                           "last_colour": r.get("colour") or r.get("last_colour"), "subs": [],
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
                r.update(kind="package", subs=subs, colour=colour, last_colour=colour, since=since)
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
            r.update(parent=member_of.get(n), colour=colour, last_colour=colour,
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
    return ap


def main(argv=None):
    a = parser().parse_args(argv)
    try:
        return a.fn(a)
    except Fail as e:
        print("suivi: %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
