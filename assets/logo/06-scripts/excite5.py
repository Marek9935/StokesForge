import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import excite3 as V
X, D, E, M = V.X, V.D, V.E, V.M
OUT = sys.argv[1]
TOP, AX, H, sw = V.TOP, V.AX, V.H, V.sw
peak_y = TOP - H
AB, EM, ab_s, em_s = V.AB, V.EM, V.ab_s, V.em_s
apex_e, hit, th, PIV = V.apex_e, V.hit, V.th, V.PIV
ink, green, ember = M.L
BG = "#f3f1ec"
YEL = V.YEL
def area(pts, base=TOP - 3):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{base} {pts[0][0]:.1f},{base}"
lg = V.lg

def comet():
    # the n°6 comet: a thin wedge widening from the blow, ending in a round head on the emission maximum
    x0, y0 = hit[0] + 10, hit[1] - 8
    x1, y1 = apex_e[0], apex_e[1] - sw / 2 - 5
    a = math.atan2(y1 - y0, x1 - x0); nx, ny = -math.sin(a), math.cos(a)
    w0, w1 = 0.8, 4.2
    poly = [(x0 + nx * w0, y0 + ny * w0), (x1 + nx * w1, y1 + ny * w1), (x1 - nx * w1, y1 - ny * w1), (x0 - nx * w0, y0 - ny * w0)]
    return (lg("cg", x0, 0, x1, 0, [(0, green, 1), (1, ember, 1)]),
            f'<path d="M{"L".join(f"{x:.1f} {y:.1f}" for x, y in poly)}Z" fill="url(#cg)"/>'
            f'<circle cx="{x1:.1f}" cy="{y1:.1f}" r="{w1+1.4:.1f}" fill="{ember}"/>')

def anvil(stroke, w, squash, veil=None):
    p = M.anvil_path(top=TOP)
    tr = f' transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"'
    out = ""
    if veil:
        out += f'<path d="{p}" fill="{veil}"{tr}/>'
    return out + f'<path fill="none" stroke="{stroke}" stroke-width="{w}" stroke-linejoin="round" vector-effect="non-scaling-stroke" d="{p}"{tr}/>'

def scene(fill="heavy", anvil_stroke="#7d848a", veil=None, squash=0.7, w=5, reflect=True, spectre=False):
    defs, parts = [], []
    mid = [(.45, YEL, 1)] if spectre else []
    # emission paint: born green on its rising flank, orange from the maximum on
    defs.append(lg("emh", M.MU_E - 24, 0, apex_e[0] - 1, 0, [(0, green, 1)] + mid + [(1 if not spectre else 1, ember, 1)]))
    defs.append(lg("abv", 0, peak_y - sw / 2, 0, TOP, [(0, green, 1), (1, green, 1)]))
    defs.append(lg("whiten", 0, peak_y - sw / 2, 0, TOP, [(0, "#ffffff", 0), (.35, "#ffffff", 0), (1, "#ffffff", .42)]))
    defs.append(lg("fa", 0, peak_y, 0, TOP, [(0, green, .75), (1, green, .3)]))
    defs.append(lg("fe", M.MU_E - 24, 0, apex_e[0] - 1, 0, [(0, green, .75)] + [(o, c, .75) for o, c, _ in mid] + [(1, ember, .8)]))
    cd, cpath = comet(); defs.append(cd)
    defs.append(lg("vmg", 0, peak_y - sw / 2, 0, TOP, [(0, "#ffffff", 1), (.3, "#ffffff", 1), (1, "#9c9c9c", 1)]))
    defs.append(f'<mask id="vm" maskUnits="userSpaceOnUse" x="0" y="0" width="256" height="256"><rect width="256" height="256" fill="url(#vmg)"/></mask>')
    if reflect:
        depth = 46 * squash
        defs.append(lg("fade", 0, TOP, 0, TOP + depth, [(0, "#ffffff", .5), (1, "#ffffff", 0)]))
        defs.append(f'<mask id="m"><rect x="0" y="{TOP}" width="256" height="{depth+4:.0f}" fill="url(#fade)"/></mask>')
    parts.append(f'<defs>{"".join(defs)}</defs>')
    parts.append(anvil(anvil_stroke, w, squash, veil))
    parts.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
                 f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'
    if fill == "heavy":                     # strong translucent fill under a visible stroke
        parts.append(f'<polygon points="{area(EM)}" fill="url(#fe)"/><polygon points="{area(AB)}" fill="url(#fa)"/>')
        parts.append(f'<polyline {line} stroke="url(#emh)" points="{em_s}"/><polyline {line} stroke="{green}" points="{ab_s}"/>')
    else:                                   # solid: the stroke takes the fill's paint, only the shape is seen
        for pts, paint in ((AB, "url(#abv)"), (EM, "url(#emh)")):
            a_ = area(pts, base=TOP - 3)
            top = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
            parts.append(f'<g mask="url(#vm)"><polygon points="{a_}" fill="{paint}"/>'
                         f'<polyline points="{top}" fill="none" stroke="{paint}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="butt"/></g>')
    if reflect:
        k = squash
        rp = lambda pts: [(x, TOP + 6 + (TOP - y) * 0.55 * k) for x, y in pts]
        if fill == "heavy":
            refl = lambda pts: " ".join(f"{x:.1f},{y:.1f}" for x, y in rp(pts))
            parts.append(f'<g mask="url(#m)"><polyline fill="none" stroke-width="6" stroke-linecap="round" stroke="{green}" points="{refl(AB)}"/>'
                         f'<polyline fill="none" stroke-width="6" stroke-linecap="round" stroke="{ember}" points="{refl(EM)}"/></g>')
        else:
            poly = lambda pts: " ".join(f"{x:.1f},{y:.1f}" for x, y in rp(pts)) + f" {pts[-1][0]:.1f},{TOP+6} {pts[0][0]:.1f},{TOP+6}"
            parts.append(f'<g mask="url(#m)"><polygon points="{poly(EM)}" fill="url(#emh)"/><polygon points="{poly(AB)}" fill="url(#abv)"/></g>')
    parts.append(E.hammer_at(th, PIV, ink))
    parts.append(E.impact(hit, ember, BG))
    parts.append(cpath)
    return M.tile("".join(parts), bg=BG)

variants = {
  "q1-rempli-fort":            dict(fill="heavy"),
  "q2-plein":                  dict(fill="solid"),
  "q3-plein-voile":            dict(fill="solid", veil="#e6e4de"),
  "q4-rempli-fort-voile":      dict(fill="heavy", veil="#e6e4de"),
  "q5-plein-voile-spectre":    dict(fill="solid", veil="#e6e4de", spectre=True),
  "q6-plein-sans-reflet":      dict(fill="solid", veil="#e6e4de", reflect=False),
}
for n, kw in variants.items():
    open(os.path.join(OUT, f"{n}.svg"), "w").write(M.svg(scene(**kw), "StokesForge"))
print("ok")
