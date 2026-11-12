#!/usr/bin/env python3
"""Where does the working time go? Aggregates from the Claude session logs (#452).

    scripts/time-report.py [--since DATE] [--until DATE] [--logs DIR] [--power FILE] [--json]

Reads the session logs of this repository's main checkout (`~/.claude/projects/<slug>`, derived from
`git rev-parse --git-common-dir`; no home directory is hard-coded) and cuts every timeline (a session,
or one of its subagents) into non-overlapping segments: model, tool, wait for the user, wait for a
subagent, wait for CI, and so on. Time while the PC was suspended or off is taken out.

Privacy: the logs are private. The tool prints hours, counts and issue numbers only. It never prints,
stores or re-emits a prompt, a reply, a command, a title or a description, and a malformed line is
skipped without echoing it. No intermediate file is written.

Dates are UTC (`2026-10-31` or `2026-10-31T20:00`). `--power FILE` is a JSON object
`{"suspend": [[start, end], ...], "off": [[start, end], ...]}` in epoch seconds; without it the windows
are read from `journalctl` (kernel suspend entry and exit, gaps between boots). When neither is
available the report says so and counts suspended time as it was logged.

Exit codes: 0 report printed, 2 bad arguments or no logs.
"""
import argparse
import collections
import datetime as dt
import glob
import json
import os
import re
import subprocess
import sys

TAG = re.compile(r'[\U0001F7E6\U0001F7E9\U0001F7E7\U0001F7EA\U0001F7E5\U0001F7E8]\s*#(\d+)')
# `chantier.sh 452 2>&1`: the `2` of a redirection is not an issue number.
CHANTIER = re.compile(r'chantier\.sh\s+(?:--\S+\s+)*((?:\d+(?![\d>])\s*)+)')
# lang: fr begin
# The user types French: these patterns read the developer's prompts and the assistant's French
# sentences, locally, and never print them.
LANCE = re.compile(r'(?:lance|traite|reprends?|continue|start|launch|resume)\s+(?:l.issue\s+)?#?(\d{2,4})', re.I)
NEXT_FORM = re.compile(r'(qcm|form|quest|retours? (fait|envoy))|\bqcm\b.*(ok|done|full|rempli|fini|fait)')
NEXT_TRIAL = re.compile(r'(test|essai|valid|try|ok.*(test|essai|fusion|merge))|\d+ valid|test ok|essai ok')
TEXT_FORM = re.compile(r'formulaire|\bqcm\b|questionnaire', re.I)
# lang: fr end
MODEL_STALL = 15 * 60
GAP = 6 * 3600
EDGES = [(120, '<2 min'), (600, '2-10 min'), (1800, '10-30 min'), (7200, '30 min-2 h'), (float('inf'), '>2 h')]

BASH_CATS = [
    ('test_python', r'python-test\.sh'),
    ('test', r'cargo\s+(\+\S+\s+)?(test|nextest)|test-all\.sh'),
    ('clippy', r'cargo\s+(\+\S+\s+)?clippy'),
    ('build', r'cargo\s+(\+\S+\s+)?(build|check|run|doc|install|update|tree|deny)'),
    ('manual_shots', r'manual-screenshots'),
    ('manual_build', r'mdbook|manual-'),
    ('screen', r'driver\.py|\bDRIVER\b|\$D\s+(start|click|key|type|ss|stop)'),
    ('try', r'\btry\.sh'),
    ('ci', r'ci-status|gh\s+run|glab\s+ci|gitlab-status|gh\s+api\s+.*actions'),
    ('deliver', r'main-lock|git\s+(\S+\s+)*(merge|push|pull|fetch|rebase)'),
    ('chantier', r'chantier\.sh|worktree\s+add'),
    ('tracking', r'suivi\.py|suivi-'),
    ('fmt', r'cargo\s+(\+\S+\s+)?fmt|rustfmt'),
    ('git', r'\bgit\s'),
    ('forge', r'\b(glab|gh)\s'),
    ('image', r'\b(magick|convert|import|grim|gnome-screenshot)\b'),
    ('sleep', r'^\s*(sleep|until|while)\b'),
    ('read', r'^\s*(cd\s+\S+\s*(&&|;)\s*)?(rg|grep|ls|cat|sed|find|head|tail|wc|awk|du|df|stat|diff|sort|tree)\b'),
    ('guard', r'french-guard|arch-guard|licenses'),
    ('script', r'python3?\s|bash\s|\./|scripts/'),
]
BASH_RE = [(c, re.compile(r)) for c, r in BASH_CATS]
POLL = re.compile(r'(^|[;&(\s"\'])(until|while)\s.*\b(sleep|do)\b|\bsleep\s+\d+\s*(;|&&)\s*(tail|cat|rg|grep)|timeout\s+\d+\s+bash\s+-c', re.S)
REAL_CARGO = re.compile(r'(^|[;&|(\s])cargo\s+(\+\S+\s+)?(test|nextest|build|clippy|check|run)\b')
HEREDOC = re.compile(r'(?<!<)<<(?!<)-?\s*[\'"]?(\w+)[\'"]?[^\n]*\n.*?(?:\n\s*\1\b|\Z)', re.S)
DRIVER_START = re.compile(r'driver\w*\.py\s+start\b')
DRIVER_VAR_START = re.compile(r'\$\{?\w+\}?\s+start\b')  # D=.../driver.py && python3 $D start
HOOK_FEEDBACK = re.compile(r'<summary>\s*Stop hook feedback\s*</summary>', re.I)
LONG_CATS = ('bash:test', 'bash:build', 'bash:clippy', 'bash:test_python', 'bash:manual_shots', 'bash:ci', 'bash:other',
             'bash:script', 'bash:deliver')


def ts(s):
    t = dt.datetime.fromisoformat(s.replace('Z', '+00:00'))
    if t.tzinfo is None:
        t = t.replace(tzinfo=dt.timezone.utc)
    return t.timestamp()


def strip_heredocs(cmd):
    """A heredoc body is data (a commit message, a script), not a command that runs."""
    return HEREDOC.sub('', cmd)


def bash_cat(cmd):
    cmd = strip_heredocs(cmd)
    if POLL.search(cmd) and not REAL_CARGO.search(re.sub(r'pgrep[^;&|]*', '', cmd)) and 'python-test' not in cmd:
        return 'ci' if re.search(r'ci-status|gh\s+run', cmd) else 'poll'
    for c, r in BASH_RE:
        if r.search(cmd):
            return c
    return 'other'


def tool_cat(name, inp):
    if name == 'Bash':
        return 'bash:' + bash_cat(inp.get('command', ''))
    if name == 'Monitor':
        return 'monitor'
    if name == 'Agent':
        return 'agent_fg'
    if name == 'AskUserQuestion':
        return 'ask_user'
    if name in ('Read', 'Grep', 'Glob', 'LS', 'ToolSearch'):
        if name == 'Read' and str(inp.get('file_path', '')).lower().endswith(('.png', '.jpg', '.jpeg')):
            return 'read_image'
        return 'read_tool'
    if name in ('Edit', 'Write', 'NotebookEdit'):
        return 'write_tool'
    if name in ('Artifact', 'ArtifactData', 'ArtifactComments', 'ArtifactCheck'):
        return 'artifact'
    if name == 'Skill':
        return 'skill'
    return 'tool:' + name


def text_of(m):
    x = m.get('content')
    if isinstance(x, str):
        return x
    if not isinstance(x, list):
        return ''
    return '\n'.join(b.get('text', '') for b in x if isinstance(b, dict) and b.get('type') == 'text')


def blocks(m, kind):
    x = m.get('content')
    if not isinstance(x, list):
        return []
    return [b for b in x if isinstance(b, dict) and b.get('type') == kind]


def human_kind(o):
    """human | notification | peer | meta | command | interrupt | None for a user record."""
    org = o.get('origin') or {}
    k = org.get('kind') if isinstance(org, dict) else None
    if k == 'human':
        return 'human'
    if k == 'task-notification':
        return 'notification'
    if k == 'peer':
        return 'peer'
    if o.get('isMeta'):
        return 'meta'
    m = o.get('message', {})
    if blocks(m, 'tool_result'):
        return None
    t = text_of(m).lstrip()
    if t.startswith('<task-notification'):
        return 'notification'
    if t.startswith(('<bash-', '<command-', '<local-command')):
        return 'command'
    if t.startswith('[Request interrupted'):
        return 'interrupt'
    if t.startswith('<system-reminder'):
        return 'meta'
    return 'human'


def is_driver_start(cmd):
    """The screen driver is started: `driver.py start`, a renamed copy, or `$D start` with D set to a driver."""
    return bool(DRIVER_START.search(cmd) or (re.search(r'driver\w*\.py', cmd) and DRIVER_VAR_START.search(cmd)))


def hook_feedback_text(tx):
    """The wake-up text of a `Stop hook feedback` notification (the hook says what the developer did), else None."""
    if not HOOK_FEEDBACK.search(tx):
        return None
    return re.sub(r'(?s)<task-notification>.*?</task-notification>|</?system-reminder>', '', tx)\
        .split('":', 1)[-1].strip()


def note_commands(state, cmd):
    """Track what the developer is asked to do: a trial entry or a decision form stays pending across turns."""
    c = strip_heredocs(cmd)
    if re.search(r'suivi\.py\s+test\s+(merged|remove)\b', c):
        state['pend_trial'] = False
    elif re.search(r'suivi\.py\s+test\s+add\b|\btry\.sh\b', c):
        state['pend_trial'] = True
    if re.search(r'suivi\.py\s+qcm\s+(answers|withdraw)\b', c):
        state['pend_form'] = False
    elif re.search(r'suivi\.py\s+qcm\s+pose\b', c):
        state['pend_form'] = True


def wait_reason(state, next_text):
    """Why the developer was being waited for: trial, form, question or nothing pending."""
    lt = (state['last_text'] or '')[-1500:]
    nt = (next_text or '')[:80].lower()
    trial = state['pend_trial'] or bool(NEXT_TRIAL.match(nt))
    form = state['pend_form'] or bool(NEXT_FORM.match(nt)) or bool(TEXT_FORM.search(lt[-500:]))
    if trial:
        return 'trial'  # a form open together with a trial reads as the trial
    if form:
        return 'form'
    if lt.rstrip().endswith('?'):
        return 'question'
    return 'nothing'


def load(path):
    rows = []
    with open(path, errors='replace') as f:
        for line in f:
            try:
                o = json.loads(line)
            except ValueError:
                continue
            if isinstance(o, dict):
                rows.append(o)
    return rows


def process(rows, session, agent, ctx):
    """One timeline to segments. Returns (segments, model_low intervals, first issues, facts)."""
    use_ts, dur, meta, notif_end = {}, {}, {}, {}
    for o in rows:
        if not o.get('timestamp'):
            continue
        t = ts(o['timestamp'])
        if o.get('type') == 'assistant':
            for b in blocks(o['message'], 'tool_use'):
                use_ts.setdefault(b['id'], t)
                meta[b['id']] = (b['name'], b.get('input') or {})
        elif o.get('type') == 'user':
            for b in blocks(o.get('message', {}), 'tool_result'):
                i = b.get('tool_use_id')
                if i in use_ts and i not in dur:
                    dur[i] = t - use_ts[i]
        elif o.get('type') == 'queue-operation' and o.get('operation') == 'enqueue':
            content = str(o.get('content') or '')
            m = re.search(r'<task-id>([^<]+)</task-id>', content)
            if content.startswith('<task-notification') and m:
                notif_end.setdefault(m.group(1), []).append(t)
    st = dict(fg={}, in_turn=False, bg={}, issues=[], last_text='', pend_trial=False, pend_form=False,
              started=False, last_long='other', last_end=0)
    facts = dict(driver_start=False, first_ts=None)
    segs, model_low = [], []
    bg_end = ctx.get('bg_end', {})
    t_prev = None
    req = None

    def set_issues(new):
        new = sorted(set(int(x) for x in new))
        if new and new != st['issues']:
            st['issues'] = new

    def emit(a, b, cat, sub=''):
        if b <= a:
            return
        if segs and segs[-1]['t1'] == a and segs[-1]['cat'] == cat and segs[-1]['sub'] == sub and segs[-1]['iss'] == st['issues']:
            segs[-1]['t1'] = b
            return
        segs.append(dict(s=session, a=agent, t0=a, t1=b, cat=cat, sub=sub, iss=list(st['issues'])))

    def advance(t):
        nonlocal t_prev
        if t_prev is None:
            t_prev = t
            return
        if t <= t_prev:
            return
        a, b = t_prev, t
        t_prev = t
        if st['fg']:
            i = max(st['fg'], key=lambda x: dur.get(x, 0))
            emit(a, b, 'gap' if b - a > GAP else 'tool', st['fg'][i])
        elif st['in_turn']:
            emit(a, b, 'stall' if b - a > MODEL_STALL else 'model', '' if b - a > MODEL_STALL else ctx.get('model', ''))
        else:
            cur = a
            while cur < b:
                live = [v for v in st['bg'].values() if v['end'] > cur]
                if live:
                    nxt = min([v['end'] for v in live] + [b])
                    emit(cur, nxt, 'wait_bg', sorted(live, key=lambda v: v['rank'])[0]['cat'])
                    cur = nxt
                else:
                    emit(cur, b, 'idle')
                    cur = b

    def add_bg(key, cat, rank, end, t):
        # A task that is not known to end is not assumed to run: it would turn later waits into phantom work.
        # It ends at its first notification after it started, or at its own last record if that comes first.
        later = [x for x in notif_end.get(key, []) if x > t]
        if later:
            end = later[0] if end is None else min(end, later[0])
        if end is not None and end > t:
            st['bg'][key] = dict(cat=cat, rank=rank, end=end)

    for o in rows:
        typ = o.get('type')
        if typ == 'api-request' and str(o.get('querySource', '')).startswith('repl_main') and o.get('timestamp'):
            req = ts(o['timestamp'])
            continue
        if typ == 'queue-operation' and o.get('operation') == 'enqueue' and o.get('timestamp'):
            m = re.search(r'<agent-message from="([^"]+)"', str(o.get('content') or ''))
            if m and m.group(1) in st['bg']:
                advance(ts(o['timestamp']))
            continue
        if typ not in ('user', 'assistant', 'system') or not o.get('timestamp'):
            continue
        t = ts(o['timestamp'])
        if facts['first_ts'] is None:
            facts['first_ts'] = t
        if typ == 'system':
            if o.get('subtype') == 'turn_duration':
                advance(t)
                st['in_turn'] = False
            continue
        advance(t)
        if typ == 'assistant':
            m = o['message']
            ctx['model'] = m.get('model', ctx.get('model', ''))
            st['started'] = True
            if req is not None:
                model_low.append((req, t))
            tx = text_of(m)
            if tx.strip():
                st['last_text'] = tx
                found = TAG.findall(tx[:300])
                if found:
                    set_issues(found)
            for b in blocks(m, 'tool_use'):
                name, inp = b['name'], b.get('input') or {}
                cat = tool_cat(name, inp)
                cmd = inp.get('command', '') if name == 'Bash' else ''
                if cat == 'bash:poll':
                    cat = 'bash:poll_' + st['last_long']
                elif cat in LONG_CATS and (cat not in ('bash:other', 'bash:script') or inp.get('run_in_background')
                                           or re.search(r'(^|\s)(nohup|setsid)\b|&\s*($|\n)', cmd)):
                    st['last_long'] = cat.split(':')[1]
                st['fg'][b['id']] = cat
                if cmd:
                    note_commands(st, cmd)
                    if is_driver_start(strip_heredocs(cmd)):
                        facts['driver_start'] = True
                    c = CHANTIER.search(strip_heredocs(cmd))
                    if c:
                        set_issues(c.group(1).split())
            if m.get('stop_reason') == 'end_turn':
                st['in_turn'] = False
                st['last_end'] = t
            elif m.get('stop_reason') == 'tool_use':
                st['in_turn'] = True
            continue
        m = o.get('message', {})
        k = human_kind(o)
        results = blocks(m, 'tool_result')
        if results:
            req = None
            r = o.get('toolUseResult') if isinstance(o.get('toolUseResult'), dict) else {}
            for b in results:
                i = b.get('tool_use_id')
                st['fg'].pop(i, None)
                nm, inp = meta.get(i, ('?', {}))
                if r.get('isAsync') and r.get('agentId'):
                    aid = r['agentId']
                    add_bg(aid, 'agent:' + str(r.get('resolvedModel') or inp.get('model') or '?'), 0,
                           bg_end.get(aid), t)
                    ctx.setdefault('agent_parent', {})[aid] = list(st['issues'])
                elif r.get('backgroundTaskId'):
                    tid = r['backgroundTaskId']
                    add_bg(tid, 'bash:' + bash_cat(inp.get('command', '')), 1, None, t)
                elif nm == 'Monitor' and r.get('taskId'):
                    end = t + (r.get('timeoutMs') or 3600000) / 1000
                    add_bg(r['taskId'], 'monitor:' + bash_cat(inp.get('command', '')), 2, end, t)
                elif nm == 'SubagentHandback':
                    st['bg'].clear()
                elif nm == 'SendMessage':
                    rid = r.get('resumedAgentId') or r.get('revivedAgentId')
                    if rid:
                        add_bg(rid, 'agent:?', 0, bg_end.get(rid), t)
            st['in_turn'] = not any(meta.get(b.get('tool_use_id'), ('?',))[0] == 'SubagentHandback' for b in results)
            continue
        if agent and k == 'human':
            st['in_turn'] = True
            st['bg'].clear()
            req = None
            continue
        if k == 'human':
            req = None
            tx = text_of(m)
            found = LANCE.findall(tx[:200]) + TAG.findall(tx[:200])
            if found:
                set_issues(found)
            if segs and segs[-1]['cat'] == 'idle' and segs[-1]['t1'] >= t - 1:
                segs[-1]['cat'] = 'wait_user'
                segs[-1]['sub'] = wait_reason(st, tx) if st['started'] else 'session_open'
            st['in_turn'] = True
            st['fg'].clear()
        elif k in ('notification', 'peer'):
            req = None
            fb = hook_feedback_text(text_of(m)) if k == 'notification' else None
            if fb is not None and segs and segs[-1]['cat'] == 'idle' and segs[-1]['t1'] >= t - 1:
                # #454: the hook that carries the developer's trial or form answer wakes the agent.
                segs[-1]['cat'] = 'wait_user'
                segs[-1]['sub'] = wait_reason(st, fb) if st['started'] else 'session_open'
            elif segs and segs[-1]['cat'] == 'idle' and segs[-1]['t1'] >= t - 1:
                # The agent had stopped, nobody was asked, and something outside woke it: not a user wait.
                segs[-1]['cat'] = 'agent_stuck'
            if t - st['last_end'] >= 1.0:
                st['in_turn'] = True
        elif k == 'interrupt':
            st['in_turn'] = False
            st['fg'].clear()
            req = None
    if segs and segs[-1]['cat'] == 'idle':
        segs[-1]['cat'] = 'session_end'
    first = next((s['iss'] for s in segs if s['iss']), [])
    for s in segs:
        if s['iss']:
            break
        if not (s['cat'] == 'wait_user' and s['sub'] == 'session_open'):
            s['iss'] = list(first)
    return segs, model_low, first, facts


def merge(iv):
    out = []
    for a, b in sorted(iv):
        if b <= a:
            continue
        if out and a <= out[-1][1]:
            out[-1][1] = max(out[-1][1], b)
        else:
            out.append([a, b])
    return out


def pieces(a, b, down):
    """Cut [a, b] on the PC-down windows: (start, end, is_down)."""
    out = []
    for x, y in down:
        if y <= a or x >= b:
            continue
        if x > a:
            out.append((a, x, False))
        out.append((max(a, x), min(b, y), True))
        a = min(b, y)
        if a >= b:
            break
    if a < b:
        out.append((a, b, False))
    return out


def on_hours(iv, down, since, until):
    tot = 0.0
    for a, b in merge([(max(a, since), min(b, until)) for a, b in iv]):
        tot += sum(y - x for x, y, d in pieces(a, b, down) if not d)
    return tot / 3600


def report_cat(s):
    c, x = s['cat'], s['sub']
    if c == 'wait_user':
        return 'wait:' + x
    if c == 'tool' and x == 'ask_user':
        return 'wait:ask_user'
    if c in ('agent_stuck', 'session_end', 'gap', 'stall', 'model'):
        return c
    if c in ('tool', 'wait_bg'):
        y = x.replace('bash:', '').replace('monitor:', '').replace('poll_', '')
        if y.startswith('agent'):
            return 'agent_wait'
        for names, label in ((('test', 'test_python'), 'test'), (('build',), 'build'), (('clippy',), 'clippy'),
                             (('ci',), 'ci'), (('screen', 'image', 'read_image'), 'screen'),
                             (('manual_shots', 'manual_build'), 'manual_shots'),
                             (('deliver', 'git', 'forge', 'chantier', 'tracking', 'try'), 'git'),):
            if y in names:
                return label
        return 'other_tools'
    return c


def analyze(logs, since, until, power):
    down = sorted(map(tuple, (power['suspend'] + power['off']))) if power else []
    files = sorted(f for f in glob.glob(os.path.join(logs, '*.jsonl')) if os.path.getmtime(f) >= since)
    main_c, agent_c = collections.Counter(), collections.Counter()
    issues = collections.defaultdict(collections.Counter)
    episodes = collections.Counter()
    ep_n = collections.Counter()
    sessions = dict(total=0, driver=0, skipped=0)
    model_low_h = 0.0
    cmp_ = dict(compared=0, without=0, before_window=0, model_h=0.0, api_h=0.0)
    for f in files:
        sid = os.path.basename(f)[:-6]
        try:
            rows = load(f)
            ctx = {}
            spans, subs = {}, {}
            for sf in glob.glob(os.path.join(logs, sid, 'subagents', '**', '*.jsonl'), recursive=True):
                aid = os.path.basename(sf)[:-6].replace('agent-', '')
                srows = load(sf)
                times = [ts(o['timestamp']) for o in srows if o.get('timestamp') and o.get('type') in ('user', 'assistant')]
                if times:
                    spans[aid] = max(times)
                    subs[aid] = (srows, min(times))
            ctx['bg_end'] = spans
            segs, low, first, facts = process(rows, sid, '', ctx)
            for aid, (srows, _) in sorted(subs.items(), key=lambda kv: kv[1][1]):
                s2, _, _, _ = process(srows, sid, aid, dict(bg_end=spans))
                iss = ctx.get('agent_parent', {}).get(aid) or first
                for s in s2:
                    s['iss'] = list(iss)
                segs += s2
            cost = next((o for o in reversed(rows) if o.get('type') == 'cost-state'), None)
        except Exception as e:  # never echo a message: it may quote a log line
            sessions['skipped'] += 1
            print('session skipped: %s' % type(e).__name__, file=sys.stderr)
            continue
        if not segs or segs[-1]['t1'] <= since or segs[0]['t0'] >= until:
            continue
        sessions['total'] += 1
        if facts['driver_start']:
            sessions['driver'] += 1
        sess_model = 0.0
        for s in segs:
            a, b = max(s['t0'], since), min(s['t1'], until)
            if b <= a:
                continue
            parts = pieces(a, b, down)
            on = sum(y - x for x, y, d in parts if not d)
            for x, y, d in parts:
                w = y - x
                key = 'pc_down' if d else report_cat(s)
                if s['a']:
                    if d:
                        pass
                    elif s['cat'] in ('idle', 'session_end', 'wait_user'):
                        agent_c['idle'] += w
                    elif s['cat'] in ('tool', 'wait_bg', 'model', 'stall'):
                        agent_c['model' if key in ('model', 'stall') else 'tools'] += w
                        for i in s['iss'] or [0]:
                            issues[i]['agent_' + key] += w / (len(s['iss']) or 1)
                    else:
                        agent_c[key] += w
                    if not d and key == 'model':
                        sess_model += w
                    continue
                main_c[key] += w
                if not d and key == 'model':
                    sess_model += w
                if d:
                    continue
                n = len(s['iss']) or 1
                for i in s['iss'] or [0]:
                    # A wait is whole for each issue it blocked; active time is shared.
                    issues[i][key] += w if key.startswith('wait:') else w / n
            if s['cat'] == 'wait_user' and not s['a'] and on > 0:
                for e, name in EDGES:
                    if on < e:
                        episodes[name] += on
                        ep_n[name] += 1
                        break
        ml = on_hours(low, down, since, until)
        model_low_h += ml
        sess_model_h = sess_model / 3600
        if cost is None:
            cmp_['without'] += 1
        elif (facts['first_ts'] or 0) < since:
            cmp_['before_window'] += 1
        else:
            cmp_['compared'] += 1
            cmp_['model_h'] += sess_model_h
            cmp_['api_h'] += (cost.get('totalAPIDuration') or 0) / 3.6e6
    out = dict(sessions=sessions, main_h={k: round(v / 3600, 2) for k, v in main_c.items()},
               agents_h={k: round(v / 3600, 2) for k, v in agent_c.items()},
               model_range_h=[round(model_low_h, 2), round(main_c['model'] / 3600, 2)],
               episodes=[dict(bucket=n, hours=round(episodes[n] / 3600, 2), n=ep_n[n]) for _, n in EDGES],
               cost_state={k: (round(v, 2) if isinstance(v, float) else v) for k, v in cmp_.items()},
               issues={str(i): {k: round(v / 3600, 2) for k, v in c.items()} for i, c in issues.items()},
               power=None if power is None else dict(suspend=len(power['suspend']), off=len(power['off'])))
    return out


def power_from_journal(since, until):
    """Suspend and off windows from journald, or None when it cannot be read."""
    try:
        k = subprocess.run(['journalctl', '_TRANSPORT=kernel', '--no-pager', '-o', 'short-unix', '-g', 'PM: suspend (entry|exit)',
                            '--since=@%d' % (since - 86400), '--until=@%d' % (until + 3600)],
                           capture_output=True, text=True, timeout=120)
        b = subprocess.run(['journalctl', '--list-boots', '--no-pager', '-o', 'json'],
                           capture_output=True, text=True, timeout=120)
        if k.returncode != 0 or b.returncode != 0:
            return None
        suspend, start = [], None
        for line in k.stdout.splitlines():
            m = re.match(r'(\d+\.\d+)\s.*PM: suspend (entry|exit)', line)
            if not m:
                continue
            if m.group(2) == 'entry':
                start = float(m.group(1))
            elif start is not None:
                suspend.append([start, float(m.group(1))])
                start = None
        boots = sorted(json.loads(b.stdout), key=lambda x: x['first_entry'])
        off = [[a['last_entry'] / 1e6, c['first_entry'] / 1e6] for a, c in zip(boots, boots[1:])
               if c['first_entry'] > a['last_entry']]
        return dict(suspend=suspend, off=[w for w in off if w[1] > since and w[0] < until])
    except (OSError, ValueError, subprocess.SubprocessError):
        return None


def default_logs():
    try:
        common = subprocess.run(['git', 'rev-parse', '--path-format=absolute', '--git-common-dir'],
                                capture_output=True, text=True, check=True).stdout.strip()
        root = os.path.dirname(common)
    except (OSError, subprocess.SubprocessError):
        root = os.getcwd()
    return os.path.join(os.path.expanduser('~/.claude/projects'), re.sub(r'[^A-Za-z0-9]', '-', root))


def render(r, since, until):
    m = r['main_h']
    clock = sum(v for k, v in m.items() if k != 'pc_down')
    wait = sum(v for k, v in m.items() if k.startswith('wait:'))
    fmt = lambda t, tag: '  %-30s%8.2f h %5.1f%%' % (tag, t, 100 * t / clock if clock else 0)
    day = lambda t: dt.datetime.fromtimestamp(t, dt.timezone.utc).strftime('%Y-%m-%d %H:%M')
    L = ['Time report %s .. %s UTC: %d sessions (%d without readable log skipped)' % (
        day(since), day(until), r['sessions']['total'], r['sessions']['skipped'])]
    if r['power'] is None:
        L.append('Power windows unavailable: suspended or off time is counted as it was logged.')
    else:
        L.append('Power windows: %d suspends, %d off periods removed (%.2f h).' % (
            r['power']['suspend'], r['power']['off'], m.get('pc_down', 0)))
    L.append('== Orchestrators, PC on: %.2f h' % clock)
    rows = [('User wait (all reasons)', wait), ('Waiting on subagents', m.get('agent_wait', 0)),
            ('CI', m.get('ci', 0)), ('Model', m.get('model', 0)), ('Model stalled > 15 min', m.get('stall', 0)),
            ('Agent stopped, woken by event', m.get('agent_stuck', 0)), ('Session end', m.get('session_end', 0)),
            ('Log gap', m.get('gap', 0)), ('Idle, no wake record', m.get('idle', 0))]
    for k in ('test', 'build', 'clippy', 'screen', 'manual_shots', 'git', 'other_tools'):
        rows.append(('Tool: ' + k, m.get(k, 0)))
    for tag, t in rows:
        L.append(fmt(t, tag))
    L.append('== User wait by reason')
    for k, v in sorted(m.items(), key=lambda kv: -kv[1]):
        if k.startswith('wait:'):
            L.append('  %-20s%8.2f h' % (k[5:], v))
    L.append('== User wait by episode length (PC-on hours; one episode is one wait, however many suspends cut it)')
    for e in r['episodes']:
        L.append('  %-12s%8.2f h  n=%d' % (e['bucket'], e['hours'], e['n']))
    a = r['agents_h']
    L.append('== Subagents (timelines, summed): model %.2f h, tools %.2f h, idle %.2f h' % (
        a.get('model', 0), a.get('tools', 0), a.get('idle', 0)))
    lo, hi = r['model_range_h']
    L.append('== Orchestrator model time: %.1f-%.1f h (request to last reply record, up to the extractor figure '
             'that includes client latency)' % (lo, hi))
    c = r['cost_state']
    L.append('== Cost-state check: compared %d sessions (model %.2f h against API %.2f h); without cost-state %d '
             '(left out, not zero); started before the window %d' % (
                 c['compared'], c['model_h'], c['api_h'], c['without'], c['before_window']))
    L.append('== Screen driver: %d sessions started the driver (reading images does not count)' % r['sessions']['driver'])
    L.append('== Per issue, hours (waits whole per issue, active time shared 1/n): top 15 by active time')
    act = lambda c: sum(v for k, v in c.items() if not k.startswith('wait:'))
    for i, cs in sorted(r['issues'].items(), key=lambda kv: -act(kv[1]))[:15]:
        L.append('  #%-5s wait %6.2f  model %5.2f  test %5.2f  ci %5.2f  active %6.2f' % (
            i, sum(v for k, v in cs.items() if k.startswith('wait:')), cs.get('model', 0), cs.get('test', 0),
            cs.get('ci', 0), act(cs)))
    return '\n'.join(L)


def main(argv=None):
    ap = argparse.ArgumentParser(description='Aggregate hours from the session logs (no conversation is printed).')
    now = dt.datetime.now(dt.timezone.utc)
    ap.add_argument('--since', default=(now - dt.timedelta(days=7)).strftime('%Y-%m-%d'))
    ap.add_argument('--until', default=now.strftime('%Y-%m-%dT%H:%M'))
    ap.add_argument('--logs', default=None)
    ap.add_argument('--power', default=None)
    ap.add_argument('--json', action='store_true')
    a = ap.parse_args(argv)
    try:
        since, until = ts(a.since), ts(a.until)
    except ValueError:
        print('bad date: use YYYY-MM-DD or YYYY-MM-DDTHH:MM', file=sys.stderr)
        return 2
    logs = a.logs or default_logs()
    if not os.path.isdir(logs):
        print('no session logs found; pass --logs DIR', file=sys.stderr)
        return 2
    if a.power:
        try:
            with open(a.power) as f:
                power = json.load(f)
            power = dict(suspend=power.get('suspend', []), off=power.get('off', []))
        except (OSError, ValueError, AttributeError):
            print('cannot read --power file', file=sys.stderr)
            return 2
    else:
        power = power_from_journal(since, until)
    r = analyze(logs, since, until, power)
    print(json.dumps(r, indent=1, sort_keys=True) if a.json else render(r, since, until))
    return 0


if __name__ == '__main__':
    sys.exit(main())
