import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import make2 as M
OUT = sys.argv[1]
TOP, AX, H = M.TOP, M.AX, M.H
sw, w = 10, 6
peak_y = TOP - H
base, hh = TOP - sw / 2, H - sw / 2
ab_s = M.spectrum(M.MU_A, -1, base, hh, AX + 6, M.MU_A + 30)
em_s = M.spectrum(M.MU_E, +1, base, hh, M.MU_E - 30, 232)
EM = [tuple(map(float, p.split(','))) for p in em_s.split()]

HW, HT, LH, HAND = 38, 18, 126, 7     # head width, head thickness, handle length, handle width
PIV = (AX, TOP)
def outline(step=1.0):
    out = []
    def rect(x0, y0, x1, y1):
        x = x0
        while x <= x1: out.extend([(x, y0), (x, y1)]); x += step
        y = y0
        while y <= y1: out.extend([(x0, y), (x1, y)]); y += step
    rect(-HAND / 2, -LH, HAND / 2, 0); rect(-HW / 2, -LH - HT, HW / 2, -LH); rect(-HW / 2 - 4, -LH - HT + 2, -HW / 2 + 3, -LH - 2)
    return out
HO = outline()
def rot(p, th):
    c, s = math.cos(th), math.sin(th)
    return (PIV[0] + p[0] * c - p[1] * s, PIV[1] + p[0] * s + p[1] * c)
def land():
    a = 0.0
    while a < math.radians(89):
        best = min((math.dist(rot(p, a), q), rot(p, a), q) for p in HO[::2] for q in EM)
        if best[0] - sw / 2 <= 0.6:
            return a, best[2]
        a += math.radians(0.25)
    raise SystemExit("no landing")
TH, HIT = land()

def _g(th, fill, opacity, body):
    return f'<g fill="{fill}" opacity="{opacity}" transform="translate({PIV[0]} {PIV[1]}) rotate({math.degrees(th):.2f})">{body}</g>'
HANDLE = f'<rect x="{-HAND/2}" y="{-LH}" width="{HAND}" height="{LH}" rx="2"/>'
HEAD = (f'<rect x="{-HW/2}" y="{-LH-HT}" width="{HW}" height="{HT}" rx="3"/>'
        f'<rect x="{-HW/2-4}" y="{-LH-HT+2}" width="7" height="{HT-4}" rx="2"/>')
HEAD_HALO = f'<rect x="{-HW/2-4}" y="{-LH-HT}" width="{HW+4}" height="{HT}" rx="3" stroke-width="7" stroke-linejoin="round"/>'
def ham(th, fill, opacity=1.0):
    return _g(th, fill, opacity, HANDLE + HEAD)

def burst(cx, cy, r0, r1, n, fill, rot0=0):
    pts = []
    for i in range(2 * n):
        r = r1 if i % 2 == 0 else r0
        a = rot0 + math.pi * i / n
        pts.append(f"{cx + r*math.cos(a):.1f},{cy + r*math.sin(a):.1f}")
    return f'<polygon points="{" ".join(pts)}" fill="{fill}" stroke-linejoin="round"/>'

def spark_ring(cx, cy, color, angles, r0=9, r1=18, width=4):
    s = "".join(f'<line x1="{cx+r0*math.cos(math.radians(a)):.1f}" y1="{cy+r0*math.sin(math.radians(a)):.1f}" x2="{cx+r1*math.cos(math.radians(a)):.1f}" y2="{cy+r1*math.sin(math.radians(a)):.1f}"/>' for a in angles)
    return f'<g stroke="{color}" stroke-width="{width}" stroke-linecap="round">{s}</g>'

def scene(ink, green, ember, bg, axis=False, ghosts=True, cartoon=False):
    line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'
    out = [f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>']
    if ghosts:
        R = math.hypot(HW / 2, LH + HT)                      # far corner of the head
        a0 = math.atan2(HW / 2, LH + HT)
        sx, sy = PIV[0] + R * math.sin(a0), PIV[1] - R * math.cos(a0)
        ex, ey = PIV[0] + R * math.sin(a0 + TH), PIV[1] - R * math.cos(a0 + TH)
        out.append(f'<path d="M{sx:.1f} {sy:.1f}A{R:.1f} {R:.1f} 0 0 1 {ex:.1f} {ey:.1f}" fill="none" stroke="{ink}" stroke-opacity=".35" stroke-width="3" stroke-dasharray="2 7" stroke-linecap="round"/>')
        for k, op in ((0, .12), (1, .22)):
            out.append(ham(TH * k / 2.4, ink, op))
    if axis:
        out.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
                   f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    out.append(f'<polyline {line} stroke="{green}" points="{ab_s}"/>')
    out.append(f'<polyline {line} stroke="{ember}" points="{em_s}"/>')
    # the fallen handle lies across the graph, resting on the emission maximum
    out.append(f'<g fill="{bg}" stroke="{bg}" stroke-width="6" transform="translate({PIV[0]} {PIV[1]}) rotate({math.degrees(TH):.2f})"><rect x="{-HAND/2}" y="{-LH+4}" width="{HAND}" height="{LH-16}"/></g>')
    out.append(_g(TH, ink, 1.0, HANDLE))
    hx, hy = HIT
    if cartoon:
        out.append(burst(hx + 4, hy - 4, 9, 20, 7, "#ffd166", rot0=0.2))
        # vibration lines on the struck emission band's right flank
        for i, r in enumerate((26, 34)):
            out.append(f'<path d="M{M.MU_E + r:.1f} {peak_y + 14 + 4*i:.1f}q7 12 0 24" fill="none" stroke="{ember}" stroke-width="3.5" stroke-linecap="round"/>')
    out.append(f'<g fill="{bg}" stroke="{bg}" transform="translate({PIV[0]} {PIV[1]}) rotate({math.degrees(TH):.2f})">{HEAD_HALO}</g>')
    out.append(_g(TH, ink, 1.0, HEAD))            # head in front, resting on the emission band
    out.append(spark_ring(hx + 2, hy - 2, ember, (-150, -110, -70, -30, 10), r0=10, r1=19, width=4 if not cartoon else 4.5))
    return "".join(out)

L, Dk = M.L, M.Dk
BG, BGD = "#f3f1ec", "#15181a"
files = {
  "explore-fall.svg":            M.svg(M.tile(scene(*L, BG), bg=BG), "StokesForge — chute"),
  "explore-fall-axis.svg":       M.svg(M.tile(scene(*L, BG, axis=True), bg=BG), "StokesForge — chute, axe fixe"),
  "explore-fall-cartoon.svg":    M.svg(M.tile(scene(*L, BG, axis=True, ghosts=False, cartoon=True), bg=BG), "StokesForge — cartoon"),
  "explore-fall-night.svg":      M.svg(M.tile(scene(*Dk, BGD), bg=BGD), "StokesForge — chute, de nuit"),
}
for n, s in files.items():
    open(os.path.join(OUT, n), "w").write(s)
print("landing", round(math.degrees(TH), 1), "hit", HIT)
