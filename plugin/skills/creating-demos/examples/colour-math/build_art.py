"""Undersea: procedural art. Writes assets/{reef,ruins,fish,bubbles}.png and light.lua.

python3 build_art.py   (deterministic; seeded)
"""
import math
import os
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
rng = np.random.default_rng(7)
BAYER = (np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]) + 0.5) / 16


def dith(v, n, x, y):
    """Ordered-dither continuous v (0..1) to integer level 0..n-1 at pixel (x, y)."""
    s = np.clip(v, 0, 1) * (n - 1)
    base = np.floor(s)
    return (base + ((s - base) > BAYER[y % 4, x % 4])).astype(int).clip(0, n - 1)


def grid(w, h):
    y, x = np.mgrid[0:h, 0:w]
    return x, y


def save(img_idx, pal, name):
    """img_idx: int array (-1 transparent); pal: list of RGB."""
    h, w = img_idx.shape
    out = np.zeros((h, w, 4), np.uint8)
    m = img_idx >= 0
    p = np.array(pal, np.uint8)
    out[m, :3] = p[img_idx[m]]
    out[m, 3] = 255
    Image.fromarray(out, "RGBA").save(os.path.join(HERE, "assets", name))


def noise2(w, h, scale, seed, wrap_x=True):
    """Smooth value noise, horizontally tileable."""
    r = np.random.default_rng(seed)
    gw, gh = max(1, w // scale), h // scale + 2
    g = r.random((gh, gw))
    x, y = grid(w, h)
    fx, fy = x / scale, y / scale
    x0, y0 = np.floor(fx).astype(int), np.floor(fy).astype(int)
    tx, ty = fx - x0, fy - y0
    tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
    a = g[y0, x0 % gw]; b = g[y0, (x0 + 1) % gw]
    c = g[y0 + 1, x0 % gw]; d = g[y0 + 1, (x0 + 1) % gw]
    return (a * (1 - tx) + b * tx) * (1 - ty) + (c * (1 - tx) + d * tx) * ty


# ---------------------------------------------------------------- ruins (BG2)
# Both BG layers are one 256-px screen wide (32x32 maps) and tile seamlessly:
# the camera only sways, so the composition is authored for the exact frame.
W = 256


def ruins():
    H = 256
    x, y = grid(W, H)
    shade = np.full((H, W), np.nan)
    moss = np.zeros((H, W), bool)

    def fill(mask, val):
        shade[mask] = val[mask] if isinstance(val, np.ndarray) else val

    tex = noise2(W, H, 8, 1) * 0.6 + noise2(W, H, 4, 2) * 0.4
    # obelisk
    m = (np.abs(x - 16) < 7 - (196 - y) * 0.012) & (y >= 66) & (y < 196)
    m |= (np.abs(x - 16) < (y - 54) * 0.6) & (y >= 54) & (y < 68)
    fill(m, 0.62 - 0.07 * (x - 16) + 0.06 * tex)
    # arch gate
    for px in (40, 94):
        m = (x >= px) & (x < px + 18) & (y >= 108) & (y < 196)
        fill(m, 0.66 - 0.4 * (x - px) / 18 + 0.06 * tex - 0.18 * ((y % 14) == 0))
    cx, cy = 76, 110
    r = np.hypot(x - cx, (y - cy) * 1.1)
    ang = np.arctan2(y - cy, x - cx)
    m = (r >= 26) & (r < 40) & (y < cy + 2) & ~((x > 104) & (y < 86))
    fill(m, 0.5 - 0.22 * np.cos(ang + 0.7) + 0.2 * (r > 37) + 0.05 * tex
         - 0.3 * (np.abs(np.sin(ang * 5)) < 0.07))
    moss |= m & (r > 37) & (tex > 0.5)
    # temple steps
    for i, (y0, x0, x1) in enumerate([(184, 128, 256), (174, 134, 256), (164, 140, 256)]):
        m = (y >= y0) & (y < y0 + 10) & (x >= x0) & (x < x1)
        fill(m, 0.42 - 0.05 * i + 0.28 * (y < y0 + 2) + 0.05 * tex)
        moss |= m & (y < y0 + 2) & (tex > 0.55)
    # cella wall behind the columns
    m = (x >= 142) & (x < 250) & (y >= 76) & (y < 164)
    fill(m, 0.1 + 0.04 * tex + 0.06 * (((y - 76) % 12) == 0))
    # columns
    for cx, top in [(150, 64), (176, 64), (202, 112), (228, 64), (252, 96)]:
        u = (x - cx) / 8.0
        flute = 0.07 * np.cos((x - cx) * 1.6)
        if top == 64:
            m = (np.abs(u) <= 1) & (y >= top) & (y < 164)
            fill(m, 0.66 - 0.36 * u + flute - 0.15 * np.abs(u) ** 4 + 0.04 * tex)
            mc = (np.abs(x - cx) <= 11) & (y >= 58) & (y < 66)
            fill(mc, 0.74 - 0.28 * (y >= 63) - 0.02 * (x - cx))
            mb = (np.abs(x - cx) <= 10) & (y >= 160) & (y < 164)
            fill(mb, 0.6 - 0.02 * (x - cx))
        else:
            jag = top + 3 * np.sin(x * 1.3) + 2
            m = (np.abs(u) <= 1) & (y >= jag) & (y < 164)
            fill(m, 0.66 - 0.36 * u + flute + 0.12 * (y < jag + 3) + 0.04 * tex)
    # entablature, broken at the right
    m = (x >= 138) & (x < 214 + 5 * np.sin(y * 0.9)) & (y >= 44) & (y < 58)
    fill(m, 0.55 + 0.25 * (y < 46) - 0.18 * (y > 54) + 0.05 * tex)
    moss |= m & (y < 47) & (tex > 0.45)
    # pediment, right half fallen
    m = (y >= 22) & (y < 44) & (np.abs(x - 176) < (y - 22) * 1.75) & (x < 196 + 4 * np.sin(y * 1.7))
    fill(m, 0.48 + 0.24 * ((x - 176) < 0) + 0.18 * (np.abs(np.abs(x - 176) - (y - 22) * 1.75) < 1.5) + 0.04 * tex)
    # fallen drums + rubble
    for cx, cy, rr in [(118, 186, 9), (104, 190, 6)]:
        d = ((x - cx) / rr) ** 2 + ((y - cy) / (rr * 0.8)) ** 2
        fill(d <= 1, 0.66 - 0.4 * ((x - cx) / rr) - 0.2 * ((y - cy) / rr) + 0.05 * tex)
    top = 186 + 4 * np.sin(x * 2 * math.pi / 256 * 3) + 3 * np.sin(x * 2 * math.pi / 256 * 7 + 1)
    fill((y >= top) & (y < 200), 0.28 + 0.12 * tex - (y - top) / 40)
    # cracks
    r2 = np.random.default_rng(3)
    for _ in range(26):
        px, py = int(r2.integers(0, W)), int(r2.integers(40, 180))
        for _ in range(int(r2.integers(4, 12))):
            if not np.isnan(shade[py, px]) and shade[py, px] > 0.25:
                shade[py, px] = 0.06
            px = (px + int(r2.integers(-1, 2))) % W; py = min(H - 1, py + 1)
    pal = [(10, 30, 48), (16, 40, 60), (22, 52, 72), (30, 64, 86), (40, 78, 100), (54, 96, 116),
           (72, 118, 132), (40, 84, 76), (58, 106, 88)]
    idx = np.full((H, W), -1)
    m = ~np.isnan(shade)
    lev = dith(np.nan_to_num(shade), 7, x, y)
    idx[m] = lev[m]
    mm = m & moss
    idx[mm] = 7 + (lev[mm] > 3)
    idx[200:] = -1
    save(idx, pal, "ruins.png")


# ---------------------------------------------------------------- reef (BG1)
def reef():
    H = 256
    x, y = grid(W, H)
    idx = np.full((H, W), -1)
    SAND = [0, 1, 2, 3, 4]; ROCK = [5, 6, 7, 8]; PINK = [9, 10, 11]; ORNG = [12, 13]; PURP = [14, 15]
    KELP = [16, 17, 18, 19, 20]
    pal = [(176, 168, 128), (146, 140, 108), (116, 112, 92), (86, 86, 78), (58, 62, 64),
           (104, 112, 116), (74, 84, 94), (50, 60, 74), (30, 38, 54),
           (240, 124, 132), (196, 78, 106), (138, 48, 88),
           (246, 168, 86), (194, 104, 58),
           (160, 98, 178), (104, 62, 134),
           (176, 186, 80), (128, 152, 58), (88, 118, 48), (56, 86, 44), (34, 58, 40)]

    def put(mask, ramp, v):
        lev = dith(v, len(ramp), x, y)
        idx[mask] = np.array(ramp)[lev[mask]]

    tex = noise2(W, H, 8, 11)
    P = 2 * math.pi / W
    sand_top = 180 + 4 * np.sin(x * P) + 3 * np.sin(x * P * 3 + 1)
    # kelp (behind coral and rocks). (x, top, phase, lean)
    for kx, ktop, ph, lean in [(10, 18, 0.0, 1), (28, 64, 1.3, -1), (128, 120, 2.1, 1),
                               (246, 30, 2.9, -1)]:
        base = 184
        for yy in range(ktop, base + 4):
            s = (base - yy) / (base - ktop)
            cx = (kx + lean * 6 * math.sin(s * 2.6 + ph) * s) % W
            for off in (0, -W, W):
                m = (np.abs(x - cx - off) < 1.6) & (y == yy)
                put(m, KELP, 0.55 - 0.3 * (x - cx - off) / 1.6 + 0 * x)
            if (yy - ktop) % 8 == 3 and yy < base - 8:
                side = 1 if ((yy - ktop) // 8) % 2 else -1
                L = 8 + 8 * min(1, s * 1.6)
                for k in range(int(L)):
                    t = k / L
                    bx = cx + side * k
                    by = yy - k * 0.6 - 2.2 * math.sin(t * math.pi)
                    th = 2.4 * math.sin(t * math.pi * 0.9 + 0.2) + 0.4
                    for off in (0, -W, W):
                        m = (np.abs(x - bx - off) < 0.6) & (np.abs(y - by) < th)
                        put(m, KELP, 0.9 - 0.35 * t - 0.3 * (y - by) / th + 0 * x)
    # sand: ripples, lit crest
    m = y >= sand_top
    ripple = np.sin(x * 0.19 + 3 * np.sin(y * 0.11 + x * 0.02) + y * 0.5)
    v = 0.95 - (y - sand_top) / 110 + 0.07 * ripple
    put(m, SAND[::-1], v)
    # rocks, partly buried
    for cx, r0, sq in [(0, 20, 0.8), (54, 12, 0.75), (98, 9, 0.8), (150, 15, 0.7), (190, 10, 0.8),
                       (240, 16, 0.75)]:
        cy = 184
        for off in (0, -W, W):
            dx = (x - cx - off) / r0; dy = (y - cy) / (r0 * sq)
            d = dx * dx + dy * dy
            m = (d <= 1) & (y < cy + 4)
            nz = np.sqrt(np.clip(1 - d, 0, 1))
            light = (-dx * 0.5 - dy * 0.6 + nz * 0.55) * 0.62 + 0.24 + 0.1 * (tex - 0.5)
            put(m, ROCK[::-1], light)
    # brain coral domes
    for cx, rr in [(76, 11), (170, 13)]:
        cy = 182
        dx = (x - cx) / rr; dy = (y - cy) / (rr * 0.75)
        d = dx * dx + dy * dy
        m = (d <= 1) & (y <= cy + 2)
        groove = np.sin((x - cx) * 0.9 + 3 * np.sin((y - cy) * 0.6)) > 0.6
        put(m, ORNG[::-1], 0.78 - dx * 0.35 - dy * 0.3 - 0.45 * groove)
    # branching coral
    r = np.random.default_rng(5)

    def branch(x0, y0, ang, ln, th, depth):
        for k in range(int(ln)):
            px = x0 + math.cos(ang) * k; py = y0 - math.sin(ang) * k
            m = ((x - px) ** 2 + (y - py) ** 2) <= th * th
            put(m, PINK[::-1], 0.62 - (x - px) / (th + 1) * 0.4 + 0 * x)
        ex, ey = x0 + math.cos(ang) * ln, y0 - math.sin(ang) * ln
        if depth > 0:
            for da in (-0.45, 0.42):
                branch(ex, ey, ang + da + r.uniform(-0.15, 0.15), ln * 0.72, max(0.8, th * 0.7), depth - 1)
    for cx, h in ((38, 14), (136, 11), (216, 16)):
        branch(cx, 186, math.pi / 2, h, 2.4, 3)
    # sea fans
    for cx, cy, R in [(112, 184, 22), (196, 182, 26)]:
        for a in np.linspace(0.35, math.pi - 0.35, 12):
            for k in range(R):
                px = cx + math.cos(a) * k; py = cy - math.sin(a) * k * 1.05
                m = (np.abs(x - px) < 0.7) & (np.abs(y - py) < 0.7)
                put(m, PURP, 0.25 + k / (R * 1.6) + 0 * x)
        for rr in (R * 0.3, R * 0.55, R * 0.8, R - 1):
            d = np.hypot(x - cx, (y - cy) / 1.05)
            m = (np.abs(d - rr) < 0.55) & (y < cy) & (np.abs(np.arctan2(cy - y, x - cx) - math.pi / 2) < 1.2)
            put(m, PURP, 0.6 + 0 * x)
    idx[224:] = -1
    save(idx, pal, "reef.png")


# ---------------------------------------------------------------- sprites
def fish():
    # 4 frames, 16x16 cells, facing left
    pal = [(24, 40, 66), (48, 76, 108), (128, 160, 182), (206, 226, 236), (8, 10, 22)]
    img = np.full((16, 64), -1)
    for fr in range(4):
        sw = math.sin(fr / 4 * 2 * math.pi)
        for yy in range(16):
            for xx in range(16):
                u = (xx - 7) / 6.0; v = (yy - 8) / 3.2
                body = u * u + v * v <= 1 and xx >= 2
                tx = xx - 12; tail = 0 <= tx <= 3 and abs(yy - 8 - sw * 1.2 * tx / 3) <= tx * 0.9 + 0.3
                fin = 5 <= xx <= 8 and yy == 4 + (xx - 5) // 2 and False
                if body or tail:
                    c = 1
                    if body and v > 0.2: c = 2
                    if body and v < -0.55: c = 0
                    if tail: c = 0
                    img[yy, fr * 16 + xx] = c
            if True:
                pass
        img[7, fr * 16 + 3] = 4
        img[9, fr * 16 + 4:fr * 16 + 10] = 3
    save(img, pal, "fish.png")


def bubbles():
    # 8x8 cells: big, mid, small, tiny bubble (additive), then a 2-frame distant fish
    pal = [(40, 80, 96), (110, 170, 190), (210, 245, 250), (60, 90, 120), (34, 64, 92)]
    art = [
        ["..###..", ".#...#.", "#.H...#", "#.....#", "#.....#", ".#...#.", "..###.."],
        ["", ".###.", "#.H.#", "#...#", "#...#", ".###."],
        ["", "", ".#.", "#H#", ".#."],
        ["", "", "", "..H"],
    ]
    img = np.full((8, 48), -1)
    for i, rows in enumerate(art):
        for yy, row in enumerate(rows):
            for xx, ch in enumerate(row):
                if ch != ".":
                    img[yy, i * 8 + xx] = {"#": 1, "H": 2}[ch]
    for fr in range(2):
        o = 32 + fr * 8
        for xx in range(1, 7):
            for yy in range(3, 6):
                if abs(yy - 4) <= (1 if 2 <= xx <= 4 else 0):
                    img[yy, o + xx] = 4
        img[3 + fr * 2, o + 6] = 4; img[4, o + 6] = 4
        img[4, o + 2] = 3
    save(img, pal, "bubbles.png")


# ---------------------------------------------------------------- light layer (BG3, Lua-generated 2bpp)
def light():
    """BG3: 32x32 map at 0x0800, 2bpp chars at 0x1000.
    rows 0-3: surface underside (pal 4); row 4: ray row (pal 0-3 per ray); rows 4-9: ray depth variants; rows 12-17: caustics (pal 5)."""
    W = 256
    canvas = np.zeros((256, W), int)  # level 0..3
    palmap = np.zeros((256, W), int)
    x, y = grid(W, 256)
    # surface: squashed tileable voronoi edges
    def voronoi_edges(h, cw, ch, seed, period=W):
        r = np.random.default_rng(seed)
        nx, ny = period // cw, h // ch + 2
        pts = []
        for j in range(-1, ny):
            for i in range(nx):
                pts.append(((i + r.random()) * cw, (j + r.random()) * ch))
        pts = np.array(pts)
        xs, ys = np.mgrid[0:h, 0:W][1], np.mgrid[0:h, 0:W][0]
        d1 = np.full((h, W), 1e9); d2 = np.full((h, W), 1e9)
        for px, py in pts:
            for off in (-period, 0, period):
                d = np.hypot((xs - px - off) / cw, (ys - py) / ch)
                d2 = np.where(d < d1, d1, np.minimum(d2, d)); d1 = np.minimum(d1, d)
        return d2 - d1  # 0 at edges
    e = voronoi_edges(32, 32, 7, 21)
    sx, sy = grid(W, 32)
    v = np.clip(1.0 - e * 2.6, 0, 1) * 0.8 + 0.2 + 0.15 * (sy < 3)
    v = v * np.clip((19 - sy) / 10, 0, 1)
    lv = np.clip(np.floor(v * 3.99), 0, 3).astype(int)
    canvas[0:32] = np.where(sy > 15, 0, lv)
    palmap[0:32] = 4
    # rays (vertical; slant comes from per-line HDMA skew)
    NV = 6
    rays = [(14, 12, 0), (54, 26, 1), (96, 8, 2), (142, 34, 3), (196, 14, 0), (230, 18, 2)]
    used = set()
    for cx, w, g in rays:
        for xx in range(int(cx - w / 2 - 3), int(cx + w / 2 + 4)):
            u = abs(xx + 0.5 - cx) / (w / 2 + 2)
            val = np.clip(1.0 - u ** 1.6, 0, 1)
            # NV depth variants, one tile row each: HDMA picks a row per line
            for vi in range(NV):
                for yy in range(32 + vi * 8, 40 + vi * 8):
                    lv = int(dith(np.array([[val * (1 - vi * 0.16)]]), 4, np.array([[xx]]), np.array([[yy]]))[0, 0])
                    canvas[yy, xx % W] = lv
                    palmap[yy, xx % W] = g
            used.add((xx % W) // 8)
    # caustics: two overlapping nets
    e1 = voronoi_edges(48, 16, 8, 31)
    e2 = voronoi_edges(48, 23, 11, 32)
    cxg, cyg = grid(W, 48)
    v = np.clip(1 - e1 * 5.0, 0, 1) ** 1.6 * 0.8 + np.clip(1 - e2 * 6.0, 0, 1) ** 2 * 0.45
    v = np.clip(v, 0, 1) * np.clip((cyg + 2) / 10, 0, 1)
    canvas[96:144] = np.where(v > 0.15, dith(v, 4, cxg, cyg + 96), 0)
    palmap[96:144] = 5
    # pack tiles
    tiles = {(0,) * 64: 0}
    tile_list = [[0] * 8]
    mapw = []
    for ty in range(32):
        for tx in range(32):
            blk = canvas[ty * 8:ty * 8 + 8, tx * 8:tx * 8 + 8]
            pals = set(palmap[ty * 8:ty * 8 + 8, tx * 8:tx * 8 + 8][blk > 0].tolist())
            assert len(pals) <= 1, (tx, ty, pals)
            pal = pals.pop() if pals else 0
            key = tuple(blk.flatten().tolist())
            if key not in tiles:
                words = []
                for row in blk:
                    lo = sum(((int(p) & 1) << (7 - i)) for i, p in enumerate(row))
                    hi = sum((((int(p) >> 1) & 1) << (7 - i)) for i, p in enumerate(row))
                    words.append(lo | (hi << 8))
                tiles[key] = len(tile_list); tile_list.append(words)
            mapw.append(tiles[key] | (pal << 10))
    assert len(tile_list) * 8 <= 0x1000, len(tile_list)
    with open(os.path.join(HERE, "light.lua"), "w") as f:
        f.write("-- light.lua: generated by build_art.py. BG3 light layer: %d 2bpp tiles at 0x1000, map at 0x0800.\n" % len(tile_list))
        flat = [w for t in tile_list for w in t]
        for i in range(0, len(flat), 256):
            f.write("vr(0x%04x,{%s})\n" % (0x1000 + i, ",".join(str(w) for w in flat[i:i + 256])))
        for i in range(0, 1024, 256):
            f.write("vr(0x%04x,{%s})\n" % (0x0800 + i, ",".join(str(w) for w in mapw[i:i + 256])))
    print("light tiles", len(tile_list))


os.makedirs(os.path.join(HERE, "assets"), exist_ok=True)
ruins(); reef(); fish(); bubbles(); light()
