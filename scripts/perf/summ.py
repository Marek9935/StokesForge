"""Summarizes the last PROF block of the app log in which a window was moved."""
import os, re
log = os.environ.get('SF_RUN_DIR', '/tmp/stokesforge-run') + '/app.log'
blocks=[];cur=None
for l in open(log):
    if l.startswith('--- PROF'): cur=[];blocks.append(cur)
    elif l.startswith('PROF') and cur is not None: cur.append(l.rstrip())
# drag block = the one with the most 'ui total'
def n(b):
    for l in b:
        if "08 set_window_geometry" in l: return int(l.split()[-4])
    return 0
kept = [b for b in blocks if n(b) >= 20]
if not kept:
    print("Drag missed (no block with 20 calls to set_window_geometry): rerun.")
for b in kept:
    print("--- 2 s block during the drag")
    print("\n".join(l for l in b if re.search(r'PROF (00|01|05|07|08|09|10)', l)))
