#!/usr/bin/env python3
"""Mirror art. Every layer is authored as palette INDICES, not colours:
index k is stored as rgb(8k, 0, 0), which the importer sorts into CGRAM
slot k. main.lua then owns every real colour (and changes them per line).

Only rows 0..HORIZON-1 are ever shown directly. The lake below the horizon
is the same rows read upside down by HDMA, so nothing below is drawn.
"""
from pathlib import Path
import numpy as np
from PIL import Image

OUT = Path(__file__).resolve().parent / "assets"
W, H = 512, 256
HORIZON = 132
rng = np.random.default_rng(7)


def save(name, idx, w=W, h=H):
    a = np.zeros((h, w, 4), np.uint8)
    a[..., 0] = idx * 8
    a[..., 3] = np.where(idx > 0, 255, 0)
    Image.fromarray(a, "RGBA").save(OUT / f"{name}.png")


def ridge(n, base, amp, rough, seed):
    """1-D mountain profile: summed octaves of smoothed noise."""
    r = np.random.default_rng(seed)
    x = np.arange(n)
    y = np.zeros(n)
    for octave, (period, a) in enumerate([(256, 1.0), (96, .45), (40, .22), (14, .1 * rough)]):
        k = n // period + 2
        pts = r.uniform(-1, 1, k)
        y += a * np.interp(x / period, np.arange(k), pts)
    y = base - amp * y
    return y


# ── BG3 (2bpp): sun (1), far range (2), cloud streaks (3) ───────────────────
def far():
    idx = np.zeros((H, W), np.uint8)
    yy, xx = np.mgrid[0:H, 0:W]
    cx, cy, rad = 256, 92, 42
    idx[((xx - cx) ** 2 + (yy - cy) ** 2) <= rad * rad] = 1
    # distant range: low in the middle so the sun sits on it
    r = ridge(W, 120, 7, 1.0, 3)
    dip = np.exp(-((np.arange(W) - cx) / 70.0) ** 2)
    r = r + 5 * dip
    for x in range(W):
        idx[int(r[x]):HORIZON, x] = 2
    # thin cloud streaks crossing the sky and the sun
    for (y0, x0, ln, th) in [(54, 150, 150, 3), (61, 230, 80, 2), (72, 290, 160, 3),
                             (80, 170, 70, 2), (40, 320, 100, 2), (46, 120, 90, 2), (90, 360, 60, 1)]:
        for x in range(x0, x0 + ln):
            # taper the ends
            f = min(x - x0, x0 + ln - x) / 14
            t = th if f >= 1 else (1 if f > .3 else 0)
            for dy in range(t):
                idx[y0 + dy, x % W] = 3
    idx[HORIZON:] = 0
    save("far", idx)


# ── BG2 (4bpp): mid mountains, banded tones 1..7 (7 = sunlit rim) ───────────
def mid():
    idx = np.zeros((H, W), np.uint8)
    x = np.arange(W)
    # two framing peaks (screen x ~20 and ~220 at rest), low saddle under the sun
    peak = lambda c, wd, ht: ht * np.clip(1 - np.abs(x - c) / wd, 0, 1) ** 1.3
    r = HORIZON - 8 - np.maximum.reduce([peak(150, 110, 36), peak(352, 130, 52), peak(470, 90, 30)])
    r += ridge(W, 0, 7, 2.5, 11)
    for xx in range(W):
        top = int(r[xx])
        for yy in range(top, HORIZON):
            d = yy - top
            if d < 1:
                v = 7  # rim light
            else:
                # body darkens downward, then mist lifts it near the water
                mist = max(0, (yy - (HORIZON - 14)) / 14)
                v = int(round(1 + min(d, 24) / 24 * 2 + mist * 3))
            idx[yy, xx] = v
    save("mid", idx)


# ── BG1 (4bpp): near shore, pines and a pagoda. 1 body, 2 rim, 3 window ─────
def pine(idx, x0, base, h, w):
    for yy in range(base - h, base):
        f = (yy - (base - h)) / h
        # layered skirts: width saw-tooths as we go down
        tier = (f * 5) % 1
        half = int(w * (0.12 + 0.88 * f) * (0.6 + 0.4 * tier) / 2) + 1
        idx[yy, max(0, x0 - half):x0 + half + 1] = 1
    idx[base - 2:base, x0 - 1:x0 + 2] = 1


def pagoda(idx, x0, base):
    y = base
    widths = [30, 26, 22, 18, 14]
    for i, w in enumerate(widths):
        wall_h = 9 - i
        idx[y - wall_h:y, x0 - w // 2 + 3:x0 + w // 2 - 2] = 1
        # lit windows on each storey
        for k in range(-w // 2 + 6, w // 2 - 5, 4):
            idx[y - wall_h + 3:y - wall_h + 5, x0 + k] = 3
        y -= wall_h
        # curved roof with upturned eaves
        for dy in range(4):
            half = w // 2 + 4 - dy * 2
            idx[y - dy - 1, x0 - half:x0 + half + 1] = 1
        idx[y - 2, x0 - w // 2 - 4] = 1
        idx[y - 2, x0 + w // 2 + 4] = 1
        y -= 4
    idx[y - 10:y, x0] = 1  # spire
    idx[y - 6, x0 - 1:x0 + 2] = 1


def boat(idx, x0, base):
    """A little sampan with a standing fisherman and a lit lantern on a pole."""
    for dy, (l, r) in enumerate([(-11, 12), (-12, 13), (-10, 11), (-8, 9)]):
        idx[base - 4 + dy, x0 + l:x0 + r] = 1
    idx[base - 9:base - 4, x0 - 4:x0 + 4] = 1        # cabin
    idx[base - 10, x0 - 5:x0 + 5] = 1
    idx[base - 16:base - 4, x0 + 7] = 1              # fisherman
    idx[base - 18:base - 16, x0 + 6:x0 + 9] = 1
    for k in range(14):                                # rod
        idx[base - 15 - k // 2, x0 + 8 + k] = 1
    idx[base - 20:base - 4, x0 - 9] = 1              # lantern pole
    idx[base - 20, x0 - 9:x0 - 6] = 1
    idx[base - 19:base - 17, x0 - 6] = 3


def near():
    idx = np.zeros((H, W), np.uint8)
    x = np.arange(W)
    # map x = screen x + 128 at rest. Banks frame the left and right of the view.
    shore = np.full(W, float(HORIZON))
    bank = lambda c, wd, ht: ht * np.clip(1 - np.abs(x - c) / wd, 0, 1) ** .7
    shore -= np.maximum.reduce([bank(128, 76, 12), bank(400, 90, 13)])
    for xx in range(W):
        idx[int(shore[xx]):HORIZON, xx] = 1
    for sx, h, w in [(-6, 84, 30), (12, 70, 26), (30, 56, 22), (46, 40, 16),
                     (208, 30, 12), (224, 50, 20), (242, 68, 26), (260, 56, 22), (276, 42, 18)]:
        xx = sx + 128
        pine(idx, xx, int(shore[xx]) + 1, h, w)
    pagoda(idx, 128 + 72, int(shore[128 + 72]) + 1)
    boat(idx, 128 + 168, HORIZON)
    # rim light: the topmost silhouette pixel of every column faces the sun
    body = idx > 0
    top = body & ~np.vstack([np.zeros((1, W), bool), body[:-1]])
    idx[top & (idx == 1)] = 2
    idx[HORIZON:] = 0
    save("near", idx)


# ── sprites: 8x8 cells. cell 0 big lantern, cell 1 small lantern ────────────
def lanterns():
    idx = np.zeros((8, 32), np.uint8)
    big = ["..1111..",
           ".122221.",
           ".233332.",
           ".233332.",
           ".233332.",
           ".122221.",
           "..1111..",
           "...33..."]
    small = ["........",
             "........",
             "...11...",
             "..2332..",
             "..2332..",
             "...11...",
             "........",
             "........"]
    dot = ["........"] * 3 + ["...3...."] + ["........"] * 4
    cross = ["........"] * 2 + ["...1....", "..131...", "...1...."] + ["........"] * 3
    for c, art in enumerate([big, small, dot, cross]):
        for y, row in enumerate(art):
            for x, ch in enumerate(row):
                if ch != ".":
                    idx[y, c * 8 + x] = int(ch)
    save("lantern", idx, 32, 8)


if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    far(); mid(); near(); lanterns()
    print("wrote", sorted(p.name for p in OUT.glob("*.png")))
