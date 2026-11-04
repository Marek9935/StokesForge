#!/usr/bin/env python3
"""Generate geometric StokesForge marks. Bands are computed, never hand-drawn."""
import math, os

OUT = os.path.dirname(os.path.abspath(__file__))
INK, GREEN, ORANGE, GROUND, LIGHT, HOT = "#22262a", "#3a9155", "#e0651a", "#f3f1ec", "#e6e9e7", "#ffd166"

# Absorption profile in units of sigma: main Gaussian + one shoulder Gaussian on the LEFT
# (high-energy side). Checked unimodal: monotonic left flank, single inflection plateau.
SA, SD, SW = 0.62, 1.8, 0.85
def prof(u):
    return math.exp(-u * u / 2) + SA * math.exp(-(u + SD) ** 2 / (2 * SW * SW))
_us = [i / 1000 for i in range(-7000, 5000)]
_vals = [prof(u) for u in _us]
_im = max(range(len(_vals)), key=lambda i: _vals[i])
UMAX, PMAX = _us[_im], _vals[_im]
GAP = 5.0  # Stokes shift between maxima, in sigma (crossing low at the feet)

def band(xmax, base, height, sigma, mirror=False, cut=0.004, n=160):
    """Points of a band whose maximum sits at xmax. mirror=True gives the emission."""
    pts = []
    u0, u1 = -6.5, 4.0
    for i in range(n + 1):
        u = u0 + (u1 - u0) * i / n
        p = prof(u) / PMAX
        if p < cut:
            continue
        dx = sigma * (u - UMAX)
        x = xmax - dx if mirror else xmax + dx
        pts.append((x, base - height * p))
    pts.sort()
    return pts

def fill_path(pts, base):
    s = f"M{pts[0][0]:.2f},{base:.2f} " + " ".join(f"L{x:.2f},{y:.2f}" for x, y in pts)
    return s + f" L{pts[-1][0]:.2f},{base:.2f} Z"

def line_path(pts):
    return "M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in pts)

def pair(cx, base, height, sigma):
    xa = cx - GAP * sigma / 2
    xe = cx + GAP * sigma / 2
    return xa, xe, band(xa, base, height, sigma), band(xe, base, height, sigma, mirror=True)

def star(cx, cy, r, color, k=0.2):
    pts = []
    for i in range(8):
        a = -math.pi / 2 + i * math.pi / 4
        rr = r if i % 2 == 0 else r * k
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    return f'<path d="M{" L".join(f"{x:.2f},{y:.2f}" for x, y in pts)} Z" fill="{color}"/>'

def hammer(fx, fy, angle, hw, hh, L, t, color, extra="", flip=False):
    """Hammer whose striking face centre is (fx, fy); head is vertical in local frame,
    handle goes right from the head's middle. angle in degrees (negative = handle rises)."""
    return (f'<g transform="translate({fx:.2f},{fy:.2f}) rotate({angle}){' scale(-1,1)' if flip else ''}" fill="{color}" {extra}>'
            f'<rect x="{-hw/2:.2f}" y="{-hh:.2f}" width="{hw:.2f}" height="{hh:.2f}" rx="{min(hw,hh)*0.08:.2f}"/>'
            f'<rect x="{hw/2-1:.2f}" y="{-hh/2 - t/2:.2f}" width="{L:.2f}" height="{t:.2f}" rx="{t/2:.2f}"/>'
            '</g>')

def svg(body, defs=""):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">'
            + (f"<defs>{defs}</defs>" if defs else "") + body + "</svg>")

marks = []

# ---------- 1. PRESS: the y axis bends over and becomes the hammer handle ----------
def m1():
    base, sig, h = 176, 11, 98
    xa, xe, A, E = pair(134, base, h, sig)
    top = base - h
    hw, hh = 30, 44
    head_x0 = xa - hw / 2
    # frame: y axis stem at x=34, handle at y = top - hh/2 - gap
    gap = 0  # face touches the green maximum
    face_y = top - gap
    hy = face_y - hh / 2
    t = 12
    body = []
    body.append(f'<path d="{fill_path(A, base)}" fill="{GREEN}"/>')
    body.append(f'<path d="{fill_path(E, base)}" fill="{ORANGE}" stroke="{GROUND}" stroke-width="3" paint-order="stroke"/>')
    # re-draw green on top of the overlap edge? keep orange in front with a ground cut.
    # anvil: horn + face (x axis) + waist + foot, flat geometric
    body.append(f'<path d="M10,{base} L40,{base} L40,{base+16} Q24,{base+14} 10,{base} Z" fill="{INK}"/>')
    body.append(f'<rect x="38" y="{base}" width="190" height="16" fill="{INK}"/>')
    body.append(f'<path d="M78,{base+16} L190,{base+16} L176,{base+34} L92,{base+34} Z" fill="{INK}"/>')
    body.append(f'<path d="M66,{base+44} L82,{base+34} L186,{base+34} L202,{base+44} L202,{base+54} L66,{base+54} Z" fill="{INK}"/>')
    # y axis + handle (one stroke) + head
    body.append(f'<path d="M34,{base+2} L34,{hy+t/2} Q34,{hy-t/2} 44,{hy-t/2} L{head_x0+2},{hy-t/2} L{head_x0+2},{hy+t/2} L46,{hy+t/2} L46,{base+2} Z" fill="{INK}"/>')
    body.append(f'<rect x="{head_x0}" y="{face_y-hh}" width="{hw}" height="{hh}" rx="3" fill="{INK}"/>')
    # strike marks: two short yellow ticks at the contact
    for s in (-1, 1):
        x0 = xa + s * (hw / 2 + 5)
        body.append(f'<line x1="{x0}" y1="{face_y-2}" x2="{x0+s*12}" y2="{face_y+6}" stroke="{HOT}" stroke-width="5" stroke-linecap="round"/>')
    body.append(star(xe + 22, top - 14, 15, ORANGE))
    return svg("".join(body))
marks.append(("m1-press.svg", "Press", m1()))

# ---------- 2. ANVIL OF LIGHT: the two bands ARE the anvil's waist ----------
def m2():
    slab_top, slab_bot = 98, 120
    base = 212
    sig = 11.5
    h = base - slab_bot
    xa, xe, A, E = pair(134, base, h, sig)
    body = []
    body.append(f'<path d="{fill_path(A, base)}" fill="{GREEN}"/>')
    body.append(f'<path d="{fill_path(E, base)}" fill="{ORANGE}" stroke="{GROUND}" stroke-width="3" paint-order="stroke"/>')
    left = A[0][0]
    right = E[-1][0]
    # face slab with horn (left) and heel (right)
    body.append(f'<path d="M8,{slab_top} L{left+18},{slab_top} L{right-6},{slab_top} L{right-6},{slab_bot} '
                f'L{left+30},{slab_bot} Q{left-6},{slab_bot} 8,{slab_top} Z" fill="{INK}"/>')
    # base / feet
    body.append(f'<path d="M{left-4},{base} L{right+4},{base} L{right+4},{base+10} L{right+12},{base+22} '
                f'L{left-12},{base+22} L{left-4},{base+10} Z" fill="{INK}"/>')
    # green inlay: the absorption maximum rises through the face, where the hammer lands
    body.append(f'<rect x="{xa-12}" y="{slab_top}" width="24" height="{slab_bot-slab_top+1}" fill="{GREEN}"/>')
    body.append(hammer(xa, slab_top - 1, -28, 30, 46, 104, 11, INK))
    # impact: a yellow wedge dent at the strike point
    body.append(f'<path d="M{xa-14},{slab_top-4} L{xa-26},{slab_top-16} M{xa+14},{slab_top-4} L{xa+28},{slab_top-12}" stroke="{HOT}" stroke-width="5" stroke-linecap="round"/>')
    body.append(star(xe + 26, slab_top - 22, 14, ORANGE))
    return svg("".join(body))
marks.append(("m2-anvil-of-light.svg", "Anvil of light", m2()))

# ---------- 3. SEAL: ink disc, negative-space anvil & hammer, gradient arc ----------
def m3():
    base, sig, h = 168, 9.5, 82
    xa, xe, A, E = pair(132, base, h, sig)
    top = base - h
    defs = (f'<linearGradient id="g3" x1="{xa}" y1="0" x2="{xe}" y2="0" gradientUnits="userSpaceOnUse">'
            f'<stop offset="0" stop-color="{GREEN}"/><stop offset="0.55" stop-color="{HOT}"/><stop offset="1" stop-color="{ORANGE}"/></linearGradient>'
            '<clipPath id="c3"><circle cx="128" cy="128" r="118"/></clipPath>')
    body = [f'<circle cx="128" cy="128" r="120" fill="{INK}"/>']
    g = ['<g clip-path="url(#c3)">']
    # negative-space anvil: face, horn, waist, foot (ground colour)
    g.append(f'<path d="M14,{base} L52,{base} L222,{base} L222,{base+18} L58,{base+18} Q30,{base+16} 14,{base} Z" fill="{GROUND}"/>')
    g.append(f'<path d="M86,{base+18} L196,{base+18} L182,{base+38} L100,{base+38} Z" fill="{GROUND}"/>')
    g.append(f'<rect x="60" y="{base+38}" width="160" height="40" fill="{GROUND}"/>')
    g.append('</g>')
    body.append(f'<path d="{fill_path(A, base)}" fill="{GREEN}"/>')
    body.append(f'<path d="{fill_path(E, base)}" fill="{ORANGE}" stroke="{INK}" stroke-width="3" paint-order="stroke"/>')
    body += g
    # energy arc: half circle from green max to orange max
    r = (xe - xa) / 2
    cx = (xa + xe) / 2
    mx = (xa + xe) / 2
    # hammer in ground colour, striking green from upper left
    body.append(hammer(xa, top - 1, 42, 22, 34, 54, 9, GROUND, flip=True))
    body.append(star(xe, top - 20, 14, HOT))
    return svg("".join(body), defs)
marks.append(("m3-seal.svg", "Seal", m3()))

# ---------- 4. COLUMNS: bands sampled as data columns on a stepped anvil ----------
def m4():
    base, sig, h = 172, 11.5, 112
    cx = 132
    xa, xe = cx - GAP * sig / 2, cx + GAP * sig / 2
    pitch = GAP * sig / 4
    w = pitch * 0.74
    body = []
    # sample on a grid aligned so that a column sits on each maximum
    def A(x): return prof((x - xa) / sig + UMAX) / PMAX
    def E(x): return prof(-(x - xe) / sig + UMAX) / PMAX
    k0 = -int((xa - 24) // pitch)
    cols = []
    for k in range(k0, 40):
        x = xa + k * pitch
        if x > 236: break
        a, e = A(x), E(x)
        v = max(a, e)
        if v < 0.12: continue
        cols.append((x, v, GREEN if a >= e else ORANGE))
    for x, v, c in cols:
        body.append(f'<rect x="{x-w/2:.2f}" y="{base-h*v:.2f}" width="{w}" height="{h*v:.2f}" fill="{c}"/>')
    left, right = cols[0][0] - w / 2, cols[-1][0] + w / 2
    # anvil: stepped geometric silhouette
    body.append(f'<path d="M6,{base+2} L{left},{base+2} L{right+6},{base+2} L{right+6},{base+18} L{left+26},{base+18} Q{left-4},{base+16} 6,{base+2} Z" fill="{INK}"/>')
    body.append(f'<rect x="{left+40}" y="{base+18}" width="{right-left-70}" height="18" fill="{INK}"/>')
    body.append(f'<rect x="{left+20}" y="{base+36}" width="{right-left-26}" height="14" fill="{INK}"/>')
    # hammer: the tallest green column is crowned by the hammer head
    top = base - h
    body.append(hammer(xa, top - 1, -18, 26, 40, 96, 10, INK))
    body.append(star(xe + 30, top + 4, 13, ORANGE))
    return svg("".join(body))
marks.append(("m4-columns.svg", "Columns", m4()))

# ---------- 5. TILE: app-icon tile, monoline bands, light-ink hammer ----------
def m5():
    base, sig, h = 170, 9.5, 86
    xa, xe, A, E = pair(132, base, h, sig)
    top = base - h
    defs = (f'<linearGradient id="g5" x1="0" y1="{top}" x2="0" y2="{base}" gradientUnits="userSpaceOnUse">'
            f'<stop offset="0" stop-color="{GREEN}" stop-opacity="0.55"/><stop offset="1" stop-color="{GREEN}" stop-opacity="0"/></linearGradient>'
            f'<linearGradient id="h5" x1="0" y1="{top}" x2="0" y2="{base}" gradientUnits="userSpaceOnUse">'
            f'<stop offset="0" stop-color="{ORANGE}" stop-opacity="0.55"/><stop offset="1" stop-color="{ORANGE}" stop-opacity="0"/></linearGradient>')
    body = [f'<rect x="8" y="8" width="240" height="240" rx="52" fill="{INK}"/>']
    body.append(f'<path d="{fill_path(A, base)}" fill="url(#g5)"/>')
    body.append(f'<path d="{fill_path(E, base)}" fill="url(#h5)"/>')
    body.append(f'<path d="{line_path(A)}" fill="none" stroke="{GREEN}" stroke-width="11" stroke-linecap="butt" stroke-linejoin="round"/>')
    body.append(f'<path d="{line_path(E)}" fill="none" stroke="{ORANGE}" stroke-width="11" stroke-linecap="butt" stroke-linejoin="round"/>')
    # anvil face line with horn (light ink)
    body.append(f'<path d="M30,{base+4} L214,{base+4} L214,{base+20} L74,{base+20} Q46,{base+18} 30,{base+4} Z" fill="{LIGHT}"/>')
    body.append(f'<path d="M100,{base+20} L186,{base+20} L176,{base+36} L110,{base+36} Z" fill="{LIGHT}"/>')
    body.append(f'<rect x="86" y="{base+36}" width="116" height="12" rx="2" fill="{LIGHT}"/>')
    body.append(hammer(xa, top - 8, -30, 26, 38, 88, 10, LIGHT))
    body.append(star(xe + 22, top - 8, 14, HOT))
    return svg("".join(body), defs)
marks.append(("m5-tile.svg", "Tile", m5()))

def dark(name, s):
    if name.startswith(("m3", "m5")):
        return s
    return s.replace(f'fill="{INK}"', f'fill="{LIGHT}"').replace(f'stroke="{GROUND}"', f'stroke="{INK}"')
for name, label, s in list(marks):
    with open(os.path.join(OUT, "dark-" + name), "w") as f:
        f.write(dark(name, s))
for name, label, s in marks:
    with open(os.path.join(OUT, name), "w") as f:
        f.write(s)

# ---------- board ----------
cards = "".join(
    f'<div class="card"><img src="{n}" width="256" height="256"><div class="lab">{i+1}. {l}</div></div>'
    for i, (n, l, _) in enumerate(marks))
small = "".join(
    f'<div class="sm"><span>{i+1}</span><img src="{n}" width="32" height="32"><img src="{n}" width="64" height="64">'
    f'<div class="dark"><img src="dark-{n}" width="32" height="32"><img src="dark-{n}" width="64" height="64"></div></div>'
    for i, (n, l, _) in enumerate(marks))
html = f"""<!doctype html><html><head><meta charset="utf-8"><style>
body{{margin:0;background:{GROUND};font-family:sans-serif;color:{INK};padding:24px;width:max-content}}
.row{{display:flex;gap:24px}} .card{{background:#fff;padding:12px;border-radius:8px}}
.lab{{text-align:center;font-size:18px;margin-top:6px}}
.sm{{display:flex;align-items:center;gap:14px;background:#fff;padding:10px;border-radius:8px}}
.sm span{{font-weight:bold}} .dark{{background:{INK};display:flex;gap:10px;align-items:center;padding:6px;border-radius:6px}}
</style></head><body><div class="row">{cards}</div><div class="row" style="margin-top:24px">{small}</div></body></html>"""
with open(os.path.join(OUT, "board.html"), "w") as f:
    f.write(html)
print("ok", UMAX, PMAX)
