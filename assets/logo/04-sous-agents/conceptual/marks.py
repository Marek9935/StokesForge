"""Generate the conceptual StokesForge marks as standalone SVGs."""
import math
import os

import numpy as np

import bands
from bands import Pair, path

OUT = os.path.dirname(os.path.abspath(__file__))
INK, GREEN, ORANGE = "#22262a", "#3a9155", "#e0651a"
GROUND, DARK, LIGHT, HOT = "#f3f1ec", "#15181a", "#e6e9e7", "#ffd166"

bands.check()


def svg(body, defs=""):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">'
        + (f"<defs>{defs}</defs>" if defs else "")
        + body
        + "</svg>\n"
    )


def hammer(x, y, angle, head=(22, 40), handle=(58, 9), color=INK, flip=False, handle_path=None):
    """Hammer whose striking face is centred on (x, y), pointing down at `angle` deg."""
    hw, hl = head
    L, tw = handle
    s = "scale(-1,1)" if flip else ""
    if handle_path is None:
        hd = f'<rect x="{hw/2-3}" y="{-hl*0.62 - tw/2:.1f}" width="{L}" height="{tw}" rx="{tw/2}" fill="{color}"/>'
    else:
        hd = handle_path
    return (
        f'<g transform="translate({x:.1f},{y:.1f}) rotate({angle}) {s}">'
        + hd
        + f'<path d="M{-hw/2},{-hl} h{hw} v{hl-4} q0,4 -4,4 h{-(hw-8)} q-4,0 -4,-4 Z" fill="{color}"/>'
        + f'<rect x="{-hw/2-2}" y="{-hl}" width="{hw+4}" height="7" rx="2" fill="{color}"/>'
        + "</g>"
    )


def sparkle(cx, cy, r, color, k=0.22):
    pts = []
    for i in range(8):
        a = math.pi / 4 * i - math.pi / 2
        rr = r if i % 2 == 0 else r * k
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    d = "M" + " ".join(f"{px:.1f},{py:.1f}" for px, py in pts) + "Z"
    return f'<path d="{d}" fill="{color}"/>'


def flecks(x, y, color, r0=8, r1=15, angles=(-150, -120, -60, -30), w=3.2):
    out = []
    for a in angles:
        t = math.radians(a)
        out.append(
            f'<line x1="{x + r0*math.cos(t):.1f}" y1="{y + r0*math.sin(t):.1f}" '
            f'x2="{x + r1*math.cos(t):.1f}" y2="{y + r1*math.sin(t):.1f}" '
            f'stroke="{color}" stroke-width="{w}" stroke-linecap="round"/>'
        )
    return "".join(out)


# ---------------------------------------------------------------- mark 1
def mark_stokes_anvil():
    """The anvil IS the Stokes shift: its face spans maximum to maximum,
    its waist is cut out of the valley between the bands."""
    sig = 15.0
    gap = 4.1 * sig
    mid = 128
    p = Pair(mid - gap / 2, mid + gap / 2, 172, 100, sig)
    py = p.peak_y()
    g = 4.0  # clearance between band and anvil
    waist = 24
    xs = np.linspace(p.xa, mid, 400)
    left = []
    for x in xs:
        xl = x + g
        if 2 * (mid - xl) <= waist:
            break
        left.append((xl, p.abs_y(x)))
    yw = left[-1][1]
    foot_y0, foot_y1 = p.base + 14, p.base + 46
    fw = 60
    pts = [(p.xa + g, py)] + left[1:]
    pts += [(mid - waist / 2, yw + 6), (mid - waist / 2, foot_y0)]
    body = "M" + " L".join(f"{a:.1f},{b:.1f}" for a, b in pts)
    body += (
        f" C{mid-waist/2},{foot_y0+14} {mid-fw+8},{foot_y0+14} {mid-fw},{foot_y1}"
        f" L{mid-16},{foot_y1} Q{mid},{foot_y1-11} {mid+16},{foot_y1}"
        f" L{mid+fw},{foot_y1} C{mid+fw-8},{foot_y0+14} {mid+waist/2},{foot_y0+14} {mid+waist/2},{foot_y0}"
    )
    right = [(2 * mid - a, b) for a, b in reversed(pts)]
    body += " L" + " L".join(f"{a:.1f},{b:.1f}" for a, b in right) + " Z"

    defs = (
        f'<linearGradient id="hot1" x1="{p.xa}" x2="{p.xe}" y1="0" y2="0" gradientUnits="userSpaceOnUse">'
        f'<stop offset="0.08" stop-color="{GREEN}"/><stop offset="0.5" stop-color="{HOT}"/>'
        f'<stop offset="0.92" stop-color="{ORANGE}"/></linearGradient>'
    )
    out = f'<rect width="256" height="256" rx="40" fill="{GROUND}"/>'
    out += f'<line x1="14" y1="{p.base+2.5}" x2="242" y2="{p.base+2.5}" stroke="{INK}" stroke-width="5" stroke-linecap="round"/>'
    out += f'<path d="{path(p.abs_pts(n=200), p.base)}" fill="{GREEN}"/>'
    out += f'<path d="{path(p.em_pts(n=200), p.base)}" fill="{ORANGE}"/>'
    out += f'<path d="{body}" fill="{INK}" stroke="{GROUND}" stroke-width="3.5" stroke-linejoin="round"/>'
    # hot face = the Stokes shift, glowing from green to orange
    out += f'<rect x="{p.xa+g}" y="{py}" width="{p.xe-p.xa-2*g}" height="9" fill="url(#hot1)"/>'
    out += hammer(p.xa + 1, py - 2, 24, head=(22, 38), handle=(56, 10), flip=True)
    out += flecks(p.xa, py, INK, r0=17, r1=25, angles=(200,), w=4)
    out += sparkle(p.xe + 24, py - 20, 15, ORANGE)
    return svg(out, defs)


# ---------------------------------------------------------------- mark 2
def mark_jablonski_anvil():
    """The anvil silhouette is a Jablonski diagram: face = S1, feet = S0,
    the waist is the two transitions, the heat of relaxation stays in the anvil."""
    out = f'<rect width="256" height="256" rx="52" fill="{DARK}"/>'
    fy0, fy1 = 88, 112          # S1 block; its bottom edge is S1 v=0
    ty0, ty1 = 198, 218         # S0 block; its top edge is S0 v=0
    vib = 9                     # vibrational spacing (same in both states)
    face = (
        f"M62,{fy0} L216,{fy0} L216,{fy1-8} Q216,{fy1} 208,{fy1}"
        f" L80,{fy1} Q66,{fy1} 56,{fy1-6} Q40,{fy1-14} 20,{fy0+2} Q38,{fy0} 62,{fy0} Z"
    )
    out += f'<path d="{face}" fill="{LIGHT}"/>'
    for k in (1, 2):  # S1 v1, v2 as grooves in the face
        out += f'<rect x="84" y="{fy1-k*vib-1.25}" width="120" height="2.5" fill="{DARK}"/>'
    feet = (
        f"M84,{ty0} L172,{ty0} Q178,{ty0} 182,{ty0+5} L198,{ty1} L146,{ty1}"
        f" Q128,{ty1-11} 110,{ty1} L58,{ty1} L74,{ty0+5} Q78,{ty0} 84,{ty0} Z"
    )
    out += f'<path d="{feet}" fill="{LIGHT}"/>'
    for k in (1, 2):  # S0 v1, v2 as plates above the feet
        out += f'<rect x="90" y="{ty0-k*vib-1.5}" width="76" height="3" rx="1.5" fill="{LIGHT}"/>'
    gx, ox, sw = 106, 152, 16
    # absorption S0 v0 -> S1 v1 (green, up)
    top = fy1 - vib
    out += f'<rect x="{gx-sw/2}" y="{top+16}" width="{sw}" height="{ty0-top-16}" fill="{GREEN}"/>'
    out += f'<path d="M{gx-17},{top+19} L{gx},{top} L{gx+17},{top+19} Z" fill="{GREEN}"/>'
    # emission S1 v0 -> S0 v1 (orange, down, shorter)
    bot = ty0 - vib
    out += f'<rect x="{ox-sw/2}" y="{fy1}" width="{sw}" height="{bot-fy1-17}" fill="{ORANGE}"/>'
    out += f'<path d="M{ox-17},{bot-19} L{ox},{bot} L{ox+17},{bot-19} Z" fill="{ORANGE}"/>'
    # vibrational relaxation: wavy, glowing, inside the anvil
    xs = np.linspace(gx + 6, ox - 4, 80)
    ys = top - 3 + (vib + 1) * (xs - xs[0]) / (xs[-1] - xs[0]) + 3.0 * np.sin((xs - xs[0]) / 3.0)
    wav = "M" + " L".join(f"{a:.1f},{b:.1f}" for a, b in zip(xs, ys))
    defs = (
        f'<linearGradient id="hot2" x1="{gx}" x2="{ox}" y1="0" y2="0" gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="{GREEN}"/><stop offset="0.5" stop-color="{HOT}"/>'
        f'<stop offset="1" stop-color="{ORANGE}"/></linearGradient>'
    )
    out += f'<path d="{wav}" fill="none" stroke="url(#hot2)" stroke-width="3.6" stroke-linecap="round"/>'
    out += hammer(gx, fy0 - 3, 16, head=(22, 38), handle=(56, 9), color=LIGHT, flip=True)
    out += flecks(gx, fy0 - 3, HOT, r0=16, r1=24, angles=(198, -18), w=3.6)
    return svg(out, defs)


# ---------------------------------------------------------------- mark 3
def wave_path(x0, y0, x1, y1, period, amp, w, color, head=True, taper=8):
    L = math.hypot(x1 - x0, y1 - y0)
    ang = math.degrees(math.atan2(y1 - y0, x1 - x0))
    n = int(L * 3)
    t = np.linspace(0, L - (12 if head else 0), n)
    env = np.clip(np.minimum(t / taper, 1), 0, 1)
    yy = amp * np.sin(2 * math.pi * t / period) * env
    d = "M" + " L".join(f"{a:.2f},{b:.2f}" for a, b in zip(t, yy))
    s = f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"/>'
    if head:
        s += f'<path d="M{L-16},-10 L{L+2},0 L{L-16},10 Z" fill="{color}"/>'
    return f'<g transform="translate({x0:.1f},{y0:.1f}) rotate({ang:.2f})">{s}</g>'


def mark_photon_hammer():
    """The hammer's handle is the incoming photon (short wavelength); the
    photon that leaves the emission maximum is visibly longer."""
    sig = 14.5
    gap = 4.0 * sig
    mid = 128
    p = Pair(mid - gap / 2, mid + gap / 2, 200, 94, sig)
    py = p.peak_y()
    out = f'<rect width="256" height="256" rx="40" fill="{GROUND}"/>'
    out += f'<path d="{path(p.abs_pts(n=200), p.base)}" fill="{GREEN}"/>'
    out += f'<path d="{path(p.em_pts(n=200), p.base)}" fill="{ORANGE}"/>'
    out += f'<line x1="16" y1="{p.base}" x2="240" y2="{p.base}" stroke="{INK}" stroke-width="5" stroke-linecap="round"/>'
    hw, hl = 26, 44
    handle = wave_path(hw / 2 - 4, -hl * 0.6, 82, -hl * 0.6, 13.0, 5.0, 6, GREEN, head=False, taper=1)
    out += hammer(p.xa, py - 3, 28, head=(hw, hl), handle_path=handle, flip=True)
    out += flecks(p.xa, py - 2, INK, r0=17, r1=25, angles=(-12,), w=4)
    out += wave_path(p.xe + 10, py - 12, 236, 22, 20.0, 6.5, 6, ORANGE, taper=6)
    return svg(out)


# ---------------------------------------------------------------- mark 4
def mark_origin_hammer():
    """The plot's own y axis, pivoting at the origin, swings down as the
    hammer onto the absorption maximum: its arrowhead became the head."""
    sig = 14.5
    gap = 4.0 * sig
    p = Pair(118, 118 + gap, 206, 104, sig)
    py = p.peak_y()
    ox, oy = 28, p.base
    hl, hw = 46, 26
    tx, ty = p.xa, py - 1
    # handle direction such that the head face lands on the peak
    L = math.hypot(tx - ox, ty - oy)
    ang = math.degrees(math.atan2(ty - oy, tx - ox))
    out = f'<rect width="256" height="256" rx="40" fill="{GROUND}"/>'
    defs = (
        f'<linearGradient id="rise4" x1="{p.xe-2.6*sig}" x2="{p.xe}" y1="0" y2="0" gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="{GREEN}"/><stop offset="1" stop-color="{ORANGE}"/></linearGradient>'
    )
    out += f'<path d="{path(p.abs_pts(n=200), p.base)}" fill="{GREEN}"/>'
    out += f'<path d="{path(p.em_pts(n=200), p.base)}" fill="url(#rise4)"/>'
    # ghost of the upright y axis, with its arrowhead, and the swing
    gtop = oy - L + 26
    out += f'<line x1="{ox}" y1="{oy}" x2="{ox}" y2="{gtop+8}" stroke="{INK}" stroke-opacity="0.16" stroke-width="7" stroke-linecap="round"/>'
    out += f'<path d="M{ox-9},{gtop+12} L{ox},{gtop-4} L{ox+9},{gtop+12} Z" fill="{INK}" fill-opacity="0.16"/>'
    arc = []
    for a in np.linspace(-90 + 8, ang - 10, 40):
        t = math.radians(a)
        arc.append((ox + (L - 8) * math.cos(t), oy + (L - 8) * math.sin(t)))
    out += f'<path d="{"M" + " L".join(f"{a:.1f},{b:.1f}" for a, b in arc)}" fill="none" stroke="{INK}" stroke-opacity="0.4" stroke-width="3.5" stroke-dasharray="0.1 8" stroke-linecap="round"/>'
    # x axis
    out += f'<path d="M{ox},{oy} L236,{oy}" stroke="{INK}" stroke-width="7" stroke-linecap="round"/>'
    out += f'<path d="M230,{oy-9} L247,{oy} L230,{oy+9} Z" fill="{INK}"/>'
    # fallen y axis = handle, head perpendicular with its face on the peak
    reach = L - hw / 2
    out += (
        f'<g transform="translate({ox},{oy}) rotate({ang:.2f})">'
        f'<rect x="-3.5" y="-3.5" width="{reach:.1f}" height="7" rx="3.5" fill="{INK}"/>'
        f'<rect x="{reach - hw/2:.1f}" y="{-hl*0.5}" width="{hw}" height="{hl}" rx="4" fill="{INK}"/>'
        f'<rect x="{reach - hw/2 - 3:.1f}" y="{-hl*0.5}" width="7" height="{hl}" rx="2" fill="{INK}"/>'
        "</g>"
    )
    out += f'<circle cx="{ox}" cy="{oy}" r="7.5" fill="{INK}"/>'
    out += flecks(tx, ty, INK, r0=24, r1=32, angles=(-6, 186), w=4)
    out += sparkle(p.xe + 4, py - 24, 16, ORANGE)
    return svg(out, defs)


# ---------------------------------------------------------------- mark 5
def mark_spark_counts():
    """Every blow throws sparks; the sparks pile up as a photon-count
    histogram that draws the emission, mirror image of the struck band."""
    sig = 14.0
    gap = 4.0 * sig
    mid = 130
    p = Pair(mid - gap / 2, mid + gap / 2, 204, 104, sig)
    py = p.peak_y()
    out = f'<rect width="256" height="256" rx="40" fill="{GROUND}"/>'
    out += f'<path d="{path(p.abs_pts(n=200), p.base)}" fill="{GREEN}"/>'
    out += f'<path d="{path(p.em_pts(n=200), p.base)}" fill="#f4c19c"/>'
    # emission as columns of sparks (counts), heights sampled on the mirror profile
    step, r = 8.4, 3.9
    xs = np.arange(p.xe - 3.0 * sig, p.xe + 4.6 * sig, step)
    # align a column on the maximum
    xs = xs - (xs[np.argmin(abs(xs - p.xe))] - p.xe)
    for x in xs:
        hgt = p.base - p.em_y(x)
        n = int(round(hgt / step))
        for k in range(n):
            cy = p.base - step / 2 - k * step
            col = HOT if k == n - 1 and n >= 3 else ORANGE
            out += f'<circle cx="{x:.1f}" cy="{cy:.1f}" r="{r}" fill="{col}"/>'
    out += f'<line x1="16" y1="{p.base+3}" x2="240" y2="{p.base+3}" stroke="{INK}" stroke-width="5" stroke-linecap="round"/>'
    # hammer strikes the absorption; sparks leave toward the emission
    out += hammer(p.xa + 1, py - 2, 22, head=(24, 42), handle=(58, 10), flip=True)
    x0, x1 = p.xa + 20, p.xe + 2
    for i, f in enumerate(np.linspace(0.0, 1.0, 6)[:-1]):
        x = x0 + (x1 - x0) * f
        y = py - 18 - 46 * 4 * f * (1 - f) + 8 * f
        rr = 3.6 - 1.0 * f
        out += f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{rr:.1f}" fill="{HOT}" stroke="{ORANGE}" stroke-width="1.3"/>'
    return svg(out)


MARKS = [
    ("1-stokes-anvil", "Anvil = Stokes shift", mark_stokes_anvil),
    ("2-jablonski-anvil", "Jablonski anvil", mark_jablonski_anvil),
    ("3-photon-hammer", "Photon hammer", mark_photon_hammer),
    ("4-origin-hammer", "Hammer from the Origin", mark_origin_hammer),
    ("5-spark-counts", "Sparks count the emission", mark_spark_counts),
]

if __name__ == "__main__":
    for name, _, fn in MARKS:
        with open(os.path.join(OUT, name + ".svg"), "w") as f:
            f.write(fn())
    print("ok")
    import json
    with open(os.path.join(OUT, "marks.json"), "w") as f:
        json.dump([{"file": n + ".svg", "label": l} for n, l, _ in MARKS], f)
