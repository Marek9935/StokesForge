import math, os, sys
os.environ.setdefault('WIDE', '1')
sys.path.insert(0, os.path.dirname(__file__))
import drop3 as D          # hammer shape, sdf, spectra (WIDE: Stokes shift 44); re-runs drop3 into argv[1], harmless
M = D.M
OUT = sys.argv[1]
TOP, AX, H, sw, w = D.TOP, D.AX, D.H, D.sw, D.w
peak_y = TOP - H
AB = [tuple(map(float, p.split(','))) for p in D.ab_s.split()]
EM = D.EM0
pa = (M.peak_x(M.MU_A, -1)); pe = (M.peak_x(M.MU_E, +1))
apex_a = min(AB, key=lambda q: q[1]); apex_e = min(EM, key=lambda q: q[1])

def land_on(curve, piv):
    D.PIV = piv
    a = 0.0
    while a < math.radians(89):
        c, q = D.clearance(a, curve)
        if c <= 0.3: return a, q
        a += math.radians(0.05)
    raise SystemExit("no landing")

def spark_arc(q0, q1, color, bg, arrow=False):
    """Sparks thrown by the blow on the absorption band, landing on the emission maximum."""
    x0, y0 = q0[0] + 13, q0[1] - 11
    x1, y1 = q1[0], q1[1] - sw / 2 - 7
    cx, cy = (x0 + x1) / 2, min(y0, y1) - 34
    B = lambda t: ((1-t)**2*x0 + 2*(1-t)*t*cx + t*t*x1, (1-t)**2*y0 + 2*(1-t)*t*cy + t*t*y1)
    if arrow:
        ex, ey = B(0.97); px, py = B(0.88)
        ang = math.atan2(ey - py, ex - px)
        hx = lambda s, r: (ex + s * math.cos(ang + r), ey + s * math.sin(ang + r))
        a1, a2 = hx(-9, -0.55), hx(-9, 0.55)
        d = f"M{x0:.1f} {y0:.1f}Q{cx:.1f} {cy:.1f} {B(0.97)[0]:.1f} {B(0.97)[1]:.1f}"
        return (f'<path d="{d}" fill="none" stroke="{color}" stroke-width="4" stroke-linecap="round" stroke-dasharray="1 8"/>'
                f'<path d="M{a1[0]:.1f} {a1[1]:.1f}L{ex:.1f} {ey:.1f}L{a2[0]:.1f} {a2[1]:.1f}" fill="none" stroke="{color}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>')
    out = []
    for i, t in enumerate((0.12, 0.28, 0.44, 0.60, 0.75, 0.88, 0.98)):
        x, y = B(t); r = 1.7 + 0.28 * i
        out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.2f}"/>')
    return f'<g fill="{color}">{"".join(out)}</g>'

def impact(q, color, bg, angles=(-150, -118, -86, -54)):
    segs = []
    for k, a in enumerate(angles):
        r = math.radians(a); L = 20 if k % 2 == 0 else 16
        segs.append(f'<line x1="{q[0]+10*math.cos(r):.1f}" y1="{q[1]+10*math.sin(r):.1f}" x2="{q[0]+L*math.cos(r):.1f}" y2="{q[1]+L*math.sin(r):.1f}"/>')
    segs = "".join(segs)
    return (f'<g stroke="{bg}" stroke-width="8" stroke-linecap="round">{segs}</g>'
            f'<g stroke="{color}" stroke-width="4" stroke-linecap="round">{segs}</g>')

def hammer_at(th, piv, ink):
    D.PIV = piv
    return D.hammer_svg(th, ink)

def base_layers(ink, green, ember, bg):
    line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'
    return (f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>'
            f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
            f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>'
            f'<polyline {line} stroke="{ember}" points="{D.pl(EM)}"/>'
            f'<polyline {line} stroke="{green}" points="{D.ab_s}"/>')

def scene_fall(ink, green, ember, bg, arrow):
    piv = (66.0, 169.0)
    th, hit = land_on(AB, piv)
    return (base_layers(ink, green, ember, bg) + hammer_at(th, piv, ink) +
            impact(hit, ember, bg) + spark_arc(hit, apex_e, ember, bg, arrow)), th, hit

def scene_strike(ink, green, ember, bg):
    # blow from above: hammer mirrored so the face points down onto the absorption maximum, handle rising to the right
    th = math.radians(112)
    c, s_ = math.cos(th), math.sin(th)
    fx, fy = D.F, D.ym                                        # face centre
    tx, ty = apex_a[0], apex_a[1] - sw / 2 - 0.6
    piv = (tx - (fx * c - fy * s_), ty - (fx * s_ + fy * c))
    tr = f'translate({piv[0]:.2f} {piv[1]:.2f}) rotate({math.degrees(th):.2f})'
    ham = (f'<g transform="{tr}" fill="{ink}" stroke="{ink}" stroke-width="1.2" stroke-linejoin="round">'
           f'<path d="{D.path(D.HANDLE)}"/><path d="{D.path(D.HEAD)}"/></g>')
    hit = (apex_a[0], apex_a[1])
    return (base_layers(ink, green, ember, bg) + ham +
            impact(hit, ember, bg, angles=(-176, -150, -30, -4)) + spark_arc(hit, apex_e, ember, bg)), piv

L, BG = M.L, "#f3f1ec"
SCENE = os.environ.get('SCENE', 'fall')
if SCENE == 'fall':
    a, th, hit = scene_fall(*L, BG, arrow=False)
    a2, _, _ = scene_fall(*L, BG, arrow=True)
    files = {"explore-excite-fall.svg": a, "explore-excite-fall-arrow.svg": a2}
    info = ("fall angle", round(math.degrees(th), 1), "hit", hit)
else:
    c_, piv = scene_strike(*L, BG)
    files = {"explore-excite-strike.svg": c_}
    info = ("strike pivot", tuple(round(v, 1) for v in piv))
for n, inner in files.items():
    open(os.path.join(OUT, n), "w").write(M.svg(M.tile(inner, bg=BG), "StokesForge"))
print(*info)
