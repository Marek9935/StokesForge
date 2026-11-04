"""Band geometry shared by every mark.

Absorption: main Gaussian + one vibronic shoulder Gaussian on its LEFT
(high-energy) side, tuned so the sum is monotonic on that side (shoulder,
no dip, no second peak). Emission: exact mirror image of the absorption
about the vertical line halfway between the two maxima.
"""
import math

import numpy as np

SH_AMP = 0.50      # shoulder amplitude relative to the main band
SH_OFF = 2.05      # shoulder offset, in units of sigma (to the left)
SH_SIG = 0.72      # shoulder width, in units of sigma


def unit(u):
    """Absorption profile in reduced units (sigma = 1, main centre at 0)."""
    return np.exp(-u**2 / 2) + SH_AMP * np.exp(-(u + SH_OFF) ** 2 / (2 * SH_SIG**2))


_U = np.linspace(-6, 6, 24001)
_P = unit(_U)
U_MAX = float(_U[np.argmax(_P)])
P_MAX = float(_P.max())


def check():
    left = _P[_U <= U_MAX]
    d = np.diff(left)
    assert (d > 0).all(), "dip or second peak on the shoulder side"
    # a shoulder exists: the left flank has an extra inflection pair
    mx = [i for i in range(1, len(d) - 1) if d[i] > d[i - 1] and d[i] >= d[i + 1]]
    mn = [i for i in range(1, len(d) - 1) if d[i] < d[i - 1] and d[i] <= d[i + 1]]
    assert len(mx) >= 2 and mn, "no visible shoulder"
    return round(float(d[mn[0]] / d.max()), 3)


def stokes_for_overlap(level):
    """Gap between maxima (in sigma) so the inner flanks cross at `level` of max."""
    # inner flank of the absorption (right side) is right of U_MAX
    u = _U[_U >= U_MAX]
    p = _P[_U >= U_MAX] / P_MAX
    x = float(u[np.argmin(abs(p - level))]) - U_MAX
    return 2 * x


class Pair:
    """Absorption/emission pair placed in SVG coordinates."""

    def __init__(self, xa, xe, base, height, sigma):
        self.xa, self.xe, self.base, self.h, self.s = xa, xe, base, height, sigma
        self.mid = (xa + xe) / 2

    def abs_y(self, x):
        u = (x - self.xa) / self.s + U_MAX
        return self.base - self.h * unit(u) / P_MAX

    def em_y(self, x):
        return self.abs_y(2 * self.mid - x)

    def _pts(self, f, x0, x1, n=160):
        xs = np.linspace(x0, x1, n)
        return [(float(x), float(f(x))) for x in xs]

    def abs_pts(self, x0=None, x1=None, n=160):
        x0 = self.xa - 4.6 * self.s if x0 is None else x0
        x1 = self.xa + 3.4 * self.s if x1 is None else x1
        return self._pts(self.abs_y, x0, x1, n)

    def em_pts(self, x0=None, x1=None, n=160):
        x0 = self.xe - 3.4 * self.s if x0 is None else x0
        x1 = self.xe + 4.6 * self.s if x1 is None else x1
        return self._pts(self.em_y, x0, x1, n)

    def peak_y(self):
        return self.base - self.h


def path(pts, close_to=None):
    s = "M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    if close_to is not None:
        s += f" L{pts[-1][0]:.2f},{close_to:.2f} L{pts[0][0]:.2f},{close_to:.2f} Z"
    return s


if __name__ == "__main__":
    print("inflections", check(), "u_max", U_MAX, "p_max", P_MAX)
    for lv in (0.08, 0.12, 0.15, 0.2):
        print(lv, stokes_for_overlap(lv))
    # FWHM of the full absorption in sigma
    half = _P >= P_MAX / 2
    print("FWHM sigma", _U[half][-1] - _U[half][0])
