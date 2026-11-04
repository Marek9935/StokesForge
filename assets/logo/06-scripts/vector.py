#!/usr/bin/env python3
"""Filter-free, image-free version of the glass logo: every computed layer (refraction,
absorption, light, blur) is baked in numpy, then traced as a stack of nested iso-contours
whose opacities compose back to the exact alpha.

Usage: vector.py SOURCE.svg OUT.svg [--theme light|dark] [--levels N] [--opts k=v,...]
"""
import argparse
import numpy as np
from contourpy import contour_generator, FillType
from scipy import ndimage as ndi
from glass import rasterize, Surface, grab, mirror, THEMES, PPU, DX, HX, HY

K = PPU


def rdp(pts, eps):
    """Douglas-Peucker on an open polyline (iterative)."""
    if len(pts) < 3:
        return pts
    keep = np.zeros(len(pts), bool)
    keep[0] = keep[-1] = True
    stack = [(0, len(pts) - 1)]
    while stack:
        a, b = stack.pop()
        if b <= a + 1:
            continue
        p, q = pts[a], pts[b]
        d = q - p
        n = np.hypot(*d)
        seg = pts[a + 1:b]
        dist = np.hypot(*(seg - p).T) if n == 0 else np.abs(d[0] * (seg[:, 1] - p[1]) - d[1] * (seg[:, 0] - p[0])) / n
        i = dist.argmax()
        if dist[i] > eps:
            m = a + 1 + i
            keep[m] = True
            stack += [(a, m), (m, b)]
    return pts[keep]


def ring_path(pts, eps, prec=100):
    pts = pts[:-1] if np.allclose(pts[0], pts[-1]) else pts
    if len(pts) < 3:
        return ""
    # split the closed ring in two to run the open-curve simplifier
    far = np.hypot(*(pts - pts[0]).T).argmax()
    a = rdp(pts[: far + 1], eps)
    b = rdp(np.vstack([pts[far:], pts[:1]]), eps)
    pts = np.vstack([a[:-1], b[:-1]])
    if len(pts) < 3:
        return ""
    # integer hundredths, relative moves: shortest text at this precision
    q = np.round(pts * prec).astype(int)
    d = np.diff(q, axis=0)
    def num(v):
        s = f"{v/prec:.{2 if prec==100 else 1}f}"
        s = s.rstrip("0").rstrip(".") if "." in s else s
        s = s.replace("0.", ".", 1) if s.startswith(("0.", "-0.")) else s
        return "0" if s in ("", "-", "-0") else s
    out = [f"M{num(q[0,0])} {num(q[0,1])}l"]
    parts = []
    for dx, dy in d:
        if dx == 0 and dy == 0:
            continue
        parts.append(f"{num(dx)} {num(dy)}")
    return out[0] + " ".join(parts) + "z"


def stack(alpha, box, fill, levels, eps=.04, floor=.004, prec=100):
    """Trace alpha (0..1) over box as nested contours; returns SVG paths."""
    x0, y0, _, _ = box
    alpha = np.clip(np.nan_to_num(alpha), 0, 1)
    peak = float(alpha.max())
    if peak < floor:
        return ""
    # a closed band: zero margin so that contours never leave the array open
    a = np.pad(alpha, 1)
    ys = y0 + (np.arange(a.shape[0]) - 1 + .5) / K
    xs = x0 + (np.arange(a.shape[1]) - 1 + .5) / K
    gen = contour_generator(xs, ys, a, fill_type=FillType.OuterOffset)
    out, prev = [], 0.0
    n = levels
    for k in range(1, n + 1):
        target = peak * k / n              # alpha wanted once this layer is painted
        th = peak * (k - .5) / n
        delta = 1 - (1 - target) / (1 - prev)
        prev = target
        polys, offsets = gen.filled(th, 2.0)
        d = []
        for pts, off in zip(polys, offsets):
            for i in range(len(off) - 1):
                r = ring_path(pts[off[i]:off[i + 1]], eps, prec)
                if r:
                    d.append(r)
        if d:
            out.append(f'<path fill-opacity="{delta:.3f}" d="{"".join(d)}"/>')
    return f'<g fill="{fill}" fill-rule="evenodd">{"".join(out)}</g>\n'


def blur(a, sigma):
    return ndi.gaussian_filter(a, sigma * K)


def over(*layers):
    r = np.zeros_like(layers[0])
    for l in layers:
        r = l + r * (1 - l)
    return r


def build(src_path, theme="light", levels=14, **o):
    g = grab(open(src_path).read())
    t = dict(THEMES[theme])
    P = dict(radius=8.0, depth=16.0, absorb=.42, refl=.8, rim=.85, spec=.8, cres=.7, mir=.8, mblur=.5, mk=.78,
             c_spec=.55, c_shade=.45, c_rim=.55, ink_spec=.75, glow=.9, gl=.35, pool=.8, cast=.28)
    P.update(o)
    if P.get('ftile'):
        t['tile0'] = t['tile1'] = '#f3f1ec' if theme == 'light' else '#15181a'
        t['trim'] = 0
    if 'shadow' in P:
        t['shadow'] = P['shadow']
    if 'inkd' in P:
        t['inkd'] = P['inkd']
    edge = '#ffffff' if P.get('rimw') else 'url(#edge)'
    L1, L2 = (-.55, -.75, .55), (.6, .75, .5)
    a_inner = f'<path d="{g["anvil"]}" transform="{g["at"]}" fill="#fff"/>'
    # Anvil ----------------------------------------------------------------------------
    a_box = (30, 152, 216, 76)
    a = Surface(rasterize(a_inner, a_box), P["radius"])
    ox, oy = a.refraction(P["depth"])
    H, W = a.mask.shape
    # Lights behind the glass are rendered on a wider plane: the rays leave a_box
    l_box = (0, 120, 256, 136)
    def R(inner, box=l_box):
        return rasterize(inner, box)
    fx = R(f'<rect x="196" y="120" width="60" height="136" fill="#fff"/>')
    xs = l_box[0] + (np.arange(fx.shape[1]) + .5) / K
    ys = l_box[1] + (np.arange(fx.shape[0]) + .5) / K
    fade_x = np.interp(xs, [196 - DX, 196 - DX + .55 * 46, 242 - DX], [1, .55, 0])[None, :]
    fade_y = np.interp(ys, [160, 160 + .45 * 48, 208], [1, .45, 0])[:, None]
    ell = lambda cx, cy, rx, ry, s, op: blur(R(f'<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="#fff"/>'), s) * op
    mg = blur(R(f'<polygon points="{mirror(g["ga"], k=P["mk"])}" fill="#fff"/>'), P["mblur"]) * P["mir"] * fade_x * fade_y
    me = blur(R(f'<polygon points="{mirror(g["ge"], k=P["mk"])}" fill="#fff"/>'), P["mblur"]) * P["mir"] * fade_x * fade_y
    src_g = over(ell(150, 166, 40, 11, 7, P["gl"]), mg)
    src_e = over(ell(196 - DX, 166, 44, 12, 7, P["gl"]), ell(228 - DX, 169, 16, 9, 7, P["pool"]), me)

    def sample(src, dx, dy):
        yy, xx = np.mgrid[0:H, 0:W]
        px = xx + a_box[0] * K - l_box[0] * K + dx * K
        py = yy + a_box[1] * K - l_box[1] * K + dy * K
        return ndi.map_coordinates(src, [py, px], order=1, mode="constant") * a.inside

    if P.get('ripple'):
        yy, xx = np.mgrid[0:H, 0:W]
        rx = a_box[0] + (xx + .5) / K - 155.0
        ry = a_box[1] + (yy + .5) / K - 160.0
        rr = np.hypot(rx, ry) + 1e-6
        wave = P['ripple'] * np.sin(2 * np.pi * rr / P.get('wl', 13.0)) * np.exp(-rr / P.get('decay', 48.0))
        ox = ox + wave * rx / rr
        oy = oy + wave * ry / rr
    fres = a.fresnel(1.6)
    rg, re_ = sample(src_g, ox, oy), sample(src_e, ox, oy)
    sc = 2.4
    fg = ndi.gaussian_filter(sample(src_g, -sc * ox, -sc * oy), .6 * K) * fres * P["refl"]
    fe = ndi.gaussian_filter(sample(src_e, -sc * ox, -sc * oy), .6 * K) * fres * P["refl"]
    body = a.absorption(P["absorb"])
    rim = a.fresnel(2.6) * P["rim"]
    spec = np.clip(a.light(L1, 60) + .35 * a.light(L2, 60), 0, 1) * P["spec"]
    cres = a.crescent(L2) * P["cres"]
    # Shadows on the tile --------------------------------------------------------------
    s_box = (-20, 130, 296, 126)
    shadow = blur(rasterize(f'<path d="{g["anvil"]}" transform="translate(0 8) {g["at"]}" fill="#fff"/>', s_box), 5.5) * t["shadow"]
    cast = blur(rasterize(f'<path d="{g["anvil"]}" transform="translate(0 11) {g["at"]}" fill="#fff"/>', s_box), 5.5) * P["cast"]
    # Curves ---------------------------------------------------------------------------
    c_box = (104, 70, 136, 92)
    sg = Surface(rasterize(f'<polygon points="{g["ga"]}" fill="#fff"/>', c_box), 6.5)
    se = Surface(rasterize(f'<polygon points="{g["ge"]}" fill="#fff"/>', c_box), 6.5)
    front = se.mask > .5
    both = lambda fn: np.where(front, fn(se), fn(sg))
    shade = lambda s, a_=.75, b_=.35, p=1.2: np.where(s.inside, np.maximum(s.nx * a_ + s.ny * b_, 0) ** p, 0)
    c_spec = both(lambda s: s.light(L1, 18)) * P["c_spec"]
    c_rim = both(lambda s: s.fresnel(3.0)) * P["c_rim"]
    d_g, d_e = shade(sg) * P["c_shade"], shade(se) * P["c_shade"]
    # Glow ------------------------------------------------------------------------------
    w_box = (70, 40, 186, 120)
    gl_e = blur(rasterize(f'<polyline points="{g["ge"].rsplit(" ",2)[0]}" fill="none" stroke="#fff" stroke-width="5"/>', w_box), 2.4) * .42 * P["glow"]
    gl_g = blur(rasterize(f'<polyline points="{g["ga"].rsplit(" ",2)[0]}" fill="none" stroke="#fff" stroke-width="5"/>', w_box), 2.4) * .28 * P["glow"]
    # Ink -------------------------------------------------------------------------------
    k_box = (44, 50, 124, 130)
    ham_inner = (f'<g transform="{g["hamt"]}" fill="#fff" stroke="#fff" stroke-width="1.2" stroke-linejoin="round">{g["ham"]}</g>')
    ax_inner = '<path d="M101.5 160V66h5V160Z" fill="#fff"/><path d="M96 74L104 56L112 74Z" fill="#fff"/>'
    sh = Surface(rasterize(ham_inner, k_box), 4.5)
    sx = Surface(rasterize(ax_inner, k_box), 3.0)
    hs_box = (30, 40, 150, 150)
    ham_sh = blur(rasterize(ham_inner.replace(g["hamt"], "translate(2.5 3.5) " + g["hamt"]), hs_box), 2.2) * t["shadow"] * .8
    ks = lambda q: np.where(q.inside, np.maximum(q.nx * .75 + q.ny * .55, 0) ** .9, 0)
    defs = f'''<defs>
<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{t['tile0']}"/><stop offset="1" stop-color="{t['tile1']}"/></linearGradient>
<linearGradient id="tilerim" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="{t['trim']}"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".07"/></linearGradient>
<linearGradient id="ga" gradientUnits="userSpaceOnUse" x1="0" y1="75" x2="0" y2="160"><stop offset="0" stop-color="#45a866"/><stop offset=".55" stop-color="#2f8450"/><stop offset="1" stop-color="#1c5634"/></linearGradient>
<linearGradient id="ge" gradientUnits="userSpaceOnUse" x1="0" y1="75" x2="0" y2="160"><stop offset="0" stop-color="#f89f3e"/><stop offset=".35" stop-color="#e5681a"/><stop offset="1" stop-color="#a93a0c"/></linearGradient>
<linearGradient id="glass" gradientUnits="userSpaceOnUse" x1="0" y1="160" x2="0" y2="221"><stop offset="0" stop-color="{t['glass'][0]}"/><stop offset=".5" stop-color="{t['glass'][1]}"/><stop offset="1" stop-color="{t['glass'][2]}"/></linearGradient>
<linearGradient id="edge" gradientUnits="userSpaceOnUse" x1="36" y1="0" x2="236" y2="0"><stop offset="0" stop-color="{t['rimn']}"/><stop offset=".32" stop-color="{t['rimn']}"/><stop offset=".55" stop-color="#7fe6a0"/><stop offset=".72" stop-color="#ffd59a"/><stop offset="1" stop-color="#ffa44d"/></linearGradient>
<linearGradient id="tint" gradientUnits="userSpaceOnUse" x1="100" y1="0" x2="236" y2="0"><stop offset="0" stop-color="#46b06a" stop-opacity="0"/><stop offset=".3" stop-color="#46b06a"/><stop offset=".72" stop-color="#ff9a3c"/><stop offset="1" stop-color="#ff9a3c" stop-opacity=".3"/></linearGradient>
<linearGradient id="jun" gradientUnits="userSpaceOnUse" x1="0" y1="160" x2="0" y2="164"><stop offset="0" stop-color="#090b0d" stop-opacity=".92"/><stop offset=".35" stop-color="#090b0d" stop-opacity=".55"/><stop offset="1" stop-color="#090b0d" stop-opacity="0"/></linearGradient>
<linearGradient id="cg" gradientUnits="userSpaceOnUse" x1="163.5" y1="0" x2="{191-DX}" y2="0"><stop offset="0" stop-color="#3a9155"/><stop offset="1" stop-color="#e0651a"/></linearGradient>
<linearGradient id="inkg" gradientUnits="userSpaceOnUse" x1="60" y1="60" x2="110" y2="170"><stop offset="0" stop-color="{t['ink2']}"/><stop offset="1" stop-color="{t['ink']}"/></linearGradient>
<linearGradient id="inkg2" gradientUnits="userSpaceOnUse" x1="0" y1="-130" x2="0" y2="0"><stop offset="0" stop-color="{t['ink2']}"/><stop offset="1" stop-color="{t['ink']}"/></linearGradient>
<clipPath id="ac"><path d="{g['anvil']}" transform="{g['at']}"/></clipPath>
<clipPath id="above"><rect x="0" y="0" width="256" height="160"/></clipPath>
<clipPath id="pe"><polygon points="{g['ge']}"/></clipPath>
<clipPath id="pg"><polygon points="{g['ga']}"/></clipPath>
</defs>'''
    q = levels / 14
    def S(arr, box, fill, n, soft=False):
        n = max(2, round(n * q))
        return stack(arr, box, fill, n, eps=.14, prec=10) if soft else stack(arr, box, fill, n, eps=.05, prec=100)
    ham = f'<g transform="{g["hamt"]}" fill="url(#inkg2)" stroke="url(#inkg2)" stroke-width="1.2" stroke-linejoin="round">{g["ham"]}</g>'
    tile_rim = '' if P.get('ftile') else '<rect x=".5" y=".5" width="255" height="255" rx="55.5" fill="none" stroke="url(#tilerim)"/>\n'
    cast_layer = '' if not P['cast'] else f'<path d="{g["anvil"]}" transform="translate(0 11) {g["at"]}" fill="url(#tint)" opacity="{P["cast"]}" filter="url(#sh)"/>\n'
    A = bool(P.get('anim'))
    T = 'dur="4.2s" repeatCount="indefinite"'
    an_ham = f'<animateTransform attributeName="transform" type="rotate" {T} calcMode="spline" keyTimes="0;.42;.62;.7;.76;.8;.85;1" keySplines=".4 0 .6 1;.3 0 .3 1;0 0 1 1;.7 0 1 .6;0 .4 .4 1;.4 0 .6 1;0 0 1 1" values="0 {HX} {HY};0 {HX} {HY};-15 {HX} {HY};-15 {HX} {HY};0 {HX} {HY};-1.8 {HX} {HY};0 {HX} {HY};0 {HX} {HY}"/>' if A else ''
    an_sp = f'<animate attributeName="opacity" {T} keyTimes="0;.44;.5;.76;.765;1" values="1;1;0;0;1;1"/>' if A else ''
    an_cm = f'<animate attributeName="opacity" {T} keyTimes="0;.44;.5;.78;.82;1" values="1;1;0;0;1;1"/>' if A else ''
    an_cmx = f'<animateTransform attributeName="transform" type="scale" additive="sum" {T} keyTimes="0;.78;.88;1" values="1 1;.02 1;1 1;1 1"/>' if A else ''
    an_dot = f'<animate attributeName="r" {T} keyTimes="0;.86;.91;1" values="5.6;5.6;7.6;5.6"/>' if A else ''
    fl_g = f'<polygon points="{g["ga"]}" fill="#d8ffe4" opacity="0"><animate attributeName="opacity" {T} keyTimes="0;.76;.77;.88;1" values="0;0;.38;0;0"/></polygon>' if A else ''
    fl_e = f'<polygon points="{g["ge"]}" fill="#fff0d0" opacity="0"><animate attributeName="opacity" {T} keyTimes="0;.85;.91;1" values="0;0;.34;0"/></polygon>' if A else ''
    def reg(x, y, w, h):
        return f'filterUnits="userSpaceOnUse" x="{x}" y="{y}" width="{w}" height="{h}" color-interpolation-filters="sRGB"'
    filters = f'''<filter id="sh" {reg(-20,130,296,126)}><feGaussianBlur stdDeviation="5.5"/></filter>
<filter id="gw" {reg(70,40,186,125)}><feGaussianBlur stdDeviation="2.4"/></filter>
<filter id="hs" {reg(30,40,150,150)}><feGaussianBlur stdDeviation="2.2"/></filter>
<filter id="sg" {reg(26,150,224,82)}><feGaussianBlur stdDeviation="{P.get('sg', .32)}"/></filter>
<filter id="sc" {reg(100,66,144,98)}><feGaussianBlur stdDeviation="{P.get('sc', .4)}"/></filter>
<filter id="sk" {reg(40,46,132,138)}><feGaussianBlur stdDeviation=".22"/></filter>'''
    defs = defs.replace('</defs>', filters + '<clipPath id="cu"><polygon points="' + g['ga'] + '"/><polygon points="' + g['ge'] + '"/></clipPath></defs>')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" role="img"><title>StokesForge</title>
<rect width="256" height="256" rx="56" fill="url(#tile)"/>
{tile_rim}<g transform="translate(14 6) scale(.9)">{defs}
<path d="{g['anvil']}" transform="translate(0 8) {g['at']}" fill="#000" opacity="{t['shadow']}" filter="url(#sh)"/>
{cast_layer}<g clip-path="url(#ac)">
<g filter="url(#sg)">{S(body, a_box, 'url(#glass)', 24, True)}{S(rg, a_box, '#4cc074', 16, True)}{S(re_, a_box, '#f4913f', 16, True)}{S(fg, a_box, '#4cc074', 8, True)}{S(fe, a_box, '#f4913f', 8, True)}
{S(cres, a_box, edge, 8, True)}{S(rim, a_box, edge, 10)}{S(spec, a_box, '#ffffff' if P.get('rimw') else t['spec'], 10)}</g>
<rect x="96" y="160" width="140" height="4" fill="url(#jun)"/>
</g>
<path d="M104 160V66" stroke="url(#inkg)" stroke-width="5"/><path d="M96 74L104 56L112 74Z" fill="url(#inkg)"/>
<g filter="url(#sk)">{S(sx.light(L1, 14) * P['ink_spec'], k_box, t['spec'], 8, True)}{S(ks(sx) * t['inkd'], k_box, '#000', 8, True)}</g>
<g clip-path="url(#above)"><g filter="url(#gw)" opacity="{P['glow']}"><polyline points="{g['ge'].rsplit(' ',2)[0]}" fill="none" stroke="#ffa040" stroke-width="5" opacity=".42"/><polyline points="{g['ga'].rsplit(' ',2)[0]}" fill="none" stroke="#4fbf72" stroke-width="5" opacity=".28"/></g></g>
<polygon points="{g['ga']}" fill="url(#ga)"/>
<g clip-path="url(#pg)"><g filter="url(#sc)">{S(d_g, c_box, '#000', 12, True)}</g></g>
<polygon points="{g['ge']}" fill="url(#ge)"/>
<g clip-path="url(#pe)"><g filter="url(#sc)">{S(d_e, c_box, '#4a1503', 12, True)}</g></g>
<g clip-path="url(#cu)"><g filter="url(#sc)">{S(c_spec, c_box, '#fff2d8', 12, True)}{S(c_rim, c_box, '#fff', 10)}</g></g>
<g>{an_ham}<g transform="translate(2.5 3.5) {g['hamt']}" fill="#000" stroke="#000" stroke-width="1.2" stroke-linejoin="round" opacity="{t['shadow']*.8:.2f}" filter="url(#hs)">{g['ham']}</g>
{ham}
<g filter="url(#sk)">{S(sh.light(L1, 14) * P['ink_spec'], k_box, t['spec'], 8, True)}{S(ks(sh) * t['inkd'], k_box, '#000', 8, True)}</g></g>
{fl_g}{fl_e}
<g stroke="#e0651a" stroke-width="3.4" stroke-linecap="round">{an_sp}{g['sparks']}</g>
<g transform="translate(163.5 0)"><g>{an_cmx}<g transform="translate(-163.5 0)"><path d="{g['comet']}" fill="url(#cg)">{an_cm}</path></g></g></g><circle cx="{191-DX}" cy="70" r="5.6" fill="#e0651a">{an_dot}</circle>
</g></svg>
'''


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("src"); ap.add_argument("out")
    ap.add_argument("--theme", default="light")
    ap.add_argument("--levels", type=int, default=18)
    ap.add_argument("--opts", default="")
    a = ap.parse_args()
    opts = {k: float(v) for k, v in (kv.split("=") for kv in a.opts.split(",") if kv)}
    s = build(a.src, a.theme, a.levels, **opts)
    import re
    for gid in re.findall(r'<linearGradient id="(\w+)"', s):
        if f'url(#{gid})' not in s:
            s = re.sub(r'<linearGradient id="%s".*?</linearGradient>\n?' % gid, '', s, flags=re.S)
    open(a.out, "w").write(s)
    print(a.out, f"{len(s)/1024:.0f} Ko")
