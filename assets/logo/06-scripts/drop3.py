import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import make2 as M
OUT = sys.argv[1]
TOP, AX, H = M.TOP, M.AX, M.H
if os.environ.get('WIDE'):
    # narrower bands, larger Stokes shift: room for the hammer head between the two maxima
    M.SIG, M.D, M.A, M.SIG2 = 7.0, 17.0, 0.6, 8.0
    M.MU_A = 151.0; M.MU_E = M.MU_A + 44
sw, w = 10, 6
peak_y = TOP - H
base, hh = TOP - sw / 2, H - sw / 2
AB_END = M.MU_A + (21 if os.environ.get('WIDE') else 28)
ab_s = M.spectrum(M.MU_A, -1, base, hh, AX + 6, AB_END)
EM0 = [tuple(map(float, p.split(','))) for p in M.spectrum(M.MU_E, +1, base, hh, M.MU_E - (20 if os.environ.get('WIDE') else 28), 233).split()]
LH = int(os.environ.get('LH', '84')); MODE = os.environ.get('MODE', 'dent')
PIV = (float(os.environ.get('PX', AX)), float(os.environ.get('PY', TOP)))

# ---- hammer drawn in local coords: pivot (0,0), handle along -y, head across it ----
HT = 17            # head thickness (along the handle)
F = 10             # striking face plane, on the falling side (+x)
P = 22             # cross-peen length on the other side (-x)
y0, y1 = -LH, -LH - HT; ym = (y0 + y1) / 2
def head_profile(x):
    """Half-height of the head at abscissa x (side view)."""
    h0 = HT / 2
    if x >= F - 4:                                   # striking face: flared, chamfered edge
        return h0 + 2.0 - max(0.0, (x - (F - 1.0))) * 1.6
    if x >= F - 7:                                   # neck before the face
        t = (x - (F - 7)) / 3.0
        return h0 - 1.3 * math.sin(math.pi * t)
    if x >= -8:                                      # body, with cheeks round the eye
        return h0 + 1.4 * math.exp(-(x / 4.0) ** 2)
    t = (x + P) / (P - 8)                            # cross peen: blunt wedge
    return 3.4 + (h0 - 3.4) * t ** 0.95
def head_poly():
    xs = [F - k * 0.5 for k in range(int((F + P) / 0.5) + 1)]
    top = [(x, ym - head_profile(x)) for x in xs]
    tip = [(-P - 3.4 * math.sin(math.pi * k / 8), ym - 3.4 * math.cos(math.pi * k / 8)) for k in range(1, 8)]
    bot = [(x, ym + head_profile(x)) for x in reversed(xs)]
    return top + tip + bot
def handle_poly():
    pts = [(-2.6, y0 + 1), (2.6, y0 + 1), (3.8, -6)]
    for k in range(7):                        # rounded butt end
        a = 0 + math.pi * k / 6
        pts.append((3.8 * math.cos(a), -6 + 3.8 * math.sin(a)))
    pts.append((-3.8, -6))
    return pts
HEAD, HANDLE = head_poly(), handle_poly()
def path(pts): return "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + "Z"

def to_local(p, th):
    dx, dy = p[0] - PIV[0], p[1] - PIV[1]; c, s = math.cos(-th), math.sin(-th)
    return (dx * c - dy * s, dx * s + dy * c)
def to_world(p, th):
    c, s = math.cos(th), math.sin(th)
    return (PIV[0] + p[0] * c - p[1] * s, PIV[1] + p[0] * s + p[1] * c)
def poly_sdf(p, poly):
    x, y = p; d = 1e9; inside = False
    n = len(poly)
    for i in range(n):
        ax, ay = poly[i]; bx, by = poly[(i + 1) % n]
        ex, ey = bx - ax, by - ay
        t = max(0, min(1, ((x - ax) * ex + (y - ay) * ey) / (ex * ex + ey * ey or 1)))
        d = min(d, math.hypot(x - ax - t * ex, y - ay - t * ey))
        if (ay > y) != (by > y) and x < ax + (y - ay) * ex / (ey or 1e-12): inside = not inside
    return -d if inside else d
def sdf(p, th):
    q = to_local(p, th)
    return min(poly_sdf(q, HEAD), poly_sdf(q, HANDLE))
def clearance(th, curve): return min((sdf(q, th) - sw / 2, q) for q in curve)
def land(curve):
    a = 0.0
    while a < math.radians(89):
        c, q = clearance(a, curve)
        if c <= 0.3: return a, q
        a += math.radians(0.05)
    raise SystemExit("no landing")
TH, HIT = land(EM0)

def push_dir(q):
    dx, dy = q[0] - PIV[0], q[1] - PIV[1]; n = math.hypot(dx, dy); return (-dy / n, dx / n)
def deformed(th):
    pts = [list(p) for p in EM0]; d = push_dir(HIT)
    for _ in range(60):
        need = [max(0.0, sw / 2 + 0.4 - sdf(p, th)) for p in pts]
        if max(need) < 0.05: break
        k = 3.0
        sm = [max(need[j] * math.exp(-((i - j) ** 2) / (2 * k * k)) for j in range(len(need))) for i in range(len(need))]
        for p, m in zip(pts, sm): p[0] += 0.5 * m * d[0]; p[1] += 0.5 * m * d[1]
    for p in pts: p[1] = min(p[1], base)
    return pts
def pl(pts): return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)

def sparks(q, color, bg, r0=10, r1=20):
    segs = []
    for k, a in enumerate((-125, -88, -52, -16)):
        r = math.radians(a); L = r1 if k % 2 == 0 else r1 - 4
        segs.append(f'<line x1="{q[0]+r0*math.cos(r):.1f}" y1="{q[1]+r0*math.sin(r):.1f}" x2="{q[0]+L*math.cos(r):.1f}" y2="{q[1]+L*math.sin(r):.1f}"/>')
    segs = "".join(segs)
    return (f'<g stroke="{bg}" stroke-width="8" stroke-linecap="round">{segs}</g>'
            f'<g stroke="{color}" stroke-width="4" stroke-linecap="round">{segs}</g>')

def hammer_svg(th, ink):
    tr = f'translate({PIV[0]} {PIV[1]}) rotate({math.degrees(th):.2f})'
    return (f'<g transform="{tr}" fill="{ink}" stroke="{ink}" stroke-width="1.2" stroke-linejoin="round">'
            f'<path d="{path(HANDLE)}"/><path d="{path(HEAD)}"/></g>')

def scene(ink, green, ember, bg, dent):
    th = TH + (math.radians(4.0) if dent else 0.0)
    em = deformed(th) if dent else EM0
    _, hit = clearance(th, em)
    line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'
    halo = f'fill="none" stroke-width="{sw+6}" stroke-linejoin="round" stroke-linecap="round" stroke="{bg}"'
    return "".join([
        f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>',
        f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>',
        f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>',
        f'<polyline {line} stroke="{ember}" points="{pl(em)}"/>',            # back: emission
        hammer_svg(th, ink),                                                    # middle: the hammer, resting on it
        f'<polyline {halo} points="{ab_s}"/><polyline {line} stroke="{green}" points="{ab_s}"/>',  # front: absorption
        sparks(hit, ember, bg)])

L, BG = M.L, "#f3f1ec"
NAME = os.environ.get('NAME', 'explore-drop')
files = ({f"{NAME}-dent.svg": scene(*L, BG, dent=True)} if MODE == "dent"
         else {f"{NAME}.svg": scene(*L, BG, dent=False)})
for n, inner in files.items():
    open(os.path.join(OUT, n), "w").write(M.svg(M.tile(inner, bg=BG), "StokesForge"))
print("landing", round(math.degrees(TH), 2), "hit", HIT)
