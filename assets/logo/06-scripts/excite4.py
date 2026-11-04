import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import excite3 as V            # base pieces (comet, sparkle, flank, hammer on the absorption, Stokes 34)
X, D, E, M = V.X, V.D, V.E, V.M
OUT = sys.argv[1]
TOP, AX, H, sw = V.TOP, V.AX, V.H, V.sw
peak_y = TOP - H
AB, EM, ab_s, em_s = V.AB, V.EM, V.ab_s, V.em_s
apex_e, hit, th, PIV = V.apex_e, V.hit, V.th, V.PIV
YEL = V.YEL
line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'

def area(pts):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{TOP-3} {pts[0][0]:.1f},{TOP-3}"

def anvil(ink, w, squash=1.0, details=False, polish=False):
    p = M.anvil_path(top=TOP)
    tr = f' transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"' if squash != 1 else ""
    out = f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" vector-effect="non-scaling-stroke" d="{p}"{tr}/>'
    if details:   # hardy hole (square) and pritchel hole (round), seen on the side of the table
        y = TOP + 6 + 4 * (squash - 1)
        out += (f'<rect x="205" y="{y:.1f}" width="8.5" height="8.5" rx="1.5" fill="none" stroke="{ink}" stroke-width="{w*0.6:.1f}"/>'
                f'<circle cx="223" cy="{y+4.25:.1f}" r="3.4" fill="none" stroke="{ink}" stroke-width="{w*0.6:.1f}"/>')
    if polish:    # a short highlight on the horn: polished steel
        out += f'<path d="M40 {TOP+12.5}L84 {TOP+13.5}" stroke="#ffffff" stroke-opacity=".55" stroke-width="1.6" stroke-linecap="round"/>'
    return out

def stars(color):
    s = [V.sparkle(48, 46, 5, color), V.sparkle(226, 30, 4, color), V.sparkle(150, 22, 3.2, color)]
    dots = [(30, 90, 1.3), (70, 26, 1.1), (118, 40, 1.0), (196, 52, 1.2), (240, 78, 1.0), (176, 18, 0.9), (92, 62, 0.8)]
    return "".join(s) + "".join(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{color}" opacity=".75"/>' for x, y, r in dots)

def scene(fill=True, reflect=True, details=False, polish=False, squash=1.0, w=6, night=False, spectre=False, discreet_ink=None):
    ink, green, ember = M.Dk if night else M.L
    bg = "#15181a" if night else "#f3f1ec"
    anvil_ink = discreet_ink or ink
    cmid = YEL if spectre else None
    defs = [V.flank(green, ember, cmid=cmid)]
    cd, cpath, tip = V.comet(green, ember, cmid=cmid)
    defs.append(cd)
    if fill:
        defs.append(V.lg("fa", 0, peak_y, 0, TOP, [(0, green, .34 if not night else .45), (1, green, 0)]))
        defs.append(V.lg("fe", 0, peak_y, 0, TOP, [(0, ember, .40 if not night else .55), (1, ember, 0)]))
    if reflect:
        depth = 46 * squash
        defs.append(V.lg("fade", 0, TOP, 0, TOP + depth, [(0, "#ffffff", .5), (1, "#ffffff", 0)]))
        defs.append(f'<mask id="m"><rect x="0" y="{TOP}" width="{198 if details else 256}" height="{depth+4:.0f}" fill="url(#fade)"/></mask>')
    if night:
        defs.append('<filter id="bl" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="4"/></filter>')
    parts = [f'<defs>{"".join(defs)}</defs>']
    if night:
        parts.append(stars(YEL))
    parts.append(anvil(anvil_ink, w, squash, details, polish))
    parts.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
                 f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    if fill:
        parts.append(f'<polygon points="{area(AB)}" fill="url(#fa)"/><polygon points="{area(EM)}" fill="url(#fe)"/>')
    if night:
        parts.append(f'<polyline {line} stroke="{green}" stroke-opacity=".6" filter="url(#bl)" points="{ab_s}"/>'
                     f'<polyline {line} stroke="{ember}" stroke-opacity=".75" filter="url(#bl)" points="{em_s}"/>')
    parts.append(f'<polyline {line} stroke="url(#flank)" points="{em_s}"/>')
    parts.append(f'<polyline {line} stroke="{green}" points="{ab_s}"/>')
    if reflect:
        k = squash
        refl = lambda s: " ".join(f"{float(x):.1f},{TOP + 6 + (TOP - float(y)) * 0.55 * k:.1f}" for x, y in (p.split(',') for p in s.split()))
        parts.append(f'<g mask="url(#m)" opacity="{.8 if night else 1}"><polyline fill="none" stroke-width="6" stroke-linecap="round" stroke="{green}" points="{refl(ab_s)}"/>'
                     f'<polyline fill="none" stroke-width="6" stroke-linecap="round" stroke="{ember}" points="{refl(em_s)}"/></g>')
    parts.append(E.hammer_at(th, PIV, ink))
    parts.append(E.impact(hit, YEL if night else ember, bg))
    parts.append(cpath)
    parts.append(V.sparkle(tip[0] + 1, tip[1] - 1, 10.5, YEL if night else ember, outline=bg))
    parts.append(V.sparkle(tip[0] + 16, tip[1] + 12, 5, YEL if night else ember))
    return M.tile("".join(parts), bg=bg)

variants = {
  "p1-reflet-rempli":        dict(),
  "p2-reflet-organique":     dict(details=True, polish=True, squash=0.82),
  "p3-discret":              dict(details=True, squash=0.7, w=4, discreet_ink="#5b6168"),
  "p4-spectre":              dict(details=True, polish=True, squash=0.82, spectre=True),
  "p5-nuit-etoiles":         dict(details=True, squash=0.82, night=True, spectre=True),
  "p6-nuit-discret":         dict(details=True, squash=0.7, w=4, night=True, spectre=True, discreet_ink="#9aa29e"),
}
for n, kw in variants.items():
    open(os.path.join(OUT, f"{n}.svg"), "w").write(M.svg(scene(**kw), "StokesForge"))
print("ok", list(variants))
