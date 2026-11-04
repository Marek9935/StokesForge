import math, os, sys
sys.path.insert(0, os.path.dirname(__file__))
OUT = sys.argv[1]; os.makedirs(OUT, exist_ok=True)
from make import anvil_path, hammer, sparks, C   # reuse anvil, hammer, sparks (make.py also rewrites its own files)

SIG, D, A, SIG2 = 8.0, 20.0, 0.6, 9.0   # main band, shoulder offset, shoulder amplitude, shoulder width (px)
def band(x, mu, side):   # side=+1: shoulder to the right (emission, low energy); -1: to the left (absorption)
    g = lambda m, s: math.exp(-((x - m) ** 2) / (2 * s * s))
    return g(mu, SIG) + A * g(mu + side * D, SIG2)
def peak_norm(mu, side):
    return max(band(mu + i * 0.1, mu, side) for i in range(-50, 50))
def spectrum(mu, side, base, height, x_from, x_to):
    n = peak_norm(mu, side); pts = []
    x = x_from
    while x <= x_to:
        pts.append(f"{x:.1f},{base - height * band(x, mu, side) / n:.1f}"); x += 1.5
    return " ".join(pts)
def peak_x(mu, side):
    return max((band(mu + i * 0.05, mu, side), mu + i * 0.05) for i in range(-100, 100))[1]

TOP, AX = 160, 104
S = 30                       # Stokes shift between the two maxima (px)
MU_A, MU_E = 155, 155 + S    # absorption (green), emission (orange)
H = 80

def mark(ink, green, ember, axis_hammer=False, glow=False, minimal=False, w=6, sw=10):
    peak_y = TOP - H
    pa = peak_x(MU_A, -1)
    parts = []
    defs = ""
    if glow:
        defs = (f'<defs><linearGradient id="hot" x1="0" y1="{peak_y}" x2="0" y2="{TOP}" gradientUnits="userSpaceOnUse">'
                f'<stop offset="0" stop-color="#ffd166"/><stop offset=".55" stop-color="{ember}"/><stop offset="1" stop-color="#b8420f"/></linearGradient>'
                f'<filter id="blur" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="5"/></filter></defs>')
    parts.append(defs)
    parts.append(f'<path fill="none" stroke="{ink}" stroke-width="{w}" stroke-linejoin="round" d="{anvil_path(top=TOP)}"/>')
    if not minimal:
        if axis_hammer:
            # the ordinate is the hammer's handle; its head strikes the absorption band's peak from the left
            # the ordinate is the handle of an upright hammer: the head replaces the arrow
            parts.append(f'<path d="M{AX} {TOP}V{peak_y-8}" stroke="{ink}" stroke-width="{w+2}"/>')
            parts.append(f'<rect x="{AX-24}" y="{peak_y-30}" width="48" height="22" rx="3" fill="{ink}"/>')
            parts.append(f'<rect x="{AX-28}" y="{peak_y-28}" width="7" height="18" rx="2" fill="{ink}"/>')
        else:
            parts.append(f'<path d="M{AX} {TOP}V{peak_y-14}" stroke="{ink}" stroke-width="{w}"/>')
            parts.append(f'<path d="M{AX-8} {peak_y-6}L{AX} {peak_y-24}L{AX+8} {peak_y-6}Z" fill="{ink}"/>')
    base, hh = TOP - sw / 2, H - sw / 2
    ab = spectrum(MU_A, -1, base, hh, AX + 6, MU_A + 30)
    em = spectrum(MU_E, +1, base, hh, MU_E - 30, 232)
    line = 'fill="none" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round"'.format(sw)
    parts.append(f'<polyline {line} stroke="{green}" points="{ab}"/>')
    if glow:
        parts.append(f'<polyline {line} stroke="{ember}" stroke-opacity=".55" filter="url(#blur)" points="{em}"/>')
        parts.append(f'<polyline {line} stroke="url(#hot)" points="{em}"/>')
    else:
        parts.append(f'<polyline {line} stroke="{ember}" points="{em}"/>')
    if not minimal and not axis_hammer:
        parts.append(sparks(pa, peak_y - sw / 2, ember, r0=7, r1=14, w=5))
        parts.append(hammer(pa, peak_y - sw / 2 - 2, -14, ink, scale=0.75))
    return "".join(parts)

def svg(inner, title, vb="0 0 256 256", w=256, h=256):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{w}" height="{h}" role="img"><title>{title}</title>{inner}</svg>\n'
def tile(inner, bg="#f3f1ec"):
    return f'<rect width="256" height="256" rx="56" fill="{bg}"/><g transform="translate(14 6) scale(.9)">{inner}</g>'

L = (C["ink"], C["green"], C["ember"]); Dk = (C["inkd"], C["greend"], C["emberd"])
def lockup(colors, ground=None):
    ink = colors[0]
    t = (f'<text x="262" y="166" font-family="system-ui, \'Segoe UI\', Roboto, \'Noto Sans\', sans-serif" font-size="88" letter-spacing="-1.5" fill="{ink}">'
         f'<tspan font-weight="400">Stokes</tspan><tspan font-weight="700">Forge</tspan></text>')
    return svg(mark(*colors) + t, "StokesForge", vb="0 0 860 256", w=860, h=256)

files = {
  "stokesforge-icon.svg": svg(tile(mark(*L)), "StokesForge"),
  "stokesforge-mark.svg": svg(mark(*L), "StokesForge"),
  "stokesforge-mark-dark.svg": svg(mark(*Dk), "StokesForge"),
  "stokesforge-lockup.svg": lockup(L),
  "stokesforge-lockup-dark.svg": lockup(Dk),
  "explore-axis-hammer.svg": svg(tile(mark(*L, axis_hammer=True)), "StokesForge — axe-marteau"),
  "explore-hot-emission.svg": svg(tile(mark(*L, glow=True)), "StokesForge — émission incandescente"),
  "explore-favicon.svg": svg(tile(mark(*L, minimal=True, w=9, sw=14)), "StokesForge — favicon"),
  "explore-dark-tile.svg": svg(tile(mark(*Dk, glow=True), bg="#15181a"), "StokesForge — forge de nuit"),
}
for n, s in files.items():
    open(os.path.join(OUT, n), "w").write(s)
print("ok", peak_x(MU_A, -1), peak_x(MU_E, 1))
