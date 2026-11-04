import math, os, sys
os.environ.setdefault('WIDE', '1')
sys.path.insert(0, os.path.dirname(__file__))
import excite2 as X            # spectra (Stokes 34), hammer landing on the absorption maximum; writes its own files, ignored
D, E, M = X.D, X.E, X.M
OUT = sys.argv[1]
TOP, AX, H, sw, w = X.TOP, X.AX, X.H, X.sw, X.w
peak_y = TOP - H
AB, EM, ab_s, em_s = X.AB, X.EM, X.ab_s, X.em_s
apex_a, apex_e, hit, th, PIV = X.apex_a, X.apex_e, X.hit, X.th, X.PIV
YEL = "#f2b233"
line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'

def lg(id_, x0, y0, x1, y1, stops):
    s = "".join(f'<stop offset="{o}" stop-color="{c}" stop-opacity="{a}"/>' for o, c, a in stops)
    return f'<linearGradient id="{id_}" gradientUnits="userSpaceOnUse" x1="{x0:.1f}" y1="{y0:.1f}" x2="{x1:.1f}" y2="{y1:.1f}">{s}</linearGradient>'

def sparkle(cx, cy, r, color, k=0.24, outline=None):
    pts = []
    for i in range(8):
        a = -math.pi / 2 + i * math.pi / 4; rr = r if i % 2 == 0 else r * k
        pts.append(f"{cx + rr*math.cos(a):.1f},{cy + rr*math.sin(a):.1f}")
    o = f'<polygon points="{" ".join(pts)}" fill="{outline}" stroke="{outline}" stroke-width="5" stroke-linejoin="round"/>' if outline else ""
    return o + f'<polygon points="{" ".join(pts)}" fill="{color}" stroke="{color}" stroke-width="1.2" stroke-linejoin="round"/>'

def comet(c0, c1, cmid=None, w1=4.0):
    x0, y0 = hit[0] + 10, hit[1] - 9
    x1, y1 = apex_e[0] + 3, apex_e[1] - 15
    a = math.atan2(y1 - y0, x1 - x0); nx, ny = -math.sin(a), math.cos(a)
    poly = [(x0 + nx * .6, y0 + ny * .6), (x1 + nx * w1, y1 + ny * w1), (x1 - nx * w1, y1 - ny * w1), (x0 - nx * .6, y0 - ny * .6)]
    stops = [(0, c0, 1), (1, c1, 1)] if not cmid else [(0, c0, 1), (.55, cmid, 1), (1, c1, 1)]
    d = lg("comet", x0, y0, x1, y1, stops)
    return d, f'<path d="M{"L".join(f"{x:.1f} {y:.1f}" for x, y in poly)}Z" fill="url(#comet)"/>', (x1, y1)

def flank(green, ember, cmid=None, light=True):
    x0, x1 = M.MU_E - 24, apex_e[0] - 1
    if cmid:
        stops = [(0, green, 1), (.45, cmid, 1), (1, ember, 1)]
    elif light:
        stops = [(0, green, 1), (.5, ember, 1), (1, ember, 1)]     # lighter: turns orange halfway up the flank
    else:
        stops = [(0, green, 1), (1, ember, 1)]
    return lg("flank", x0, 0, x1, 0, stops)

def anvil(ink, details=False, hot=False, filled=None):
    p = M.anvil_path(top=TOP)
    out = ""
    if filled:
        out += f'<path d="{p}" fill="{filled}"/>'
    out += f'<path fill="none" stroke="{"url(#hot)" if hot else ink}" stroke-width="{w}" stroke-linejoin="round" d="{p}"/>'
    if details:   # hardy hole (square) and pritchel hole (round) seen in the face, as notches under the table
        out += (f'<rect x="206" y="{TOP+6}" width="9" height="9" rx="1.5" fill="none" stroke="{ink}" stroke-width="3.5"/>'
                f'<circle cx="224" cy="{TOP+10.5}" r="3.6" fill="none" stroke="{ink}" stroke-width="3.5"/>')
    return out

def y_axis(ink, tip_sparkle=None):
    if tip_sparkle:
        return (f'<path d="M{AX} {TOP}V{peak_y-10}" stroke="{ink}" stroke-width="{w}" stroke-linecap="round"/>'
                + sparkle(AX, peak_y - 20, 11, tip_sparkle))
    return (f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
            f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')

def embers(color, pts):
    return "".join(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{color}" opacity="{o}"/>' for x, y, r, o in pts)

def scene(v, night=False):
    ink, green, ember = M.Dk if night else M.L
    bg = "#15181a" if night else "#f3f1ec"
    defs, under, over = [], [], []
    cmid = YEL if v in ("spectre", "nuit") else None
    defs.append(flank(green, ember, cmid=cmid))
    cd, cpath, tip = comet(green, ember, cmid=cmid if cmid else None)
    defs.append(cd)
    if v == "chaude":
        defs.append(lg("hot", 0, TOP - 3, 0, TOP + 40, [(0, ember, 1), (.35, ink, 1), (1, ink, 1)]))
        defs.append(lg("glow", 0, apex_e[1], 0, TOP, [(0, ember, .32), (1, ember, 0)]))
        poly = " ".join(f"{x:.1f},{y:.1f}" for x, y in EM) + f" {EM[-1][0]:.1f},{TOP-3} {EM[0][0]:.1f},{TOP-3}"
        under.append(f'<polygon points="{poly}" fill="url(#glow)"/>')
    if v == "reflet":
        defs.append(lg("fade", 0, TOP, 0, TOP + 46, [(0, "#ffffff", .55), (1, "#ffffff", 0)]))
        defs.append(f'<mask id="m"><rect x="0" y="{TOP}" width="256" height="60" fill="url(#fade)"/></mask>')
    if v == "nuit":
        defs.append('<filter id="bl" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="4"/></filter>')
    parts = [f'<defs>{"".join(defs)}</defs>']
    parts.append(anvil(ink, details=(v in ("enclume",)), hot=(v == "chaude")))
    parts.append(y_axis(ink, tip_sparkle=(YEL if v == "enclume" else None)))
    parts += under
    if v == "nuit":
        parts.append(f'<polyline {line} stroke="{green}" stroke-opacity=".7" filter="url(#bl)" points="{ab_s}"/>')
        parts.append(f'<polyline {line} stroke="{ember}" stroke-opacity=".8" filter="url(#bl)" points="{em_s}"/>')
    parts.append(f'<polyline {line} stroke="url(#flank)" points="{em_s}"/>')
    parts.append(f'<polyline {line} stroke="{green}" points="{ab_s}"/>')
    if v == "reflet":
        refl = lambda s: " ".join(f"{x},{2*TOP+6-float(y):.1f}" for x, y in (p.split(',') for p in s.split()))
        parts.append(f'<g mask="url(#m)"><polyline fill="none" stroke-width="6" stroke-linecap="round" stroke="{green}" points="{refl(ab_s)}"/>'
                     f'<polyline fill="none" stroke-width="6" stroke-linecap="round" stroke="{ember}" points="{refl(em_s)}"/></g>')
    parts.append(E.hammer_at(th, PIV, ink))
    parts.append(E.impact(hit, ember if not night else YEL, bg))
    parts.append(cpath)
    parts.append(sparkle(tip[0] + 1, tip[1] - 1, 10.5, YEL if night else ember, outline=bg))
    parts.append(sparkle(tip[0] + 16, tip[1] + 12, 5, YEL if night else ember))
    if v == "chaude":
        parts.append(embers(ember, [(128, TOP - 6, 2.2, .9), (214, TOP - 8, 1.8, .8), (226, TOP - 16, 1.4, .6), (120, TOP - 14, 1.3, .5)]))
    if v == "nuit":
        parts.append(embers(YEL, [(206, 44, 1.6, .9), (222, 62, 1.2, .7), (60, 70, 1.3, .6), (180, 30, 1.0, .6), (238, 40, 1.0, .5)]))
    return M.tile("".join(parts), bg=bg)

for i, v in enumerate(("base", "spectre", "chaude", "enclume", "reflet", "nuit"), 1):
    open(os.path.join(OUT, f"v{i}-{v}.svg"), "w").write(M.svg(scene(v, night=(v == "nuit")), "StokesForge"))
print("ok")
