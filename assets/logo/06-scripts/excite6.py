import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import excite5 as Q            # comet (wedge + dot), spectra, hammer on the absorption maximum
V, E, M = Q.V, Q.E, Q.M
OUT = sys.argv[1]
TOP, AX, H, sw = Q.TOP, Q.AX, Q.H, Q.sw
peak_y = TOP - H
AB, EM = Q.AB, Q.EM
apex_e, hit, th, PIV = Q.apex_e, Q.hit, Q.th, Q.PIV
lg = V.lg
GREEN, EMBER, HOT, DEEP = "#3a9155", "#e0651a", "#ffd166", "#b8420f"
GDARK = "#24683a"
base_y = Q.D.base

def thick(pts):
    # the filled band's top edge sits where the old stroke's outer edge was (half a stroke above at the top, nothing at the feet)
    hh = Q.D.hh
    return [(x, y - (sw / 2) * max(0.0, (base_y - y) / hh)) for x, y in pts]
def area(pts, base=TOP - 3):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{base} {pts[0][0]:.1f},{base}"
ABT, EMT = thick(AB), thick(EM)
apex_a = min(ABT, key=lambda q: q[1]); apex_et = min(EMT, key=lambda q: q[1])

def radial(id_, stops):
    return f'<radialGradient id="{id_}">' + "".join(f'<stop offset="{o}" stop-color="{c}" stop-opacity="{a}"/>' for o, c, a in stops) + '</radialGradient>'

def scene(squash=0.72, night=False, halo=1.0, sheen=True, reflect=True, table_glow=True):
    ink = "#e6e9e7" if night else "#22262a"
    bg = "#15181a" if night else "#f3f1ec"
    defs = []
    defs.append(lg("ga", 0, peak_y - 5, 0, TOP, [(0, "#4aa866" if not night else "#5cc07d", 1), (1, GDARK, 1)]))
    defs.append(lg("ge", 0, peak_y - 5, 0, TOP, [(0, HOT, 1), (.35, "#f39a3c", 1), (.7, EMBER, 1), (1, DEEP, 1)]))
    # dark glass anvil (darkens what is behind it so the bands stand out)
    glass = [(0, "#5a626b", .95), (.5, "#3a4148", .95), (1, "#2a2f35", .97)] if night else [(0, "#3a4047", .96), (.5, "#23272c", .96), (1, "#16191c", .98)]
    defs.append(lg("glass", 0, TOP, 0, TOP + 84 * squash, glass))
    defs.append(lg("rim", 0, 0, 256, 0, [(0, "#ffffff", 0), (.35, "#ffffff", .5), (.75, "#ffffff", .25), (1, "#ffffff", 0)]))
    defs.append(lg("tbl", AB[0][0], 0, EM[-1][0], 0, [(0, GREEN, 0), (.12, GREEN, 1), (.45, "#8aa64a", 1), (.6, EMBER, 1), (.85, HOT, 1), (1, EMBER, 0)]))
    defs.append(radial("sh", [(0, "#ffffff", .55), (1, "#ffffff", 0)]))
    defs.append('<filter id="blur" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="9"/></filter>')
    defs.append('<filter id="blur2" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="2.2"/></filter>')
    if reflect:
        depth = 46 * squash
        defs.append(lg("fade", 0, TOP, 0, TOP + depth, [(0, "#ffffff", .55), (1, "#ffffff", 0)]))
        defs.append(f'<mask id="m"><rect x="0" y="{TOP}" width="256" height="{depth+4:.0f}" fill="url(#fade)"/></mask>')
        defs.append(f'<clipPath id="ac"><path d="{M.anvil_path(top=TOP)}" transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"/></clipPath>')
    cd, cpath = Q.comet(); defs.append(cd)
    P = [f'<defs>{"".join(defs)}</defs>']
    # halos first (light behind the bands)
    if halo:
        P.append(f'<polygon points="{area(EMT)}" fill="#ffb347" opacity="{.55*halo:.2f}" filter="url(#blur)"/>')
        P.append(f'<polygon points="{area(ABT)}" fill="{GREEN}" opacity="{.25*halo:.2f}" filter="url(#blur)"/>')
    # anvil of dark glass
    tr = f'transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"'
    P.append(f'<path d="{M.anvil_path(top=TOP)}" fill="url(#glass)" {tr}/>')
    if reflect:
        rp = lambda pts: " ".join(f"{x:.1f},{TOP + 4 + (TOP - y) * 0.6 * squash:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{TOP+2} {pts[0][0]:.1f},{TOP+2}"
        P.append(f'<g clip-path="url(#ac)"><g mask="url(#m)" opacity=".75"><polygon points="{rp(ABT)}" fill="url(#ga)"/><polygon points="{rp(EMT)}" fill="url(#ge)"/></g></g>')
    P.append(f'<path d="M40 {TOP+9.5}L{230} {TOP+1.6}" stroke="url(#rim)" stroke-width="{2.2 if night else 1.4}" stroke-linecap="round"/>')
    # y axis
    P.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="5"/>'
             f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    # filled bands, no stroke
    P.append(f'<polygon points="{area(ABT)}" fill="url(#ga)"/>')
    P.append(f'<polygon points="{area(EMT)}" fill="url(#ge)"/>')
    if sheen:   # liquid-glass sheen on each maximum
        for (ax_, ay_), rx, ry in ((apex_a, 7, 16), (apex_et, 7, 16)):
            P.append(f'<ellipse cx="{ax_-5:.1f}" cy="{ay_+20:.1f}" rx="{rx}" ry="{ry}" fill="url(#sh)" transform="rotate(14 {ax_-5:.1f} {ay_+20:.1f})"/>')
        P.append(f'<ellipse cx="{apex_et[0]:.1f}" cy="{apex_et[1]+4:.1f}" rx="6" ry="5" fill="#fff6d8" opacity=".85" filter="url(#blur2)"/>')
    if table_glow:   # the table lights up under the bands
        P.append(f'<path d="M{AB[0][0]:.1f} {TOP-1}H{EM[-1][0]:.1f}" stroke="url(#tbl)" stroke-width="4" stroke-linecap="round"/>')
        P.append(f'<path d="M{apex_et[0]-26:.1f} {TOP}H{apex_et[0]+26:.1f}" stroke="{HOT}" stroke-width="6" opacity=".5" filter="url(#blur2)"/>')
    P.append(E.hammer_at(th, PIV, ink))
    P.append(E.impact(hit, EMBER if not night else HOT, bg))
    P.append(cpath)
    return M.tile("".join(P), bg=bg)

variants = {
  "r1-verre-sombre": dict(),
  "r2-verre-sans-reflet": dict(reflect=False),
  "r3-verre-lumineux": dict(halo=1.6),
  "r4-verre-sobre": dict(halo=0.5, sheen=False),
  "r5-verre-nuit": dict(night=True, halo=1.3),
}
for n, kw in variants.items():
    open(os.path.join(OUT, f"{n}.svg"), "w").write(M.svg(scene(**kw), "StokesForge"))
print("ok")
