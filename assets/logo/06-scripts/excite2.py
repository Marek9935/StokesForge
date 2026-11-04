import math, os, sys
os.environ.setdefault('WIDE', '1')
sys.path.insert(0, os.path.dirname(__file__))
import drop3 as D
import drop4 as E
M = D.M
OUT = sys.argv[1]
TOP, AX, H, sw, w = D.TOP, D.AX, D.H, D.sw, D.w
peak_y = TOP - H
base, hh = D.base, D.hh
SHIFT = float(os.environ.get('SHIFT', '34'))
M.MU_A = 173 - SHIFT / 2; M.MU_E = 173 + SHIFT / 2
ab_s = M.spectrum(M.MU_A, -1, base, hh, AX + 6, M.MU_A + 26)
em_s = M.spectrum(M.MU_E, +1, base, hh, M.MU_E - 26, 233)
AB = [tuple(map(float, p.split(','))) for p in ab_s.split()]
EM = [tuple(map(float, p.split(','))) for p in em_s.split()]
apex_a = min(AB, key=lambda q: q[1]); apex_e = min(EM, key=lambda q: q[1])
PIV = (66.0, 169.0)
th, hit = E.land_on(AB, PIV)
L = M.L; ink, green, ember = L; BG = "#f3f1ec"
line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'

def streak_pts():
    x0, y0 = hit[0] + 10, hit[1] - 8            # just right of the blow
    x1, y1 = apex_e[0], apex_e[1] - sw / 2 - 5  # just above the emission maximum
    return x0, y0, x1, y1

def grad(id_, x0, x1, c0, c1, y=0):
    return (f'<linearGradient id="{id_}" gradientUnits="userSpaceOnUse" x1="{x0:.1f}" y1="{y}" x2="{x1:.1f}" y2="{y}">'
            f'<stop offset="0" stop-color="{c0}"/><stop offset="1" stop-color="{c1}"/></linearGradient>')

def trait():
    x0, y0, x1, y1 = streak_pts()
    return (grad("g", x0, x1, green, ember),
            f'<path d="M{x0:.1f} {y0:.1f}L{x1:.1f} {y1:.1f}" stroke="url(#g)" stroke-width="6" stroke-linecap="round"/>')

def comete():
    x0, y0, x1, y1 = streak_pts()
    a = math.atan2(y1 - y0, x1 - x0); nx, ny = -math.sin(a), math.cos(a)
    w0, w1 = 0.8, 4.2
    poly = [(x0 + nx * w0, y0 + ny * w0), (x1 + nx * w1, y1 + ny * w1), (x1 - nx * w1, y1 - ny * w1), (x0 - nx * w0, y0 - ny * w0)]
    d = "M" + "L".join(f"{x:.1f} {y:.1f}" for x, y in poly) + "Z"
    return (grad("g", x0, x1, green, ember),
            f'<path d="{d}" fill="url(#g)"/><circle cx="{x1:.1f}" cy="{y1:.1f}" r="{w1+1.4:.1f}" fill="{ember}"/>')

def bande():
    # the emission's rising flank is born green and turns orange at its maximum
    return (grad("gb", M.MU_E - 24, apex_e[0] - 2, green, ember), "")

def lueur():
    defs, link = trait()
    under = " ".join(f"{x:.1f},{y:.1f}" for x, y in EM) + f" {EM[-1][0]:.1f},{TOP-3} {EM[0][0]:.1f},{TOP-3}"
    defs += (f'<linearGradient id="glow" gradientUnits="userSpaceOnUse" x1="0" y1="{apex_e[1]:.1f}" x2="0" y2="{TOP}">'
             f'<stop offset="0" stop-color="{ember}" stop-opacity=".38"/><stop offset="1" stop-color="{ember}" stop-opacity="0"/></linearGradient>')
    return defs, f'<polygon points="{under}" fill="url(#glow)"/>' + link

def glow_defs():
    under = " ".join(f"{x:.1f},{y:.1f}" for x, y in EM) + f" {EM[-1][0]:.1f},{TOP-3} {EM[0][0]:.1f},{TOP-3}"
    return (f'<linearGradient id="glow" gradientUnits="userSpaceOnUse" x1="0" y1="{apex_e[1]:.1f}" x2="0" y2="{TOP}">'
            f'<stop offset="0" stop-color="{ember}" stop-opacity=".38"/><stop offset="1" stop-color="{ember}" stop-opacity="0"/></linearGradient>',
            f'<polygon points="{under}" fill="url(#glow)"/>')

def sparkle(cx, cy, r, color, k=0.26):
    pts = []
    for i in range(8):
        a = -math.pi / 2 + i * math.pi / 4
        rr = r if i % 2 == 0 else r * k
        pts.append(f"{cx + rr*math.cos(a):.1f},{cy + rr*math.sin(a):.1f}")
    return f'<polygon points="{" ".join(pts)}" fill="{color}" stroke="{color}" stroke-width="1.2" stroke-linejoin="round"/>'

def scene(kind):
    glow = ""
    if kind in ("bande", "bande-lueur", "etoile"):
        defs, link = bande()
        if kind == "bande-lueur":
            gd, glow = glow_defs(); defs += gd
        if kind == "etoile":
            link = (f'<g stroke="{BG}" stroke-width="5" stroke-linejoin="round">{sparkle(apex_e[0] + 15, apex_e[1] - 20, 11, BG)}</g>'
                    + sparkle(apex_e[0] + 15, apex_e[1] - 20, 11, ember) + sparkle(apex_e[0] + 30, apex_e[1] - 6, 5.5, ember))
    elif kind == "lueur":
        defs, link = trait(); gd, glow = glow_defs(); defs += gd
    else:
        defs, link = {"trait": trait, "comete": comete}[kind]()
    em_stroke = "url(#gb)" if kind in ("bande", "bande-lueur", "etoile") else ember
    parts = [f'<defs>{defs}</defs>',
             f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>',
             f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>',
             f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>',
             glow,
             f'<polyline {line} stroke="{em_stroke}" points="{em_s}"/>',
             f'<polyline {line} stroke="{green}" points="{ab_s}"/>',
             E.hammer_at(th, PIV, ink),
             E.impact(hit, ember, BG),
             link]
    return "".join(parts)

for kind in ("trait", "comete", "bande", "lueur", "bande-lueur", "etoile"):
    open(os.path.join(OUT, f"explore-excite-{kind}.svg"), "w").write(M.svg(M.tile(scene(kind), bg=BG), "StokesForge"))
print("angle", round(math.degrees(th), 1), "hit", hit, "apex_e", apex_e)
