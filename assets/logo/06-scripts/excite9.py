import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import excite8 as T
S, G, Q, V, E, M = T.S, T.G, T.Q, T.V, T.E, T.M
OUT = sys.argv[1]
TOP, AX, H, sw = T.TOP, T.AX, T.H, T.sw
peak_y = TOP - H
AB, EM = T.AB, T.EM
hit, th, PIV = T.hit, T.th, T.PIV
lg = V.lg
GREEN, EMBER, DEEP = "#3a9155", "#e0651a", "#b8420f"
base_y, hh = Q.D.base, Q.D.hh

def filled(pts):
    # rescale the band so its feet sit exactly on the face (y = TOP) and its top keeps the old stroke's outer edge
    k = (hh + sw / 2 + (TOP - base_y)) / hh
    return [(x, TOP - (base_y - y) * k) for x, y in pts]
ABF, EMF = filled(AB), filled(EM)
apex_a = min(ABF, key=lambda q: q[1]); apex_e = min(EMF, key=lambda q: q[1])
def area(pts):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{TOP} {pts[0][0]:.1f},{TOP}"
def edge(pts):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)

def anvil_path(top=TOP, right=236, left=96, tip=38):
    return (f"M{left} {top}H{right}V{top+12}Q{right} {top+20} {right-10} {top+21}"
            f"C{right-32} {top+24} {right-42} {top+32} {right-42} {top+46}"
            f"C{right-42} {top+62} {right-30} {top+70} {right-14} {top+73}L{right-12} {top+84}"
            f"H{left+4}L{left+6} {top+73}C{left+22} {top+70} {left+32} {top+62} {left+32} {top+46}"
            f"C{left+32} {top+36} {left+24} {top+32} {left+8} {top+32}"
            f"L{tip+6} {top+22.4}"                                              # straight underside of the cone
            f"C{tip-1} {top+21.4} {tip-1} {top+17.6} {tip+6} {top+16.6}"        # rounded tip
            f"L{left} {top+8}Z")                                                # straight top of the cone

def scene(squash=0.72, night=False, inner=0.85, edge_boost=1.0, rim=True, edge_sd=5, dark_seam=False, edge_stroke=False):
    ink = "#e6e9e7" if night else "#22262a"
    bg = "#15181a" if night else "#f3f1ec"
    ap = anvil_path()
    tr = f'transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"'
    hgt = 84 * squash
    x0, x1 = AB[0][0], EM[-1][0]
    xa, xe = apex_a[0], apex_e[0]
    f = lambda x: (x - x0) / (x1 - x0)
    D = []
    D.append(lg("ga", 0, peak_y - 5, 0, TOP, [(0, "#3f9c5c", 1), (1, "#22603a", 1)]))
    D.append(lg("ge", 0, peak_y - 5, 0, TOP, [(0, "#f39a3c", 1), (.35, EMBER, 1), (1, DEEP, 1)]))
    glass = [(0, "#363c43", 1), (.5, "#24282d", 1), (1, "#191c20", 1)] if not night else [(0, "#4a5159", 1), (.5, "#333940", 1), (1, "#262a2f", 1)]
    D.append(lg("glass", 0, TOP, 0, TOP + hgt, glass))
    stops = [(0, "#7fdc9a", 0), (max(0, f(xa) - .3), "#7fdc9a", .9), (f(xa), "#a8f0bc", 1), (f((xa + xe) / 2), "#f2e58a", 1),
             (f(xe), "#ffe7a8", 1), (min(1, f(xe) + .22), "#ffb060", .95), (1, "#ffb060", 0)]
    D.append(lg("strip", x0, 0, x1, 0, stops))
    # edge light: the spectra's colours along the glass border, neutral elsewhere, fading downward
    D.append(lg("edgec", 20, 0, 236, 0, [(0, "#8a939c", 1), (f(xa) * (x1 - x0) / 216 + (x0 - 20) / 216, "#5fd17f", 1),
             ((xe - 20) / 216, "#ffb256", 1), (1, "#ff9a3c", 1)]))
    D.append(lg("emg", 0, TOP, 0, TOP + hgt, [(0, "#ffffff", 1), (.45, "#ffffff", .35), (1, "#ffffff", .12)]))
    D.append(f'<mask id="em" maskUnits="userSpaceOnUse" x="0" y="0" width="256" height="256"><rect width="256" height="256" fill="url(#emg)"/></mask>')
    # darker seam option: saturated mid-tones, the light sits under the face instead of on it
    D.append(lg("stripd", x0, 0, x1, 0, [(0, "#2f8a4d", 0), (max(0, f(xa) - .3), "#2f8a4d", .9), (f(xa), "#3aa860", 1),
             (f((xa + xe) / 2), "#9a8a32", .9), (f(xe), "#e88a2e", 1), (min(1, f(xe) + .22), "#d0661c", .9), (1, "#d0661c", 0)]))
    D.append(f'<clipPath id="ac"><path d="{ap}" {tr}/></clipPath>')
    D.append(f'<clipPath id="above"><rect x="0" y="0" width="256" height="{TOP}"/></clipPath>')
    D.append(lg("fade", 0, TOP, 0, TOP + 40 * squash, [(0, "#ffffff", 1), (1, "#ffffff", 0)]))
    D.append(f'<mask id="m"><rect x="0" y="{TOP}" width="256" height="{hgt:.0f}" fill="url(#fade)"/></mask>')
    D.append(lg("rimg", 0, TOP - 1, 0, TOP + 10, [(0, "#ffffff", .5), (1, "#ffffff", 0)]))
    D.append(lg("hornhl", 40, 0, 100, 0, [(0, "#ffffff", 0), (.4, "#ffffff", .45), (1, "#ffffff", .6)]))
    for i, sd in ((9, 9), (4, 4), (3, 3), (2, 1.5)):
        D.append(f'<filter id="b{i}" x="-50%" y="-100%" width="200%" height="300%"><feGaussianBlur stdDeviation="{sd}"/></filter>')
    # liquid edge: the light inside the glass gathers in the rounded border (no white gloss)
    b = 2.2 * edge_boost
    D.append('<filter id="liquid" x="-10%" y="-10%" width="120%" height="120%">'
             f'<feGaussianBlur in="SourceAlpha" stdDeviation="{edge_sd}" result="ba"/>'
             '<feComposite in="SourceAlpha" in2="ba" operator="arithmetic" k2="1" k3="-1" result="edge"/>'
             f'<feColorMatrix in="SourceGraphic" type="matrix" values="{b} 0 0 0 .06  0 {b} 0 0 .06  0 0 {b} 0 .06  0 0 0 1 0" result="bright"/>'
             '<feComposite in="bright" in2="edge" operator="in" result="edgelight"/>'
             '<feGaussianBlur in="edgelight" stdDeviation=".8" result="el"/>'
             '<feMerge><feMergeNode in="SourceGraphic"/><feMergeNode in="el"/><feMergeNode in="el"/></feMerge></filter>')
    cd, cpath = Q.comet(); D.append(cd)
    P = [f'<defs>{"".join(D)}</defs>']
    # the anvil: dark glass with the spectra's light poured inside, edges lit by that light
    rp = lambda pts: " ".join(f"{x:.1f},{TOP + 3 + (TOP - y) * 0.55 * squash:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{TOP} {pts[0][0]:.1f},{TOP}"
    P.append(f'<g {"" if edge_stroke else "filter=\"url(#liquid)\""}><g clip-path="url(#ac)">'
             f'<path d="{ap}" fill="url(#glass)" {tr}/>'
             f'<ellipse cx="{xa:.1f}" cy="{TOP+8:.1f}" rx="36" ry="16" fill="#46b06a" opacity="{inner:.2f}" filter="url(#b9)"/>'
             f'<ellipse cx="{xe:.1f}" cy="{TOP+8:.1f}" rx="38" ry="17" fill="#ff9a3c" opacity="{min(1, inner+.1):.2f}" filter="url(#b9)"/>'
             f'<g mask="url(#m)" opacity=".85" filter="url(#b2)"><polygon points="{rp(ABF)}" fill="#46b06a"/><polygon points="{rp(EMF)}" fill="#f08a3a"/></g>'
             + (f'<rect x="{x0:.1f}" y="{TOP+1.5:.1f}" width="{x1-x0:.1f}" height="9" fill="url(#stripd)" opacity=".9" filter="url(#b4)"/>'
                f'<rect x="96" y="{TOP:.1f}" width="140" height="2.2" fill="#121417"/>'
                if dark_seam else
                f'<rect x="{x0:.1f}" y="{TOP:.1f}" width="{x1-x0:.1f}" height="7" fill="url(#strip)" opacity=".85" filter="url(#b4)"/>'
                f'<rect x="{x0:.1f}" y="{TOP:.1f}" width="{x1-x0:.1f}" height="2" fill="url(#strip)"/>') +
             f'<path d="M44 {TOP+13*squash:.1f}L100 {TOP+9.4*squash:.1f}" stroke="url(#hornhl)" stroke-width="2.6" stroke-linecap="round" filter="url(#b2)"/>'
             + (f'<g mask="url(#em)"><path d="{ap}" {tr} fill="none" stroke="url(#edgec)" stroke-width="13" vector-effect="non-scaling-stroke" opacity=".9" filter="url(#b4)"/>'
                f'<path d="{ap}" {tr} fill="none" stroke="#9aa3ac" stroke-width="7" vector-effect="non-scaling-stroke" opacity=".45" filter="url(#b3)"/></g>' if edge_stroke else '') +
             f'</g></g>')
    if rim:
        P.append(f'<g clip-path="url(#ac)"><path d="{ap}" {tr} fill="none" stroke="url(#rimg)" stroke-width="2" vector-effect="non-scaling-stroke" filter="url(#b2)"/></g>')
    # y axis
    P.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="5"/>'
             f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    # a thin glow following only the top of each band (no halo at the feet), kept above the face
    P.append(f'<g clip-path="url(#above)">'
             f'<polyline points="{edge(EMF)}" fill="none" stroke="#ffa040" stroke-width="7" opacity=".55" filter="url(#b3)"/>'
             f'<polyline points="{edge(ABF)}" fill="none" stroke="#4fbf72" stroke-width="7" opacity=".35" filter="url(#b3)"/></g>')
    P.append(f'<polygon points="{area(ABF)}" fill="url(#ga)"/>')
    P.append(f'<polygon points="{area(EMF)}" fill="url(#ge)"/>')
    P.append(E.hammer_at(th, PIV, ink))
    P.append(E.impact(hit, EMBER, bg))
    P.append(cpath)
    return M.tile("".join(P), bg=bg)

variants = {
  "w1-final": dict(rim=False, dark_seam=True, edge_stroke=True),
  "w2-final-nuit": dict(rim=False, dark_seam=True, edge_stroke=True, night=True),
  "u1-liquide": dict(),
  "u2-liquide-bord-fort": dict(edge_boost=1.6),
  "u3-liquide-sans-liseré": dict(rim=False, edge_boost=1.3),
  "u4-liquide-nuit": dict(night=True, edge_boost=1.3),
}
for n, kw in variants.items():
    open(os.path.join(OUT, f"{n}.svg"), "w").write(M.svg(scene(**kw), "StokesForge"))
print("ok")
