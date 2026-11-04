import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import make2 as M
OUT = sys.argv[1]
TOP, AX, H = M.TOP, M.AX, M.H
sw, w = 10, 6
peak_y = TOP - H
base, hh = TOP - sw / 2, H - sw / 2
ab_s = M.spectrum(M.MU_A, -1, base, hh, AX + 6, M.MU_A + 30)
EM0 = [tuple(map(float, p.split(','))) for p in M.spectrum(M.MU_E, +1, base, hh, M.MU_E - 30, 232).split()]

HW, HT, HAND = 30, 18, 6
LH = int(os.environ.get('LH', '84'))
MODE = os.environ.get('MODE', 'dent')
PIV = (AX, TOP)
LEAD = 9      # the head sticks out only this much on the falling side (the handle sits near that end)
HC = LEAD - HW / 2   # head centre offset
# hammer in local coords (pivot at 0,0, handle along -y): rects (cx, cy, half_w, half_h)
RECTS = [(0, -LH / 2, HAND / 2, LH / 2), (HC, -LH - HT / 2, HW / 2, HT / 2), (HC - HW / 2 - 0.5, -LH - HT / 2, 3.5, HT / 2 - 2)]
def to_local(p, th):
    dx, dy = p[0] - PIV[0], p[1] - PIV[1]
    c, s = math.cos(-th), math.sin(-th)
    return (dx * c - dy * s, dx * s + dy * c)
def sdf(p, th):
    x, y = to_local(p, th)
    best = 1e9
    for cx, cy, hx, hy in RECTS:
        qx, qy = abs(x - cx) - hx, abs(y - cy) - hy
        out = math.hypot(max(qx, 0), max(qy, 0)); ins = min(max(qx, qy), 0)
        best = min(best, out + ins)
    return best
def clearance(th, curve):
    return min((sdf(q, th) - sw / 2, q) for q in curve)
def land(curve):
    a = 0.0
    while a < math.radians(89):
        c, q = clearance(a, curve)
        if c <= 0.3: return a, q
        a += math.radians(0.05)
    raise SystemExit("no landing")
TH, HIT = land(EM0)

def push_dir(q):   # direction the hammer moves at point q when it keeps rotating (clockwise)
    dx, dy = q[0] - PIV[0], q[1] - PIV[1]
    n = math.hypot(dx, dy); return (-dy / n, dx / n)

def deformed(th):
    """Push the emission curve out of the hammer turned to `th`, as a soft dent."""
    pts = [list(p) for p in EM0]
    d = push_dir(HIT)
    for _ in range(60):
        need = [max(0.0, sw / 2 + 0.4 - sdf(p, th)) for p in pts]
        if max(need) < 0.05: break
        k = 3.0   # smooth the push along the curve (soft material)
        sm = [max(need[j] * math.exp(-((i - j) ** 2) / (2 * k * k)) for j in range(len(need))) for i in range(len(need))]
        for p, m in zip(pts, sm):
            p[0] += 0.5 * m * d[0]; p[1] += 0.5 * m * d[1]
    for p in pts: p[1] = min(p[1], base)    # never below the baseline
    return pts

def pl(pts): return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
def g(th, fill, opacity, body):
    return f'<g fill="{fill}" opacity="{opacity}" transform="translate({PIV[0]} {PIV[1]}) rotate({math.degrees(th):.2f})">{body}</g>'
HANDLE = f'<rect x="{-HAND/2}" y="{-LH}" width="{HAND}" height="{LH}" rx="2"/>'
HEAD = (f'<rect x="{HC-HW/2}" y="{-LH-HT}" width="{HW}" height="{HT}" rx="3"/>'
        f'<rect x="{HC-HW/2-4}" y="{-LH-HT+2}" width="7" height="{HT-4}" rx="2"/>')

def sparks(q, th, color, bg, r0=10, r1=20):
    """Impact flecks fanning upward from the contact (above the hammer and the bands)."""
    segs = []
    for k, a in enumerate((-125, -88, -52, -16)):
        r = math.radians(a); L = r1 if k % 2 == 0 else r1 - 4
        segs.append(f'<line x1="{q[0]+r0*math.cos(r):.1f}" y1="{q[1]+r0*math.sin(r):.1f}" x2="{q[0]+L*math.cos(r):.1f}" y2="{q[1]+L*math.sin(r):.1f}"/>')
    segs = "".join(segs)
    return (f'<g stroke="{bg}" stroke-width="8" stroke-linecap="round">{segs}</g>'
            f'<g stroke="{color}" stroke-width="4" stroke-linecap="round">{segs}</g>')

def to_world(p, th):
    c, s_ = math.cos(th), math.sin(th)
    return (PIV[0] + p[0] * c - p[1] * s_, PIV[1] + p[0] * s_ + p[1] * c)

def scene(ink, green, ember, bg, ghosts, dent):
    th = TH + (math.radians(6.0) if dent else 0.0)
    em = deformed(th) if dent else EM0
    _, hit = clearance(th, em)
    line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'
    halo = f'fill="none" stroke-width="{sw+4}" stroke-linejoin="round" stroke-linecap="round" stroke="{bg}"'
    out = [f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>']
    if ghosts:
        R = math.hypot(LEAD, LH + HT); a0 = math.atan2(LEAD, LH + HT)
        sx, sy = PIV[0] + R * math.sin(a0), PIV[1] - R * math.cos(a0)
        ex, ey = PIV[0] + R * math.sin(a0 + th), PIV[1] - R * math.cos(a0 + th)
        out.append(f'<path d="M{sx:.1f} {sy:.1f}A{R:.1f} {R:.1f} 0 0 1 {ex:.1f} {ey:.1f}" fill="none" stroke="{ink}" stroke-opacity=".35" stroke-width="3" stroke-dasharray="2 7" stroke-linecap="round"/>')
        for k, op in ((0, .12), (1, .22)):
            out.append(g(th * k / 2.4, ink, op, HANDLE + HEAD))
    out.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
               f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    out.append(f'<polyline {halo} points="{ab_s}"/><polyline {line} stroke="{green}" points="{ab_s}"/>')
    out.append(f'<polyline {halo} points="{pl(em)}"/><polyline {line} stroke="{ember}" points="{pl(em)}"/>')
    # the whole hammer in front of the graph; a ground-coloured outline on the handle only
    # (the head keeps no outline so it rests on the band instead of biting into it)
    out.append(f'<g fill="{bg}" stroke="{bg}" stroke-width="4" transform="translate({PIV[0]} {PIV[1]}) rotate({math.degrees(th):.2f})">'
               f'<rect x="{-HAND/2}" y="{-LH+2}" width="{HAND}" height="{LH-8}"/></g>')
    out.append(g(th, ink, 1, HANDLE + HEAD))
    out.append(sparks(hit, th, ember, bg))
    return "".join(out)

L, BG = M.L, "#f3f1ec"
if MODE == "dent":
    files = {"explore-drop-dent.svg": scene(*L, BG, ghosts=True, dent=True),
             "explore-drop-dent-still.svg": scene(*L, BG, ghosts=False, dent=True)}
else:
    files = {"explore-drop.svg": scene(*L, BG, ghosts=True, dent=False),
             "explore-drop-still.svg": scene(*L, BG, ghosts=False, dent=False)}
for n, inner in files.items():
    open(os.path.join(OUT, n), "w").write(M.svg(M.tile(inner, bg=BG), "StokesForge"))
print("landing", round(math.degrees(TH), 2), "hit", HIT, "min clearance", round(clearance(TH, EM0)[0], 2))
