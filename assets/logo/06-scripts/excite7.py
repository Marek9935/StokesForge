import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import excite6 as G
Q, V, E, M = G.Q, G.V, G.E, G.M
OUT = sys.argv[1]
TOP, AX, H, sw = G.TOP, G.AX, G.H, G.sw
peak_y = TOP - H
AB, EM, ABT, EMT = G.AB, G.EM, G.ABT, G.EMT
apex_a, apex_et, hit, th, PIV = G.apex_a, G.apex_et, G.hit, G.th, G.PIV
lg = V.lg
GREEN, EMBER, DEEP = "#3a9155", "#e0651a", "#b8420f"

def area(pts, base=TOP):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{base} {pts[0][0]:.1f},{base}"

def scene(squash=0.72, halo=1.6, strip=5.0, night=False, refl_op=.9):
    ink = "#e6e9e7" if night else "#22262a"
    bg = "#15181a" if night else "#f3f1ec"
    ap = M.anvil_path(top=TOP)
    tr = f'transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"'
    hgt = 84 * squash
    x0, x1 = AB[0][0], EM[-1][0]
    xa, xe = apex_a[0], apex_et[0]
    f = lambda x: (x - x0) / (x1 - x0)
    D = []
    D.append(lg("ga", 0, peak_y - 5, 0, TOP, [(0, "#3f9c5c", 1), (1, "#22603a", 1)]))
    D.append(lg("ge", 0, peak_y - 5, 0, TOP, [(0, "#f39a3c", 1), (.35, EMBER, 1), (1, DEEP, 1)]))
    glass = [(0, "#4b525a", .96), (.45, "#2a2f35", .96), (1, "#1b1e22", .98)] if not night else [(0, "#5f6770", .95), (.45, "#3c434a", .95), (1, "#2b3035", .97)]
    D.append(lg("glass", 0, TOP, 0, TOP + hgt, glass))
    D.append(lg("spec", 0, TOP - 2, 0, TOP + 18 * squash, [(0, "#ffffff", .95), (.4, "#ffffff", .35), (1, "#ffffff", 0)]))
    D.append(lg("specb", 0, TOP + hgt - 14, 0, TOP + hgt + 2, [(0, "#ffffff", 0), (1, "#ffffff", .28)]))
    # the light strip on the table: coloured by what stands above it
    stops = [(0, "#7fdc9a", 0), (max(0, f(xa) - .3), "#7fdc9a", .9), (f(xa), "#a8f0bc", 1), (f((xa + xe) / 2), "#f2e58a", 1),
             (f(xe), "#ffe7a8", 1), (min(1, f(xe) + .22), "#ffb060", .95), (1, "#ffb060", 0)]
    D.append(lg("strip", x0, 0, x1, 0, stops))
    D.append(f'<clipPath id="ac"><path d="{ap}" {tr}/></clipPath>')
    D.append(lg("fade", 0, TOP, 0, TOP + 40 * squash, [(0, "#ffffff", 1), (1, "#ffffff", 0)]))
    D.append(f'<mask id="m"><rect x="0" y="{TOP}" width="256" height="{hgt:.0f}" fill="url(#fade)"/></mask>')
    D.append('<filter id="blur" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="9"/></filter>')
    D.append('<filter id="b3" x="-20%" y="-200%" width="140%" height="500%"><feGaussianBlur stdDeviation="3.2"/></filter>')
    D.append('<filter id="b1" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="1.4"/></filter>')
    D.append(f'<radialGradient id="sh"><stop offset="0" stop-color="#ffffff" stop-opacity=".32"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/></radialGradient>')
    cd, cpath = Q.comet(); D.append(cd)
    P = [f'<defs>{"".join(D)}</defs>']
    # light behind the bands
    P.append(f'<polygon points="{area(EMT)}" fill="#ffa040" opacity="{.5*halo:.2f}" filter="url(#blur)"/>')
    P.append(f'<polygon points="{area(ABT)}" fill="{GREEN}" opacity="{.24*halo:.2f}" filter="url(#blur)"/>')
    # dark glass anvil
    P.append(f'<path d="{ap}" fill="url(#glass)" {tr}/>')
    rp = lambda pts: " ".join(f"{x:.1f},{TOP + strip + (TOP - y) * 0.55 * squash:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{TOP} {pts[0][0]:.1f},{TOP}"
    P.append(f'<g clip-path="url(#ac)">'
             f'<g mask="url(#m)" opacity="{refl_op}" filter="url(#b1)"><polygon points="{rp(ABT)}" fill="#46b06a"/><polygon points="{rp(EMT)}" fill="#f08a3a"/></g>'
             # specular rim along the real (flat) edges, then a faint one at the bottom
             f'<path d="{ap}" {tr} fill="none" stroke="url(#spec)" stroke-width="3.2" vector-effect="non-scaling-stroke"/>'
             f'<path d="{ap}" {tr} fill="none" stroke="url(#specb)" stroke-width="2.4" vector-effect="non-scaling-stroke"/>'
             f'</g>')
    # y axis
    P.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="5"/>'
             f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    # filled bands, standing on the table
    P.append(f'<polygon points="{area(ABT)}" fill="url(#ga)"/>')
    P.append(f'<polygon points="{area(EMT)}" fill="url(#ge)"/>')
    # the diffracting strip on top of the table, in front of the bands' feet, glowing
    P.append(f'<rect x="{x0:.1f}" y="{TOP-4:.1f}" width="{x1-x0:.1f}" height="{strip+3}" fill="url(#strip)" opacity=".95" filter="url(#b3)"/>')
    P.append(f'<rect x="{x0:.1f}" y="{TOP-strip/2:.1f}" width="{x1-x0:.1f}" height="{strip}" rx="{strip/2:.1f}" fill="url(#strip)"/>')
    for (ax_, ay_) in (apex_a, apex_et):     # discreet glassy sheen on the left flank
        P.append(f'<ellipse cx="{ax_-6:.1f}" cy="{ay_+28:.1f}" rx="5" ry="15" fill="url(#sh)" transform="rotate(16 {ax_-6:.1f} {ay_+28:.1f})"/>')
    P.append(E.hammer_at(th, PIV, ink))
    P.append(E.impact(hit, EMBER, bg))
    P.append(cpath)
    return M.tile("".join(P), bg=bg)

variants = {
  "s1-verre": dict(),
  "s2-verre-bande-forte": dict(strip=7.0, halo=1.8),
  "s3-verre-reflet-fort": dict(refl_op=1.0, halo=1.4),
  "s4-verre-nuit": dict(night=True),
}
for n, kw in variants.items():
    open(os.path.join(OUT, f"{n}.svg"), "w").write(M.svg(scene(**kw), "StokesForge"))
print("ok")
