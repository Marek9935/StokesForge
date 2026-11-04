#!/usr/bin/env python3
"""Generate the organic / warm-forge StokesForge logo marks."""
import math, os

OUT = os.path.dirname(os.path.abspath(__file__))

INK, GREEN, ORANGE = "#22262a", "#3a9155", "#e0651a"
WARM, DARK, LIGHT, HOT = "#f3f1ec", "#15181a", "#e6e9e7", "#ffd166"

# Band model (in units of the Gaussian sigma): main band at 0, one vibronic
# shoulder on the high-energy (left) side. a=0.5, d=1.5 gives one single
# maximum and no dip (checked numerically). Emission = mirror image.
A_SH, D_SH = 0.52, 2.1
def absorb(u):
    return math.exp(-u * u / 2) + A_SH * math.exp(-(u + D_SH) ** 2 / 2)

# physics guard: absorption rises monotonically up to its single maximum
_h = 0.001
assert all(absorb(-6 + (i + 1) * _h) > absorb(-6 + i * _h) for i in range(int((6 - 0.2) / _h) - 200)), "dip"
UMAX_ABS = max((i * 0.001 - 2 for i in range(4001)), key=absorb)
FMAX = absorb(UMAX_ABS)
GAP = 4.4                                   # Stokes shift between maxima
MIRROR = UMAX_ABS + GAP / 2                 # mirror axis
UMAX_EMI = 2 * MIRROR - UMAX_ABS
def emit(u):
    return absorb(2 * MIRROR - u)

U0, U1 = UMAX_ABS - 4.7, UMAX_EMI + 4.7     # visible span


class Frame:
    """Maps band units to SVG pixels."""
    def __init__(self, x0, x1, base, height):
        self.x0, self.x1, self.base, self.h = x0, x1, base, height
    def X(self, u):
        return self.x0 + (u - U0) / (U1 - U0) * (self.x1 - self.x0)
    def Y(self, v):
        return self.base - self.h * v / FMAX
    def pts(self, fn, lo=U0, hi=U1, n=160):
        return [(self.X(lo + (hi - lo) * i / n), self.Y(fn(lo + (hi - lo) * i / n))) for i in range(n + 1)]
    def line(self, fn, trim=0.0, **kw):
        if trim:
            us = [U0 + (U1 - U0) * i / 400 for i in range(401)]
            on = [u for u in us if fn(u) > trim]
            kw.setdefault("lo", on[0]); kw.setdefault("hi", on[-1])
        p = self.pts(fn, **kw)
        return "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in p)
    def area(self, fn, **kw):
        p = self.pts(fn, **kw)
        return (f"M{p[0][0]:.1f},{self.base:.1f} L" + " L".join(f"{x:.1f},{y:.1f}" for x, y in p)
                + f" L{p[-1][0]:.1f},{self.base:.1f} Z")
    def amax(self):
        return self.X(UMAX_ABS), self.Y(FMAX)
    def emax(self):
        return self.X(UMAX_EMI), self.Y(FMAX)


def hexrgb(h):
    h = h.lstrip("#"); return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
def mix(c1, c2, t):
    a, b = hexrgb(c1), hexrgb(c2)
    return "#" + "".join(f"{round(a[i] + (b[i] - a[i]) * t):02x}" for i in range(3))
def ramp(t, stops):
    """Colour along a list of stops, t in [0,1]."""
    n = len(stops) - 1
    i = min(int(t * n), n - 1)
    return mix(stops[i], stops[i + 1], t * n - i)


def quad(p0, c, p1, t):
    return tuple((1 - t) ** 2 * p0[k] + 2 * (1 - t) * t * c[k] + t * t * p1[k] for k in (0, 1))


def embers(p0, c, p1, n, r0, r1, stops, t0=0.08, t1=0.92, jitter=0.0, op=1.0):
    out = []
    for i in range(n):
        t = t0 + (t1 - t0) * i / (n - 1)
        x, y = quad(p0, c, p1, t)
        y += jitter * math.sin(i * 2.3)
        r = r0 + (r1 - r0) * math.sin(math.pi * t) ** 0.7
        out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.2f}" fill="{ramp(t, stops)}" opacity="{op}"/>')
    return "\n".join(out)


def sparkle(cx, cy, r, fill, thin=0.22):
    """Four-point star."""
    w = r * thin
    return (f'<path d="M{cx},{cy - r} Q{cx + w},{cy - w} {cx + r},{cy} Q{cx + w},{cy + w} {cx},{cy + r} '
            f'Q{cx - w},{cy + w} {cx - r},{cy} Q{cx - w},{cy - w} {cx},{cy - r} Z" fill="{fill}"/>')


def hammer(px, py, angle, head_w=18, head_l=32, handle_l=70, handle_t=10, head="#22262a",
           handle=None, face=None, scale=1.0):
    """Blacksmith's hammer whose striking face touches (px, py). The handle
    leaves the head sideways; `angle` (deg, clockwise) tilts it up-left."""
    handle = handle or head
    s = scale
    hw, hl, ht = head_w * s / 2, head_l * s, handle_t * s / 2
    hy = -hl * 0.58
    parts = [
        # handle, slightly tapered, going to -x
        f'<path d="M{-hw + 1},{hy - ht} L{-hw - handle_l * s},{hy - ht * 0.8} '
        f'Q{-hw - handle_l * s - ht * 1.6},{hy} {-hw - handle_l * s},{hy + ht * 0.8} L{-hw + 1},{hy + ht} Z" fill="{handle}"/>',
        # head: flared striking face at y=0, peen at top
        f'<path d="M{-hw * 1.18},0 L{hw * 1.18},0 L{hw},{-hl * 0.2} L{hw},{-hl * 0.86} '
        f'Q{hw * 0.6},{-hl} 0,{-hl * 1.04} Q{-hw * 0.6},{-hl} {-hw},{-hl * 0.86} L{-hw},{-hl * 0.2} Z" fill="{head}"/>',
    ]
    if face:
        parts.append(f'<rect x="{-hw * 1.18}" y="{-2.4 * s}" width="{hw * 2.36}" height="{2.4 * s}" fill="{face}"/>')
    return f'<g transform="translate({px:.1f},{py:.1f}) rotate({angle})">' + "".join(parts) + "</g>"


def svg(body, defs=""):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">'
            f"<defs>{defs}</defs>{body}</svg>\n")


# ---------------------------------------------------------------- mark 1
def mark_night_forge():
    """Dark tile, anvil with incandescent face; emission glows; ember arc."""
    F = Frame(40, 216, 160, 78)
    ax, ay = F.amax(); ex, ey = F.emax()
    defs = f"""
<linearGradient id="emi" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#fff3c4"/><stop offset="0.35" stop-color="{HOT}"/>
  <stop offset="1" stop-color="{ORANGE}"/></linearGradient>
<linearGradient id="abs" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#5fc27f"/><stop offset="1" stop-color="{GREEN}" stop-opacity="0.35"/></linearGradient>
<linearGradient id="face" gradientUnits="userSpaceOnUse" x1="30" y1="0" x2="226" y2="0">
  <stop offset="0" stop-color="{GREEN}"/><stop offset="0.45" stop-color="{HOT}"/>
  <stop offset="1" stop-color="{ORANGE}"/></linearGradient>
<linearGradient id="body" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#5a2a12"/><stop offset="0.25" stop-color="#2a2e32"/>
  <stop offset="1" stop-color="#1d2124"/></linearGradient>
<radialGradient id="halo" cx="0.5" cy="0.5" r="0.5">
  <stop offset="0" stop-color="{HOT}" stop-opacity="0.55"/><stop offset="0.5" stop-color="{ORANGE}" stop-opacity="0.22"/>
  <stop offset="1" stop-color="{ORANGE}" stop-opacity="0"/></radialGradient>
<filter id="glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="4"/></filter>
<filter id="soft" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="1.2"/></filter>
"""
    base = F.base
    anvil = (f'M30,{base} L226,{base} L226,{base + 10} Q200,{base + 12} 186,{base + 20} '
             f'L170,{base + 32} Q168,{base + 44} 196,{base + 52} L196,{base + 62} L76,{base + 62} '
             f'L76,{base + 52} Q104,{base + 44} 102,{base + 32} L86,{base + 20} '
             f'Q60,{base + 14} 30,{base + 6} Z')
    body = f"""
<rect width="256" height="256" rx="52" fill="{DARK}"/>
<ellipse cx="{ex:.1f}" cy="{ey + 30:.1f}" rx="78" ry="70" fill="url(#halo)"/>
<path d="{anvil}" fill="url(#body)"/>
<path d="M30,{base + 1} L226,{base + 1}" stroke="url(#face)" stroke-width="6" filter="url(#glow)" opacity="0.9"/>
<path d="{F.area(emit)}" fill="url(#emi)" filter="url(#glow)" opacity="0.7"/>
<path d="{F.area(absorb)}" fill="url(#abs)"/>
<path d="{F.line(absorb, trim=0.02)}" fill="none" stroke="#6fd08e" stroke-width="4.5" stroke-linejoin="round"/>
<path d="{F.area(emit)}" fill="url(#emi)"/>
<path d="M30,{base + 1} L226,{base + 1}" stroke="url(#face)" stroke-width="4" stroke-linecap="round"/>
{embers((ax + 12, ay + 2), ((ax + ex) / 2, ay - 36), (ex, ey - 8), 9, 1.5, 3.4, [GREEN, '#9ad36a', HOT, ORANGE])}
{sparkle(ex, ey - 16, 13, '#fff3c4')}
<circle cx="{ax:.1f}" cy="{ay:.1f}" r="9" fill="{HOT}" opacity="0.55" filter="url(#soft)"/>
{hammer(ax, ay - 2, 24, head=LIGHT, handle='#9a6b45', face=HOT, scale=1.0)}
"""
    return svg(body, defs)


# ---------------------------------------------------------------- mark 2
def mark_ember_arc():
    """Warm ground, no anvil: hammer strike throws an ember arc that lands
    as the glowing emission, under a sparkle."""
    F = Frame(30, 226, 196, 92)
    ax, ay = F.amax(); ex, ey = F.emax()
    defs = f"""
<linearGradient id="emi" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="{HOT}"/><stop offset="0.55" stop-color="{ORANGE}"/>
  <stop offset="1" stop-color="#b8430c"/></linearGradient>
<linearGradient id="abs" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="{GREEN}"/><stop offset="1" stop-color="#2b6e40"/></linearGradient>
<linearGradient id="bar" gradientUnits="userSpaceOnUse" x1="22" y1="0" x2="234" y2="0">
  <stop offset="0" stop-color="{INK}"/><stop offset="0.45" stop-color="{INK}"/>
  <stop offset="0.75" stop-color="{ORANGE}"/><stop offset="1" stop-color="{INK}"/></linearGradient>
<radialGradient id="halo" cx="0.5" cy="0.5" r="0.5">
  <stop offset="0" stop-color="{HOT}" stop-opacity="0.75"/><stop offset="0.55" stop-color="{ORANGE}" stop-opacity="0.18"/>
  <stop offset="1" stop-color="{ORANGE}" stop-opacity="0"/></radialGradient>
<filter id="glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="3"/></filter>
"""
    # spray of sparks from the impact (short strokes radiating)
    spray = []
    for k, (ang, ln) in enumerate([(-72, 14), (-48, 12), (-24, 10), (0, 8)]):
        a = math.radians(ang)
        x0, y0 = ax + 9 * math.cos(a), ay - 4 + 9 * math.sin(a)
        x1, y1 = ax + (9 + ln) * math.cos(a), ay - 4 + (9 + ln) * math.sin(a)
        spray.append(f'<path d="M{x0:.1f},{y0:.1f} L{x1:.1f},{y1:.1f}" stroke="{[GREEN, HOT, ORANGE, HOT][k]}" '
                     f'stroke-width="3" stroke-linecap="round"/>')
    body = f"""
<rect width="256" height="256" rx="52" fill="{WARM}"/>
<ellipse cx="{ex:.1f}" cy="{ey + 22:.1f}" rx="70" ry="66" fill="url(#halo)"/>
<path d="{F.area(emit)}" fill="url(#emi)" filter="url(#glow)" opacity="0.6"/>
<path d="{F.area(absorb)}" fill="url(#abs)"/>
<path d="{F.area(emit)}" fill="url(#emi)"/>
<path d="M22,{F.base + 3} L234,{F.base + 3}" stroke="url(#bar)" stroke-width="8" stroke-linecap="round"/>
{embers((ax + 14, ay - 4), ((ax + ex) / 2 + 4, ay - 58), (ex, ey - 22), 10, 1.6, 3.8, [GREEN, '#a7c95a', HOT, ORANGE])}
{''.join(spray)}
{sparkle(ex, ey - 22, 15, HOT)}
{sparkle(ex, ey - 22, 6, '#fffaf0')}
{hammer(ax, ay - 1, 18, head=INK, scale=1.05)}
"""
    return svg(body, defs)


# ---------------------------------------------------------------- mark 3
def mark_hearth():
    """Round medallion: the bands rise like flames from a forge fire; the
    emission is metal glowing white-hot at its crest."""
    F = Frame(40, 216, 168, 84)
    ax, ay = F.amax(); ex, ey = F.emax()
    defs = f"""
<clipPath id="disc"><circle cx="128" cy="128" r="120"/></clipPath>
<radialGradient id="bg" cx="0.5" cy="0.85" r="0.85">
  <stop offset="0" stop-color="#3a1d0e"/><stop offset="0.55" stop-color="#1b1d1f"/>
  <stop offset="1" stop-color="{DARK}"/></radialGradient>
<linearGradient id="emi" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#fff6d8"/><stop offset="0.3" stop-color="{HOT}"/>
  <stop offset="0.75" stop-color="{ORANGE}"/><stop offset="1" stop-color="#a8360a"/></linearGradient>
<linearGradient id="abs" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#7bd89a"/><stop offset="0.6" stop-color="{GREEN}"/>
  <stop offset="1" stop-color="#1f5a32"/></linearGradient>
<linearGradient id="fire" x1="0" y1="1" x2="0" y2="0">
  <stop offset="0" stop-color="{ORANGE}"/><stop offset="0.7" stop-color="{HOT}"/>
  <stop offset="1" stop-color="{HOT}" stop-opacity="0"/></linearGradient>
<filter id="glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="5"/></filter>
"""
    # fire tongues below the grate
    tongues = []
    for i, (cx, h, w) in enumerate([(58, 26, 14), (84, 34, 16), (112, 28, 14), (140, 38, 18), (168, 30, 15), (196, 24, 13)]):
        b = F.base + 52
        tongues.append(f'<path d="M{cx - w},{b} Q{cx - w * 0.6},{b - h * 0.6} {cx + w * 0.15},{b - h} '
                       f'Q{cx + w * 0.25},{b - h * 0.45} {cx + w},{b} Z" fill="url(#fire)" opacity="0.9"/>')
    grate = "".join(f'<rect x="{x}" y="{F.base + 2}" width="14" height="5" rx="2" fill="#3b4045"/>' for x in range(46, 214, 20))
    body = f"""
<circle cx="128" cy="128" r="124" fill="{DARK}"/>
<g clip-path="url(#disc)">
<rect width="256" height="256" fill="url(#bg)"/>
{''.join(tongues)}
<ellipse cx="128" cy="{F.base + 50}" rx="110" ry="22" fill="{ORANGE}" opacity="0.35" filter="url(#glow)"/>
</g>
<path d="{F.area(emit)}" fill="{HOT}" filter="url(#glow)" opacity="0.65"/>
<path d="{F.area(absorb)}" fill="url(#abs)"/>
<path d="{F.area(emit)}" fill="url(#emi)"/>
<rect x="36" y="{F.base - 1}" width="184" height="5" rx="2.5" fill="{LIGHT}"/>
{grate}
{embers((ax + 14, ay + 4), ((ax + ex) / 2, ay - 32), (ex - 4, ey - 4), 8, 1.6, 3.0, ['#7bd89a', HOT, '#fff6d8'])}
{embers((ex + 20, ey + 10), (ex + 30, ey - 20), (ex + 18, ey - 44), 4, 1.0, 2.0, [HOT, ORANGE], t0=0.1, t1=0.9)}
{sparkle(ex, ey - 14, 11, '#fff6d8')}
{hammer(ax, ay - 1, 34, head=LIGHT, handle='#c48a52', face=HOT, scale=0.95)}
<circle cx="128" cy="128" r="122" fill="none" stroke="#3a2a1e" stroke-width="4"/>
"""
    return svg(body, defs)


# ---------------------------------------------------------------- mark 4
def mark_heat_bloom():
    """Warm ground, ink anvil: the emission rises as a glowing sun with
    light rays; an ember dimension line marks the Stokes shift."""
    F = Frame(34, 222, 152, 80)
    ax, ay = F.amax(); ex, ey = F.emax()
    defs = f"""
<radialGradient id="sun" cx="0.5" cy="0.5" r="0.5">
  <stop offset="0" stop-color="{HOT}" stop-opacity="0.9"/><stop offset="0.6" stop-color="{ORANGE}" stop-opacity="0.25"/>
  <stop offset="1" stop-color="{ORANGE}" stop-opacity="0"/></radialGradient>
<linearGradient id="emi" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="{HOT}"/><stop offset="0.5" stop-color="{ORANGE}"/>
  <stop offset="1" stop-color="{ORANGE}"/></linearGradient>
<linearGradient id="ray" x1="0" y1="1" x2="0" y2="0">
  <stop offset="0" stop-color="{HOT}" stop-opacity="0.9"/><stop offset="1" stop-color="{HOT}" stop-opacity="0"/></linearGradient>
<linearGradient id="edge" gradientUnits="userSpaceOnUse" x1="22" y1="0" x2="232" y2="0">
  <stop offset="0" stop-color="{INK}"/><stop offset="0.5" stop-color="#7a3a14"/>
  <stop offset="0.72" stop-color="{ORANGE}"/><stop offset="1" stop-color="#7a3a14"/></linearGradient>
"""
    rays = []
    for ang in range(-80, 81, 20):
        a = math.radians(ang)
        r0, r1 = 30, 60
        w = 0.07
        p = lambda r, da: (ex + r * math.sin(a + da), ey - 6 - r * math.cos(a + da))
        (x0, y0), (x1, y1), (x2, y2) = p(r0, 0), p(r1, w), p(r1, -w)
        rays.append(f'<path d="M{x0:.1f},{y0:.1f} L{x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f} Z" fill="{HOT}" opacity="0.75"/>')
    base = F.base
    anvil = (f'M22,{base} L232,{base} L232,{base + 16} Q206,{base + 18} 192,{base + 28} '
             f'L176,{base + 40} Q174,{base + 52} 200,{base + 60} L200,{base + 72} L68,{base + 72} '
             f'L68,{base + 60} Q94,{base + 52} 92,{base + 40} L76,{base + 30} '
             f'Q54,{base + 20} 22,{base + 10} Z')
    # dimension line of embers between the two maxima, under the peaks
    dy = ay - 14
    dim = []
    n = 8
    for i in range(n):
        t = i / (n - 1)
        x = ax + 10 + (ex - ax - 20) * t
        dim.append(f'<circle cx="{x:.1f}" cy="{dy:.1f}" r="{1.6 + 1.6 * t:.2f}" fill="{ramp(t, [GREEN, HOT, ORANGE])}"/>')
    body = f"""
<clipPath id="tile"><rect width="256" height="256" rx="52"/></clipPath>
<rect width="256" height="256" rx="52" fill="{WARM}"/>
<g clip-path="url(#tile)">{''.join(rays)}
<circle cx="{ex:.1f}" cy="{ey + 6:.1f}" r="48" fill="url(#sun)"/>
<path d="{anvil}" fill="{INK}"/></g>
<path d="M22,{base + 2} L232,{base + 2}" stroke="url(#edge)" stroke-width="5" stroke-linecap="round"/>
<path d="{F.area(absorb)}" fill="{GREEN}"/>
<path d="{F.area(emit)}" fill="url(#emi)"/>
{''.join(dim)}
{hammer(ax, ay - 1, 16, head=INK, scale=0.92)}
"""
    return svg(body, defs)


# ---------------------------------------------------------------- mark 5
def mark_neon():
    """App-icon simplification for small sizes: glowing strokes on a dark
    tile, a hammer head on the green crest, a spark on the orange crest."""
    F = Frame(26, 230, 190, 104)
    ax, ay = F.amax(); ex, ey = F.emax()
    defs = f"""
<filter id="g" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="5"/></filter>
<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#1c2023"/><stop offset="1" stop-color="#2a1a12"/></linearGradient>
<linearGradient id="eo" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="{HOT}"/><stop offset="1" stop-color="{ORANGE}"/></linearGradient>
"""
    gl, el = F.line(absorb, trim=0.012), F.line(emit, trim=0.012)
    body = f"""
<rect width="256" height="256" rx="56" fill="url(#tile)"/>
<path d="M36,{F.base + 4} L220,{F.base + 4}" stroke="{LIGHT}" stroke-opacity="0.85" stroke-width="7" stroke-linecap="round"/>
<path d="{gl}" fill="none" stroke="{GREEN}" stroke-width="16" filter="url(#g)" opacity="0.9"/>
<path d="{el}" fill="none" stroke="{ORANGE}" stroke-width="18" filter="url(#g)"/>
<path d="{gl}" fill="none" stroke="#5fc47f" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>
<path d="{el}" fill="none" stroke="url(#eo)" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>
<circle cx="{ex:.1f}" cy="{ey - 24:.1f}" r="14" fill="{HOT}" filter="url(#g)"/>
{sparkle(ex, ey - 24, 17, '#fff3c4')}
{hammer(ax, ay - 6, 28, head=LIGHT, handle='#b98552', scale=1.25, handle_l=58)}
"""
    return svg(body, defs)


MARKS = [
    ("1-night-forge", "Night forge", mark_night_forge),
    ("2-ember-arc", "Ember arc", mark_ember_arc),
    ("3-hearth", "Hearth medallion", mark_hearth),
    ("4-heat-bloom", "Heat bloom", mark_heat_bloom),
    ("5-neon-tile", "Neon tile", mark_neon),
]

if __name__ == "__main__":
    for name, label, fn in MARKS:
        with open(os.path.join(OUT, name + ".svg"), "w") as fh:
            fh.write(fn())
    cards = "".join(
        f'<div class="c"><img src="{n}.svg" width="256" height="256"><p>{i + 1}. {l}</p></div>'
        for i, (n, l, _) in enumerate(MARKS))
    small = "".join(
        f'<div class="s"><img src="{n}.svg" width="64" height="64"><img src="{n}.svg" width="32" height="32">'
        f'<img src="{n}.svg" width="32" height="32" class="dk"><span>{i + 1}</span></div>'
        for i, (n, _, _) in enumerate(MARKS))
    html = f"""<!doctype html><meta charset="utf-8"><style>
body{{margin:0;padding:24px;background:#d9d6cf;font:15px sans-serif;color:#22262a;width:1452px}}
.row{{display:flex;gap:24px}} .c p{{text-align:center;margin:8px 0 0}}
.s{{display:flex;align-items:center;gap:14px;width:260px;margin-top:28px}}
.dk{{background:#15181a;padding:6px;border-radius:6px}}
</style><div class="row">{cards}</div><div class="row">{small}</div>"""
    with open(os.path.join(OUT, "board.html"), "w") as fh:
        fh.write(html)
    print("max abs u", UMAX_ABS, "max emi u", UMAX_EMI)
