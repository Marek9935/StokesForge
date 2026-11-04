#!/usr/bin/env python3
"""Filter version of the glass logo. Reads the anvil and the spectra from the flat logo
(02-iterations/avant-liquid-glass/), then applies the retouches of #445 (hammer, emission).

Each shape is inflated (Poisson solve, then a saturating dome). The normals of that
surface give, per pixel:
  - a Snell refraction offset (R/G of a map read by feDisplacementMap), B = Fresnel;
  - Beer-Lambert absorption (thin edges let light through, the thick centre darkens);
  - directional light on the bevel, and the inner crescent opposite the light.
Everything but the displacement is a plain <mask> holding a grayscale image.

Usage: glass.py SOURCE.svg OUT.svg [--theme light|dark] [--opts k=v,k=v]
"""
import argparse, base64, io, re, subprocess, tempfile, os
import numpy as np
from PIL import Image
from scipy import ndimage as ndi
from scipy.sparse import coo_matrix
from scipy.sparse.linalg import spsolve

PPU = 4  # map pixels per logo unit


def rasterize(inner, box, ppu=PPU):
    x, y, w, h = box
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {w} {h}" '
           f'width="{w*ppu}" height="{h*ppu}">{inner}</svg>')
    with tempfile.TemporaryDirectory() as d:
        p, o = os.path.join(d, "s.svg"), os.path.join(d, "s.png")
        open(p, "w").write(svg)
        subprocess.run(["rsvg-convert", p, "-o", o], check=True)
        return np.asarray(Image.open(o).convert("RGBA"))[..., 3] / 255.0


def inflate(mask, ppu=PPU):
    """lap(u) = -1 inside, u = 0 on the edge; sqrt(2u) is an exact circular section on a
    ribbon of constant width, with no crease on the medial axis."""
    inside = mask > 0.5
    idx = -np.ones(mask.shape, int)
    n = inside.sum()
    idx[inside] = np.arange(n)
    ys, xs = np.nonzero(inside)
    rows, cols, vals = [np.arange(n)], [np.arange(n)], [np.full(n, 4.0)]
    for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0)):
        ny, nx = ys + dy, xs + dx
        ok = (ny >= 0) & (ny < mask.shape[0]) & (nx >= 0) & (nx < mask.shape[1])
        j = np.full(n, -1)
        j[ok] = idx[ny[ok], nx[ok]]
        has = j >= 0
        rows.append(np.arange(n)[has]); cols.append(j[has]); vals.append(-np.ones(has.sum()))
    A = coo_matrix((np.concatenate(vals), (np.concatenate(rows), np.concatenate(cols))),
                   shape=(n, n)).tocsc()
    u = np.zeros(mask.shape)
    u[inside] = spsolve(A, np.full(n, 1.0 / ppu**2))
    return np.sqrt(2 * np.maximum(u, 0))


def unit(v):
    v = np.array(v, float)
    return v / np.linalg.norm(v)


class Surface:
    """Height field of one shape and everything derived from it."""

    def __init__(self, mask, radius, ppu=PPU):
        self.mask, self.ppu, self.radius = mask, ppu, radius
        d = inflate(mask, ppu)
        h = radius * np.sqrt(1 - np.exp(-(d / radius) ** 2))
        self.h = ndi.gaussian_filter(h, 0.18 * ppu)
        gy, gx = np.gradient(self.h, 1.0 / ppu)
        self.nz = 1 / np.sqrt(1 + gx**2 + gy**2)
        self.nx, self.ny = -gx * self.nz, -gy * self.nz
        self.inside = mask > 0.02

    def light(self, direction, power):
        L = unit(direction)
        H = unit(L + np.array([0, 0, 1.0]))
        s = np.maximum(self.nx * H[0] + self.ny * H[1] + self.nz * H[2], 0) ** power
        return np.where(self.inside, s, 0)

    def fresnel(self, power=2.2, f0=0.0):
        return np.where(self.inside, f0 + (1 - f0) * (1 - self.nz) ** power, 0)

    def absorption(self, a):
        """Opacity of the tinted body: 1 - exp(-a h), Beer-Lambert."""
        return np.where(self.inside, 1 - np.exp(-a * self.h), 0)

    def crescent(self, direction, power=1.2):
        """Light gathered on the inner edge facing away from the light."""
        L = unit(direction)
        f = np.maximum(self.nx * L[0] + self.ny * L[1], 0) ** power
        band = np.clip(self.nz * 1.0, 0, 1) ** 0.5 * (1 - self.nz) ** 0.5
        return np.where(self.inside, f * band * 2, 0)

    def refraction(self, depth, ior=1.5):
        eta = 1 / ior
        k = np.sqrt(np.maximum(1 - eta**2 * (1 - self.nz**2), 0))
        f = eta * self.nz - k
        tz = -eta + f * self.nz
        thick = self.h + depth
        ox, oy = f * self.nx / -tz * thick, f * self.ny / -tz * thick
        ox[~self.inside] = oy[~self.inside] = 0
        return ox, oy


def png(arr, mode=None):
    b = io.BytesIO()
    Image.fromarray(arr, mode).save(b, "PNG", optimize=True)
    return "data:image/png;base64," + base64.b64encode(b.getvalue()).decode()


def gray(a):
    return png((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8))


DX = 6.0          # emission moved left: Stokes shift 36 -> 30, mirror symmetry kept
HX, HY = 67.0, 169.21   # handle butt, resting on the top of the horn
HA = 38.33        # angle that lays the striking face on the green maximum (155, 75)


def hammer_paths():
    """Clean hammer in its own frame (handle along -y, butt at the origin), in curves."""
    yc, f, h = -121.0, 10.0, 8.4   # the face top sits just above the green maximum
    def side(sg):
        y = lambda v: yc + sg * v
        return y
    t, u = side(-1), side(1)
    head = (f"M{f-1} {t(h)}Q{f} {t(h)} {f} {t(h-1)}L{f} {u(h-1)}Q{f} {u(h)} {f-1} {u(h)}"
            f"C6.6 {u(h)} 6.6 {u(6.1)} 4.4 {u(6.1)}"          # flared face into the collar
            f"L-6.5 {u(6.1)}C-13 {u(6.1)} -18 {u(3.2)} -23 {u(2.5)}"   # block, then the peen narrows
            f"Q-25.4 {u(2.2)} -25.4 {yc}Q-25.4 {t(2.2)} -23 {t(2.5)}"  # blunt peen
            f"C-18 {t(3.2)} -13 {t(6.1)} -6.5 {t(6.1)}L4.4 {t(6.1)}C6.6 {t(6.1)} 6.6 {t(h)} {f-1} {t(h)}Z")
    handle = f"M-2.3 {yc+5.5}L2.3 {yc+5.5}L3.2 -3.2A3.2 3.2 0 0 1 -3.2 -3.2Z"
    return f'<path d="{handle}"/><path d="{head}"/>'


def spark_lines():
    ox, oy = 156.0, 73.8   # top corner of the striking face
    out = []
    for ang, r0, ln in ((128, 8.0, 8.0), (97, 8.0, 11.0), (66, 8.0, 9.0), (36, 8.0, 7.0)):
        c, s_ = np.cos(np.radians(ang)), np.sin(np.radians(ang))
        out.append(f'<line x1="{ox+r0*c:.1f}" y1="{oy-r0*s_:.1f}" x2="{ox+(r0+ln)*c:.1f}" y2="{oy-(r0+ln)*s_:.1f}"/>')
    return "".join(out)


def grab(src):
    polys = re.findall(r'<polygon points="([^"]+)" fill="url\(#g[ae]\)"', src)
    anvil = re.search(r'<clipPath id="ac"><path d="([^"]+)" transform="([^"]+)"', src)
    ge = " ".join(f"{float(p.split(',')[0]) - DX:.1f},{p.split(',')[1]}" for p in polys[1].split())
    comet = f"M163.6 74L{191.5-DX} 74.2L{190.5-DX} 65.8L163.4 72.4Z"
    return dict(ga=polys[0], ge=ge, anvil=anvil.group(1), at=anvil.group(2),
                hamt=f"translate({HX} {HY}) rotate({HA})", ham=hammer_paths(),
                sparks=spark_lines(), comet=comet)


def mirror(points, y0=160.0, k=1.0):
    out = []
    for pt in points.split():
        x, y = pt.split(",")
        out.append(f"{x},{y0 + (y0 - float(y)) * k:.1f}")
    return " ".join(out)


THEMES = {
    "light": dict(tile0="#f8f6f2", tile1="#e7e4dc", ink="#23272c", ink2="#3a4149", halo="#f3f1ec",
                  glass=("#323940", "#1d2125", "#121417"), shadow=.30, rimn="#c9d2da", spec="#eaf0f4", inkd=.35, trim=.55),
    "dark": dict(tile0="#23282e", tile1="#0f1113", ink="#d7dcdc", ink2="#f2f4f3", halo="#1a1e21",
                 glass=("#454c54", "#2a2f35", "#1b1e22"), shadow=.65, rimn="#d3dbe2", spec="#f4f7f9", inkd=.62, trim=.2),
}


def build(src_path, theme="light", **o):
    g = grab(open(src_path).read())
    t = dict(THEMES[theme])
    P = dict(radius=8.0, depth=16.0, absorb=.42, refl=.8, rim=.85, spec=.8, cres=.7, mir=.8, mblur=.5, mk=.78,
             c_spec=.55, c_shade=.45, c_rim=.55, ink_spec=.75, glow=.9, gl=.35, pool=.8, cast=.28)
    P.update(o)
    if P.get('ftile'):
        t['tile0'] = t['tile1'] = '#f3f1ec' if theme == 'light' else '#15181a'
    edge = '#ffffff' if P.get('rimw') else 'url(#edge)'
    tile_rim = '' if P.get('ftile') else '<rect x=".5" y=".5" width="255" height="255" rx="55.5" fill="none" stroke="url(#tilerim)"/>\n'
    # Maps ------------------------------------------------------------------------------
    L1, L2 = (-.55, -.75, .55), (.6, .75, .5)
    a_box = (30, 152, 216, 76)
    a = Surface(rasterize(f'<path d="{g["anvil"]}" transform="{g["at"]}" fill="#fff"/>', a_box), P["radius"])
    ox, oy = a.refraction(P["depth"])
    a_scale = 2.02 * max(abs(ox).max(), abs(oy).max())
    disp = np.dstack([np.clip(.5 + ox / a_scale, 0, 1), np.clip(.5 + oy / a_scale, 0, 1),
                      a.fresnel(1.6), np.ones_like(ox)])
    disp = (disp * 255 + .5).astype(np.uint8)
    a_body = a.absorption(P["absorb"])
    a_rim = a.fresnel(2.6)
    a_spec = np.clip(a.light(L1, 60) + .35 * a.light(L2, 60), 0, 1)
    a_cres = a.crescent(L2)

    c_box = (104, 70, 136, 92)
    sg = Surface(rasterize(f'<polygon points="{g["ga"]}" fill="#fff"/>', c_box), 6.5)
    se = Surface(rasterize(f'<polygon points="{g["ge"]}" fill="#fff"/>', c_box), 6.5)
    front = se.mask > .5

    def both(fn):
        return np.where(front, fn(se), fn(sg))

    c_spec = both(lambda s: s.light(L1, 18))
    c_shade = both(lambda s: np.where(s.inside, np.maximum(s.nx * .75 + s.ny * .35, 0) ** 1.2, 0))
    c_rim = both(lambda s: s.fresnel(3.0))

    k_box = (44, 50, 124, 130)
    ham_inner = (f'<g transform="{g["hamt"]}" fill="#fff" stroke="#fff" stroke-width="1.2" '
                 f'stroke-linejoin="round">{g["ham"]}</g>')
    ax_inner = '<path d="M101.5 160V66h5V160Z" fill="#fff"/><path d="M96 74L104 56L112 74Z" fill="#fff"/>'
    sh = Surface(rasterize(ham_inner, k_box), 4.5)
    sx = Surface(rasterize(ax_inner, k_box), 3.0)
    k_ham = sh.light(L1, 14)
    k_ax = sx.light(L1, 14)
    shade = lambda q: np.where(q.inside, np.maximum(q.nx * .75 + q.ny * .55, 0) ** .9, 0)
    k_hd, k_xd = shade(sh), shade(sx)

    def mask(mid, uri, box):
        x, y, w, h = box
        return (f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="{x}" y="{y}" width="{w}" height="{h}">'
                f'<image x="{x}" y="{y}" width="{w}" height="{h}" preserveAspectRatio="none" href="{uri}"/></mask>')

    def region(x, y, w, h):
        return (f'filterUnits="userSpaceOnUse" primitiveUnits="userSpaceOnUse" x="{x}" y="{y}" '
                f'width="{w}" height="{h}" color-interpolation-filters="sRGB"')

    ax, ay, aw, ah = a_box
    alpha_b = "0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 1 0 0"
    defs = f'''
<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{t['tile0']}"/><stop offset="1" stop-color="{t['tile1']}"/></linearGradient>
<linearGradient id="tilerim" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="{t['trim']}"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".07"/></linearGradient>
<linearGradient id="ga" gradientUnits="userSpaceOnUse" x1="0" y1="75" x2="0" y2="160"><stop offset="0" stop-color="#45a866"/><stop offset=".55" stop-color="#2f8450"/><stop offset="1" stop-color="#1c5634"/></linearGradient>
<linearGradient id="ge" gradientUnits="userSpaceOnUse" x1="0" y1="75" x2="0" y2="160"><stop offset="0" stop-color="#f89f3e"/><stop offset=".35" stop-color="#e5681a"/><stop offset="1" stop-color="#a93a0c"/></linearGradient>
<linearGradient id="glass" gradientUnits="userSpaceOnUse" x1="0" y1="160" x2="0" y2="221"><stop offset="0" stop-color="{t['glass'][0]}"/><stop offset=".5" stop-color="{t['glass'][1]}"/><stop offset="1" stop-color="{t['glass'][2]}"/></linearGradient>
<linearGradient id="edge" gradientUnits="userSpaceOnUse" x1="36" y1="0" x2="236" y2="0"><stop offset="0" stop-color="{t['rimn']}"/><stop offset=".32" stop-color="{t['rimn']}"/><stop offset=".55" stop-color="#7fe6a0"/><stop offset=".72" stop-color="#ffd59a"/><stop offset="1" stop-color="#ffa44d"/></linearGradient>
<linearGradient id="fadeM" gradientUnits="userSpaceOnUse" x1="0" y1="160" x2="0" y2="208"><stop offset="0" stop-color="#fff"/><stop offset=".45" stop-color="#fff" stop-opacity=".45"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>
<mask id="fade" maskUnits="userSpaceOnUse" x="0" y="140" width="256" height="110"><rect x="0" y="140" width="256" height="110" fill="url(#fadeM)"/></mask>
<linearGradient id="fadeXg" gradientUnits="userSpaceOnUse" x1="{196-DX}" y1="0" x2="{242-DX}" y2="0"><stop offset="0" stop-color="#fff"/><stop offset=".55" stop-color="#fff" stop-opacity=".55"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>
<mask id="fadeX" maskUnits="userSpaceOnUse" x="0" y="140" width="256" height="110"><rect x="0" y="140" width="256" height="110" fill="url(#fadeXg)"/></mask>
<linearGradient id="cg" gradientUnits="userSpaceOnUse" x1="163.5" y1="0" x2="{191-DX}" y2="0"><stop offset="0" stop-color="#3a9155"/><stop offset="1" stop-color="#e0651a"/></linearGradient>
<linearGradient id="inkg" gradientUnits="userSpaceOnUse" x1="60" y1="60" x2="110" y2="170"><stop offset="0" stop-color="{t['ink2']}"/><stop offset="1" stop-color="{t['ink']}"/></linearGradient>
<linearGradient id="jun" gradientUnits="userSpaceOnUse" x1="0" y1="160" x2="0" y2="164"><stop offset="0" stop-color="#090b0d" stop-opacity=".92"/><stop offset=".35" stop-color="#090b0d" stop-opacity=".55"/><stop offset="1" stop-color="#090b0d" stop-opacity="0"/></linearGradient>
<linearGradient id="tint" gradientUnits="userSpaceOnUse" x1="100" y1="0" x2="236" y2="0"><stop offset="0" stop-color="#000" stop-opacity="0"/><stop offset=".3" stop-color="#46b06a"/><stop offset=".72" stop-color="#ff9a3c"/><stop offset="1" stop-color="#ff9a3c" stop-opacity=".3"/></linearGradient>
<clipPath id="ac"><path d="{g['anvil']}" transform="{g['at']}"/></clipPath>
<clipPath id="above"><rect x="0" y="0" width="256" height="160"/></clipPath>
<filter id="b2" {region(0,130,256,126)}><feGaussianBlur stdDeviation="{P['mblur']}"/></filter>
<filter id="b7" {region(0,120,256,136)}><feGaussianBlur stdDeviation="7"/></filter>
<filter id="glowf" {region(70,40,186,140)}><feGaussianBlur stdDeviation="2.4"/></filter>
<filter id="shadowf" {region(-20,130,296,126)}><feGaussianBlur stdDeviation="5.5"/></filter>
<filter id="hsh" {region(30,40,150,150)}><feGaussianBlur stdDeviation="2.2"/></filter>
<filter id="lights" {region(ax,ay,aw,ah)}><feImage href="{png(disp)}" x="{ax}" y="{ay}" width="{aw}" height="{ah}" preserveAspectRatio="none" result="m"/>
  <feDisplacementMap in="SourceGraphic" in2="m" scale="{a_scale:.3f}" xChannelSelector="R" yChannelSelector="G" result="refr"/>
  <feDisplacementMap in="SourceGraphic" in2="m" scale="{-a_scale*2.4:.3f}" xChannelSelector="R" yChannelSelector="G" result="d2"/>
  <feGaussianBlur in="d2" stdDeviation=".6" result="d2b"/>
  <feColorMatrix in="m" type="matrix" values="{alpha_b}" result="fa"/>
  <feComposite in="d2b" in2="fa" operator="in" result="r0"/><feComponentTransfer in="r0" result="refl"><feFuncA type="linear" slope="{P['refl']}"/></feComponentTransfer>
  <feMerge><feMergeNode in="refr"/><feMergeNode in="refl"/></feMerge></filter>
{mask('mBody', gray(a_body), a_box)}{mask('mRim', gray(a_rim), a_box)}{mask('mSpec', gray(a_spec), a_box)}{mask('mCres', gray(a_cres), a_box)}
{mask('mCs', gray(c_spec), c_box)}{mask('mCd', gray(c_shade), c_box)}{mask('mCr', gray(c_rim), c_box)}
{mask('mKh', gray(k_ham), k_box)}{mask('mKx', gray(k_ax), k_box)}{mask('mKhd', gray(k_hd), k_box)}{mask('mKxd', gray(k_xd), k_box)}
'''
    lights = f'''<g filter="url(#lights)"><rect x="{ax}" y="{ay}" width="{aw}" height="{ah}" fill="none"/>
<ellipse cx="150" cy="166" rx="40" ry="11" fill="#46b06a" opacity="{P['gl']}" filter="url(#b7)"/>
<ellipse cx="{196-DX}" cy="166" rx="44" ry="12" fill="#ff9a3c" opacity="{P['gl']}" filter="url(#b7)"/>
<ellipse cx="{228-DX}" cy="169" rx="16" ry="9" fill="#ff9a3c" opacity="{P['pool']}" filter="url(#b7)"/>
<g mask="url(#fadeX)"><g mask="url(#fade)"><g filter="url(#b2)" opacity="{P['mir']}"><polygon points="{mirror(g['ga'],k=P['mk'])}" fill="#4cc074"/><polygon points="{mirror(g['ge'],k=P['mk'])}" fill="#f4913f"/></g></g></g>
</g>'''
    ham = f'<g transform="{g["hamt"]}" stroke-width="1.2" stroke-linejoin="round" FILL>{g["ham"]}</g>'
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" role="img"><title>StokesForge</title>
<rect width="256" height="256" rx="56" fill="url(#tile)"/>
{tile_rim}<g transform="translate(14 6) scale(.9)"><defs>{defs}</defs>
<path d="{g['anvil']}" transform="translate(0 8) {g['at']}" fill="#000" opacity="{t['shadow']}" filter="url(#shadowf)"/>
<path d="{g['anvil']}" transform="translate(0 11) {g['at']}" fill="url(#tint)" opacity="{P['cast']}" filter="url(#shadowf)"/>
<g clip-path="url(#ac)">
<rect x="{ax}" y="{ay}" width="{aw}" height="{ah}" fill="url(#glass)" mask="url(#mBody)"/>
{lights}
<rect x="{ax}" y="{ay}" width="{aw}" height="{ah}" fill="{edge}" mask="url(#mCres)" opacity="{P['cres']}"/>
<rect x="{ax}" y="{ay}" width="{aw}" height="{ah}" fill="{edge}" mask="url(#mRim)" opacity="{P['rim']}"/>
<rect x="{ax}" y="{ay}" width="{aw}" height="{ah}" fill="{'#ffffff' if P.get('rimw') else t['spec']}" mask="url(#mSpec)" opacity="{P['spec']}"/>
<rect x="96" y="160" width="140" height="4" fill="url(#jun)"/>
</g>
<path d="M104 160V66" stroke="url(#inkg)" stroke-width="5"/><path d="M96 74L104 56L112 74Z" fill="url(#inkg)"/>
<rect x="{k_box[0]}" y="{k_box[1]}" width="{k_box[2]}" height="{k_box[3]}" fill="{t['spec']}" mask="url(#mKx)" opacity="{P['ink_spec']}"/>
<rect x="{k_box[0]}" y="{k_box[1]}" width="{k_box[2]}" height="{k_box[3]}" fill="#000" mask="url(#mKxd)" opacity="{t['inkd']}"/>
<g filter="url(#glowf)" clip-path="url(#above)" opacity="{P['glow']}"><polyline points="{g['ge'].rsplit(' ',2)[0]}" fill="none" stroke="#ffa040" stroke-width="5" opacity=".42"/><polyline points="{g['ga'].rsplit(' ',2)[0]}" fill="none" stroke="#4fbf72" stroke-width="5" opacity=".28"/></g>
<polygon points="{g['ga']}" fill="url(#ga)"/>
<rect x="{c_box[0]}" y="{c_box[1]}" width="{c_box[2]}" height="{c_box[3]}" fill="#000" mask="url(#mCd)" opacity="{P['c_shade']}" clip-path="url(#above)"/>
<polygon points="{g['ge']}" fill="url(#ge)"/>
<g clip-path="url(#curvesE)"><rect x="{c_box[0]}" y="{c_box[1]}" width="{c_box[2]}" height="{c_box[3]}" fill="#4a1503" mask="url(#mCd)" opacity="{P['c_shade']}"/></g>
<rect x="{c_box[0]}" y="{c_box[1]}" width="{c_box[2]}" height="{c_box[3]}" fill="#fff2d8" mask="url(#mCs)" opacity="{P['c_spec']}"/>
<rect x="{c_box[0]}" y="{c_box[1]}" width="{c_box[2]}" height="{c_box[3]}" fill="#fff" mask="url(#mCr)" opacity="{P['c_rim']}"/>
{ham.replace('FILL','fill="#000" stroke="#000" opacity="%.2f" filter="url(#hsh)"' % (t['shadow']*.8)).replace(g['hamt'], 'translate(2.5 3.5) ' + g['hamt'])}
{ham.replace('FILL','fill="url(#inkg2)" stroke="url(#inkg2)"')}
<rect x="{k_box[0]}" y="{k_box[1]}" width="{k_box[2]}" height="{k_box[3]}" fill="{t['spec']}" mask="url(#mKh)" opacity="{P['ink_spec']}"/>
<rect x="{k_box[0]}" y="{k_box[1]}" width="{k_box[2]}" height="{k_box[3]}" fill="#000" mask="url(#mKhd)" opacity="{t['inkd']}"/>
<g stroke="#e0651a" stroke-width="3.4" stroke-linecap="round">{g['sparks']}</g>
<path d="{g['comet']}" fill="url(#cg)"/><circle cx="{191-DX}" cy="70" r="5.6" fill="#e0651a"/>
</g></svg>
'''
    extra = (f'<linearGradient id="inkg2" gradientUnits="userSpaceOnUse" x1="0" y1="-130" x2="0" y2="0">'
             f'<stop offset="0" stop-color="{t["ink2"]}"/><stop offset="1" stop-color="{t["ink"]}"/></linearGradient>'
             f'<clipPath id="curvesE"><polygon points="{g["ge"]}"/></clipPath>')
    return svg.replace('</defs>', extra + '</defs>', 1)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("src"); ap.add_argument("out")
    ap.add_argument("--theme", default="light")
    ap.add_argument("--opts", default="")
    a = ap.parse_args()
    opts = {k: float(v) for k, v in (kv.split("=") for kv in a.opts.split(",") if kv)}
    s = build(a.src, a.theme, **opts)
    open(a.out, "w").write(s)
    print(a.out, f"{len(s)/1024:.0f} Ko")
