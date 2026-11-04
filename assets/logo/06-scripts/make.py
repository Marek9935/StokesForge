import math, os, sys
OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

# The user's curve (polyline85 of drawing.svg), normalised: u in [0,1], v in [0,1] (1 = peak)
RAW = """93.75,282.125 96.75,282.125 99.6875,282.125 102.6875,282.125 105.625,282.0625 108.5625,282.0625 111.5625,282 114.5,281.9375 117.5,281.875 120.4375,281.75 123.375,281.625 126.375,281.4375 129.3125,281.1875 132.25,280.875 135.25,280.4375 138.1875,279.8125 141.1875,279.0625 144.125,278.125 147.0625,276.875 150.0625,275.375 153,273.5 156,271.1875 158.9375,268.3125 161.875,264.9375 164.875,260.875 167.8125,256.0625 170.8125,250.5 173.75,244 176.6875,236.5625 179.6875,228.1875 182.625,218.8125 185.625,208.5 188.5625,197.1875 191.5,185 194.5,172 197.4375,158.3125 200.4375,144.1875 203.375,129.75 206.3125,115.25 209.3125,100.9375 212.25,87.125 215.25,74 218.1875,62 221.125,51.25 224.125,42.0625 227.0625,34.6875 230,29.25 233,26 235.9375,24.875 238.9375,26 241.875,29.25 244.8125,34.6875 247.8125,42.0625 250.75,51.25 253.75,62 256.6875,74 259.625,87.125 262.625,100.9375 265.5625,115.25 268.5625,129.75 271.5,144.125 274.4375,158.25 277.4375,171.8125 280.375,184.6875 283.375,196.625 286.3125,207.5625 289.25,217.375 292.25,225.875 295.1875,233.0625 298.1875,238.6875 301.125,242.75 304.0625,245.125 307.0625,245.8125 310,244.625 313,241.6875 315.9375,237.125 318.875,231.125 321.875,223.9375 324.8125,216.0625 327.75,208 330.75,200.3125 333.6875,193.625 336.6875,188.4375 339.625,185.25 342.5625,184.3125 345.5625,185.8125 348.5,189.6875 351.5,195.5625 354.4375,203.125 357.375,211.8125 360.375,221.1875 363.3125,230.625 366.3125,239.6875 369.25,248.0625 372.1875,255.5 375.1875,261.8125 378.125,267.0625 381.125,271.1875 384.0625,274.4375 387,276.8125 390,278.5625 392.9375,279.8125 395.9375,280.6875 398.875,281.25 401.8125,281.5625 404.8125,281.8125 407.75,281.9375 410.75,282.0625"""
pts = [tuple(map(float, p.split(','))) for p in RAW.split()]
X0, X1, BASE, TOP = 93.75, 410.75, 282.125, 24.875
norm = [((x - X0) / (X1 - X0), (BASE - y) / (BASE - TOP)) for x, y in pts]
PEAK_U = (235.9375 - X0) / (X1 - X0)
SHIFT = 81.3 / (X1 - X0)   # horizontal offset between the two polylines of drawing.svg

def curve(x0, span, base, height, u_from=0.14, u_to=0.9, step=2):
    sel = [(u, v) for u, v in norm if u_from <= u <= u_to][::step]
    return " ".join(f"{x0+u*span:.1f},{base-v*height:.1f}" for u, v in sel)

C = dict(ink="#22262a", inkd="#e6e9e7", green="#3a9155", ember="#e0651a", greend="#5cc07d", emberd="#f4894a")

def anvil_path(top=160, right=236, left=96, horn_tip=20, step=10, base=244):
    # flat face, right underside, waist, flared feet, left waist, horn (pointed wedge, like the capture)
    return (f"M{left} {top}H{right}V{top+12}Q{right} {top+20} {right-10} {top+21}"
            f"C{right-32} {top+24} {right-42} {top+32} {right-42} {top+46}"
            f"C{right-42} {top+62} {right-30} {top+70} {right-14} {top+73}L{right-12} {base}"
            f"H{left+4}L{left+6} {top+73}C{left+22} {top+70} {left+32} {top+62} {left+32} {top+46}"
            f"C{left+32} {top+34} {left+24} {top+28} {left+8} {top+26}"
            f"C{left-16} {top+22} {horn_tip+30} {top+14} {horn_tip} {top+step}L{left} {top+step}Z")

def hammer(px, py, angle, ink, scale=1.0):
    s = scale
    # head: vertical block whose bottom face touches (px,py); handle to the right, raised by `angle`
    hw, hh = 26*s, 40*s
    head = f'<rect x="{px-hw/2:.1f}" y="{py-hh:.1f}" width="{hw:.1f}" height="{hh:.1f}" rx="{3*s:.1f}"/>'
    face = f'<rect x="{px-hw/2-3*s:.1f}" y="{py-9*s:.1f}" width="{hw+6*s:.1f}" height="{9*s:.1f}" rx="{2*s:.1f}"/>'
    cy = py - hh*0.58
    hx0 = px + hw/2 - 1
    L, t0, t1 = 82*s, 11*s, 8*s
    handle = (f'<path d="M{hx0:.1f} {cy-t0/2:.1f}L{hx0+L:.1f} {cy-t1/2:.1f}'
              f'Q{hx0+L+t1/2:.1f} {cy:.1f} {hx0+L:.1f} {cy+t1/2:.1f}L{hx0:.1f} {cy+t0/2:.1f}Z"/>')
    return f'<g fill="{ink}" transform="rotate({angle} {px} {py})">{head}{face}{handle}</g>'

def sparks(px, py, color, r0=10, r1=20, w=4):
    out = []
    for a in (-160, -125, -55, -20):
        ra = math.radians(a)
        out.append(f'<line x1="{px+r0*math.cos(ra):.1f}" y1="{py+r0*math.sin(ra)+8:.1f}" '
                   f'x2="{px+r1*math.cos(ra):.1f}" y2="{py+r1*math.sin(ra)+8:.1f}"/>')
    return f'<g stroke="{color}" stroke-width="{w}" stroke-linecap="round">{"".join(out)}</g>'

def logo(ink, green, ember, mono=False, icon=False):
    TOP, AX = 160, 104
    span, h = 126, 80
    x0 = AX - 12
    p1x = x0 + PEAK_U*span
    p2x = p1x + SHIFT*span
    peak_y = TOP - h
    sw = 10 if icon else 7
    c1, c2 = green, (green if mono else ember)
    # outline = axis line width; tick marks follow stokesforge-render's defaults
    # (pipeline.rs tick_length: major 5, minor ceil(5/2)=3, width 1, 5 subdivisions), scaled by w
    w = 6 if icon else 3.5
    major, minor = 5*w, 3*w
    step = 45
    ticks = []
    i = 0
    while True:
        x = AX + i*step/5
        if x > 236 - w*1.5: break
        L = major if i % 5 == 0 else minor
        if not icon:
            ticks.append(f"M{x:.1f} {TOP}v{L:.1f}")
        i += 1
    parts = [f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{anvil_path(top=TOP)}"/>',
             f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linecap="butt" d="{"".join(ticks)}"/>',
             f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>',
             f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>']
    if not icon:
        ay = TOP + 44
        parts.append(f'<path d="M{p1x:.1f} {ay}H{p2x-3:.1f}M{p2x-9:.1f} {ay-5}L{p2x-2:.1f} {ay}L{p2x-9:.1f} {ay+5}" fill="none" stroke="{ink}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"/>'
                     f'<circle cx="{p1x:.1f}" cy="{ay}" r="{w*0.9:.1f}" fill="{ink}"/>')
    parts += [f'<polyline fill="none" stroke="{c1}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round" points="{curve(x0, span, TOP-sw/2, h-sw/2)}"/>',
              f'<polyline fill="none" stroke="{c2}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round" points="{curve(x0+SHIFT*span, span, TOP-sw/2, h-sw/2)}"/>',
              sparks(p1x, peak_y - sw/2, c2 if not mono else ink, r0=7, r1=14, w=3 if not icon else 5),
              hammer(p1x, peak_y - sw/2 - 2, -14, ink, scale=0.6 if not icon else 0.75)]
    return parts

def svg(parts, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" role="img">'
            f'<title>{title}</title>' + "".join(parts) + "</svg>\n")

def icon_svg():
    # app icon: rounded tile, simplified mark scaled to fit
    parts = logo(C["ink"], C["green"], C["ember"], icon=True)
    inner = "".join(parts)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" role="img">'
            f'<title>StokesForge</title><rect width="256" height="256" rx="56" fill="#f3f1ec"/>'
            f'<g transform="translate(14 6) scale(.9)">{inner}</g></svg>\n')

def engraved():
    # variant B: the whole graph is engraved in a heavy anvil block; the hammer strikes its top face
    ink, green, ember = C["ink"], C["green"], C["ember"]
    TOP = 108
    body = (f"M70 {TOP}H240V{TOP+70}Q240 {TOP+80} 228 {TOP+82}C206 {TOP+86} 198 {TOP+94} 198 {TOP+106}"
            f"C198 {TOP+118} 208 {TOP+124} 222 {TOP+127}L224 {TOP+140}H86L88 {TOP+127}"
            f"C102 {TOP+124} 112 {TOP+118} 112 {TOP+106}C112 {TOP+94} 104 {TOP+86} 82 {TOP+82}"
            f"Q70 {TOP+80} 70 {TOP+70}V{TOP+46}C48 {TOP+42} 30 {TOP+30} 12 {TOP+14}L70 {TOP+14}Z")
    AX, BASE = 92, TOP + 64
    span, h = 104, 46
    x0 = AX - 4
    p1x = x0 + PEAK_U*span
    p2x = p1x + SHIFT*span
    cut = (f'<path d="M{AX} {BASE}V{BASE-h-8}M{AX} {BASE}H230" fill="none" stroke="#000" stroke-width="4"/>'
           f'<path d="M{AX-6} {BASE-h-4}L{AX} {BASE-h-16}L{AX+6} {BASE-h-4}Z" fill="#000"/>')
    mask = (f'<mask id="eng" maskUnits="userSpaceOnUse" x="0" y="0" width="256" height="256">'
            f'<rect width="256" height="256" fill="#fff"/>{cut}</mask>')
    sw = 5.5
    parts = [f'<defs>{mask}</defs>', f'<path fill="{ink}" mask="url(#eng)" d="{body}"/>',
             f'<polyline fill="none" stroke="{green}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round" points="{curve(x0, span, BASE-3, h)}"/>',
             f'<polyline fill="none" stroke="{ember}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round" points="{curve(x0+SHIFT*span, span, BASE-3, h)}"/>',
             f'<path d="M{p1x+4:.1f} {BASE-h-8}H{p2x-6:.1f}M{p2x-12:.1f} {BASE-h-13}L{p2x-4:.1f} {BASE-h-8}L{p2x-12:.1f} {BASE-h-3}" fill="none" stroke="#f3f1ec" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/>',
             sparks(p1x, TOP - 2, ember, r0=12, r1=24),
             hammer(p1x, TOP - 2, -14, ink)]
    return svg(parts, "StokesForge — gravure")

files = {
  "stokesforge-logo.svg": svg(logo(C["ink"], C["green"], C["ember"]), "StokesForge"),
  "stokesforge-logo-dark.svg": svg(logo(C["inkd"], C["greend"], C["emberd"]), "StokesForge"),
  "stokesforge-logo-mono.svg": svg(logo(C["ink"], C["green"], C["green"], mono=True), "StokesForge"),
  "stokesforge-icon.svg": icon_svg(),
  "stokesforge-engraved.svg": engraved(),
  "stokesforge-mark.svg": svg(logo(C["ink"], C["green"], C["ember"], icon=True), "StokesForge"),
  "stokesforge-mark-dark.svg": svg(logo(C["inkd"], C["greend"], C["emberd"], icon=True), "StokesForge"),
}
for n, s in files.items():
    open(os.path.join(OUT, n), "w").write(s)
print("ok", list(files))
