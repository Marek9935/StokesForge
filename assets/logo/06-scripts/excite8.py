import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import excite7 as S
G, Q, V, E, M = S.G, S.Q, S.V, S.E, S.M
OUT = sys.argv[1]
TOP, AX, H, sw = S.TOP, S.AX, S.H, S.sw
peak_y = TOP - H
AB, EM, ABT, EMT = S.AB, S.EM, S.ABT, S.EMT
apex_a, apex_et, hit, th, PIV = S.apex_a, S.apex_et, S.hit, S.th, S.PIV
lg, area = V.lg, S.area
GREEN, EMBER, DEEP = "#3a9155", "#e0651a", "#b8420f"

def anvil_path(top=TOP, right=236, left=96, tip=18):
    """Same body as before; the horn is now a cone: both edges converge on a rounded tip at mid-height."""
    return (f"M{left} {top}H{right}V{top+12}Q{right} {top+20} {right-10} {top+21}"
            f"C{right-32} {top+24} {right-42} {top+32} {right-42} {top+46}"
            f"C{right-42} {top+62} {right-30} {top+70} {right-14} {top+73}L{right-12} {top+84}"
            f"H{left+4}L{left+6} {top+73}C{left+22} {top+70} {left+32} {top+62} {left+32} {top+46}"
            f"C{left+32} {top+36} {left+24} {top+32} {left+8} {top+32}"
            f"C{left-24} {top+31} {tip+34} {top+26} {tip+5} {top+22.5}"          # underside of the cone
            f"C{tip-1} {top+21.6} {tip-1} {top+17.4} {tip+5} {top+16.5}"        # rounded tip
            f"C{tip+34} {top+13} {left-26} {top+8} {left} {top+8}Z")            # top of the cone, up to the step

def scene(squash=0.72, halo=1.8, night=False, refl_op=.9, inner=0.6):
    ink = "#e6e9e7" if night else "#22262a"
    bg = "#15181a" if night else "#f3f1ec"
    ap = anvil_path()
    tr = f'transform="translate(0 {TOP}) scale(1 {squash}) translate(0 {-TOP})"'
    hgt = 84 * squash
    x0, x1 = AB[0][0], EM[-1][0]
    xa, xe = apex_a[0], apex_et[0]
    f = lambda x: (x - x0) / (x1 - x0)
    D = []
    D.append(lg("ga", 0, peak_y - 5, 0, TOP, [(0, "#3f9c5c", 1), (1, "#22603a", 1)]))
    D.append(lg("ge", 0, peak_y - 5, 0, TOP, [(0, "#f39a3c", 1), (.35, EMBER, 1), (1, DEEP, 1)]))
    glass = [(0, "#3f464d", .97), (.5, "#262a2f", .97), (1, "#1a1d21", .98)] if not night else [(0, "#5a626b", .95), (.5, "#3a4148", .95), (1, "#2a2f35", .97)]
    D.append(lg("glass", 0, TOP, 0, TOP + hgt, glass))
    D.append(lg("spec", 0, TOP - 2, 0, TOP + 14 * squash, [(0, "#ffffff", .45), (1, "#ffffff", 0)]))
    stops = [(0, "#7fdc9a", 0), (max(0, f(xa) - .3), "#7fdc9a", .9), (f(xa), "#a8f0bc", 1), (f((xa + xe) / 2), "#f2e58a", 1),
             (f(xe), "#ffe7a8", 1), (min(1, f(xe) + .22), "#ffb060", .95), (1, "#ffb060", 0)]
    D.append(lg("strip", x0, 0, x1, 0, stops))
    D.append(f'<clipPath id="ac"><path d="{ap}" {tr}/></clipPath>')
    D.append(lg("fade", 0, TOP, 0, TOP + 40 * squash, [(0, "#ffffff", 1), (1, "#ffffff", 0)]))
    D.append(f'<mask id="m"><rect x="0" y="{TOP}" width="256" height="{hgt:.0f}" fill="url(#fade)"/></mask>')
    # horn highlight: a soft streak along the cone, thicker at the root, vanishing at the tip
    D.append(lg("hornhl", 24, 0, 100, 0, [(0, "#ffffff", 0), (.35, "#ffffff", .55), (1, "#ffffff", .7)]))
    for i, sd in ((9, 9), (4, 4), (2, 1.4), (5, 5.5)):
        D.append(f'<filter id="b{i}" x="-50%" y="-200%" width="200%" height="500%"><feGaussianBlur stdDeviation="{sd}"/></filter>')
    cd, cpath = Q.comet(); D.append(cd)
    P = [f'<defs>{"".join(D)}</defs>']
    P.append(f'<polygon points="{area(EMT)}" fill="#ffa040" opacity="{.5*halo:.2f}" filter="url(#b9)"/>')
    P.append(f'<polygon points="{area(ABT)}" fill="{GREEN}" opacity="{.24*halo:.2f}" filter="url(#b9)"/>')
    P.append(f'<path d="{ap}" fill="url(#glass)" {tr}/>')
    hy = TOP + 15.0 * squash      # upper third of the cone after the squash
    rp = lambda pts: " ".join(f"{x:.1f},{TOP + 4 + (TOP - y) * 0.55 * squash:.1f}" for x, y in pts) + f" {pts[-1][0]:.1f},{TOP} {pts[0][0]:.1f},{TOP}"
    P.append(f'<g clip-path="url(#ac)">'
             # liquid light: the spectra's colour pours into the glass
             f'<ellipse cx="{xa:.1f}" cy="{TOP+10:.1f}" rx="34" ry="16" fill="#46b06a" opacity="{inner:.2f}" filter="url(#b9)"/>'
             f'<ellipse cx="{xe:.1f}" cy="{TOP+10:.1f}" rx="36" ry="17" fill="#ff9a3c" opacity="{inner+.1:.2f}" filter="url(#b9)"/>'
             f'<g mask="url(#m)" opacity="{refl_op}" filter="url(#b2)"><polygon points="{rp(ABT)}" fill="#46b06a"/><polygon points="{rp(EMT)}" fill="#f08a3a"/></g>'
             # the lit surface: a luminous line just under the face, spreading into the glass (nothing sticks out above)
             f'<rect x="{x0:.1f}" y="{TOP:.1f}" width="{x1-x0:.1f}" height="7" fill="url(#strip)" opacity=".85" filter="url(#b4)"/>'
             f'<rect x="{x0:.1f}" y="{TOP:.1f}" width="{x1-x0:.1f}" height="2.2" fill="url(#strip)"/>'
             # soft rim on the real edges, and the round horn's highlight
             f'<path d="{ap}" {tr} fill="none" stroke="url(#spec)" stroke-width="2.4" vector-effect="non-scaling-stroke" filter="url(#b2)"/>'
             f'<path d="M28 {hy:.1f}C52 {hy-2.0:.1f} 76 {hy-3.4:.1f} 100 {hy-4.0:.1f}" stroke="url(#hornhl)" stroke-width="3.2" stroke-linecap="round" fill="none" filter="url(#b2)"/>'
             f'</g>')
    # a soft glow just above the face (no hard edge)
    P.append(f'<rect x="{x0:.1f}" y="{TOP-5:.1f}" width="{x1-x0:.1f}" height="6" fill="url(#strip)" opacity=".45" filter="url(#b5)"/>')
    P.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="5"/>'
             f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    P.append(f'<polygon points="{area(ABT)}" fill="url(#ga)"/>')
    P.append(f'<polygon points="{area(EMT)}" fill="url(#ge)"/>')
    P.append(E.hammer_at(th, PIV, ink))
    P.append(E.impact(hit, EMBER, bg))
    P.append(cpath)
    return M.tile("".join(P), bg=bg)

variants = {
  "t1-liquide": dict(),
  "t2-liquide-intense": dict(inner=0.85, halo=2.0),
  "t3-liquide-doux": dict(inner=0.45, halo=1.5, refl_op=.75),
  "t4-liquide-nuit": dict(night=True, inner=0.8),
}
for n, kw in variants.items():
    open(os.path.join(OUT, f"{n}.svg"), "w").write(M.svg(scene(**kw), "StokesForge"))
print("ok")
