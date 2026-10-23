"""Résume le dernier bloc PROF du journal de l'app où une fenêtre a été déplacée."""
import os, re
log = os.environ.get('SF_RUN_DIR', '/tmp/stokesforge-run') + '/app.log'
blocks=[];cur=None
for l in open(log):
    if l.startswith('--- PROF'): cur=[];blocks.append(cur)
    elif l.startswith('PROF') and cur is not None: cur.append(l.rstrip())
# bloc de drag = celui avec le plus de 'ui total'
def n(b):
    for l in b:
        if "08 set_window_geometry" in l: return int(l.split()[-4])
    return 0
kept = [b for b in blocks if n(b) >= 20]
if not kept:
    print("Glissement raté (aucun bloc avec 20 appels à set_window_geometry) : relancer.")
for b in kept:
    print("--- bloc de 2 s pendant le glissement")
    print("\n".join(l for l in b if re.search(r'PROF (00|01|05|07|08|09|10)', l)))
