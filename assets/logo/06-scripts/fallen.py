import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
import make2 as M   # also regenerates make2's files, unchanged
OUT = sys.argv[1]
ink, green, ember = M.L
TOP, AX, H = M.TOP, M.AX, M.H
sw, w = 10, 6
peak_y = TOP - H
base, hh = TOP - sw / 2, H - sw / 2

def pts(s):
    return [tuple(map(float, p.split(','))) for p in s.split()]
ab_s = M.spectrum(M.MU_A, -1, base, hh, AX + 6, M.MU_A + 30)
em_s = M.spectrum(M.MU_E, +1, base, hh, M.MU_E - 30, 232)
EM = pts(em_s)

HW, HT, LH, HAND = 48, 22, 88, w + 2      # head width, head thickness, handle length, handle width
def hammer_outline(step=1.0):
    """Sample points of the hammer, pivot at (0,0), handle along -y, head on top."""
    out = []
    def rect(x0, y0, x1, y1):
        x = x0
        while x <= x1: out.extend([(x, y0), (x, y1)]); x += step
        y = y0
        while y <= y1: out.extend([(x0, y), (x1, y)]); y += step
    rect(-HAND / 2, -LH, HAND / 2, 0)
    rect(-HW / 2, -LH - HT, HW / 2, -LH)
    rect(-HW / 2 - 4, -LH - HT + 2, -HW / 2 + 3, -LH - 2)
    return out
HO = hammer_outline()
def rot(p, th, piv):
    c, s = math.cos(th), math.sin(th)
    return (piv[0] + p[0] * c - p[1] * s, piv[1] + p[0] * s + p[1] * c)
def clearance(th, piv, curve):
    return min(math.dist(rot(p, th, piv), q) for p in HO[::2] for q in curve) - sw / 2
def contact(piv, curve, direction, lo=0.0, hi=math.radians(85)):
    # first angle (from upright) at which the hammer touches `curve`
    prev = lo; a = lo
    while a <= hi:
        if clearance(direction * a, piv, curve) <= 0.6:
            return direction * a
        a += math.radians(0.25)
    raise SystemExit("no contact")

def hammer_svg(th, piv):
    deg = math.degrees(th)
    return (f'<g fill="{ink}" transform="translate({piv[0]} {piv[1]}) rotate({deg:.2f})">'
            f'<rect x="{-HAND/2}" y="{-LH}" width="{HAND}" height="{LH}"/>'
            f'<rect x="{-HW/2}" y="{-LH-HT}" width="{HW}" height="{HT}" rx="3"/>'
            f'<rect x="{-HW/2-4}" y="{-LH-HT+2}" width="7" height="{HT-4}" rx="2"/></g>')
line = f'fill="none" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"'
anvil = f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>'
arrow = (f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
         f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')

def hammer_halo(th, piv, bg):
    deg = math.degrees(th)
    return (f'<g fill="{bg}" stroke="{bg}" stroke-width="8" stroke-linejoin="round" transform="translate({piv[0]} {piv[1]}) rotate({deg:.2f})">'
            f'<rect x="{-HAND/2}" y="{-LH}" width="{HAND}" height="{LH-8}"/>'
            f'<rect x="{-HW/2}" y="{-LH-HT}" width="{HW}" height="{HT}" rx="3"/></g>')

def variant_a(bg):
    # the ordinate-handle falls over the absorption band and comes to rest on the emission band
    piv = (AX, TOP)
    th = contact(piv, EM, +1)
    return th, (anvil +
                f'<polyline {line} stroke="{green}" points="{ab_s}"/>'
                f'<polyline {line} stroke="{ember}" points="{em_s}"/>' +
                hammer_halo(th, piv, bg) + hammer_svg(th, piv))

def variant_b():
    piv = (233, TOP - w / 2)
    th = contact(piv, [q for q in EM if q[1] < TOP - 22], -1)
    return th, (anvil + arrow +
                f'<polyline {line} stroke="{green}" points="{ab_s}"/>'
                f'<polyline {line} stroke="{ember}" points="{em_s}"/>' + hammer_svg(th, piv))

BG = "#f3f1ec"
thb, _ = variant_b()
def b_inner(ink, green, ember):
    piv = (233, TOP - w / 2)
    deg = math.degrees(thb)
    ham = (f'<g fill="{ink}" transform="translate({piv[0]} {piv[1]}) rotate({deg:.2f})">'
           f'<rect x="{-HAND/2}" y="{-LH}" width="{HAND}" height="{LH}"/>'
           f'<rect x="{-HW/2}" y="{-LH-HT}" width="{HW}" height="{HT}" rx="3"/>'
           f'<rect x="{-HW/2-4}" y="{-LH-HT+2}" width="7" height="{HT-4}" rx="2"/></g>')
    return (f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{M.anvil_path(top=TOP)}"/>'
            f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>'
            f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>'
            f'<polyline {line} stroke="{green}" points="{ab_s}"/>'
            f'<polyline {line} stroke="{ember}" points="{em_s}"/>' + ham)
for n, inner in [("explore-fallen.svg", M.tile(b_inner(*M.L), bg=BG)),
                 ("explore-fallen-mark.svg", b_inner(*M.L)),
                 ("explore-fallen-mark-dark.svg", b_inner(*M.Dk))]:
    open(os.path.join(OUT, n), "w").write(M.svg(inner, "StokesForge — marteau tombé"))
print("angle", round(math.degrees(thb), 1))
