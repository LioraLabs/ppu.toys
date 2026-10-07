"""Skyward art: every PNG procedurally, plus art.lua (palette ranks, craft shadow spans).

  python3 toys/showcase-mode7/build_art.py

world.png   1024x1024 Mode 7 archipelago, <=255 exact colours, <=256 unique 8x8 tiles
ceiling.png 256x64    BG1 cirrus streaks (perspective ceiling, one palette)
bank.png    256x48    BG2 far cumulus bank + low sun (one palette)
craft.png   448x64    OBJ sheet, 7 bank poses, each 2x2 32x32 cells
"""
import os
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "assets")
rng = np.random.default_rng(1991)


def c8(r, g, b):  # snap to BGR555 so the importer keeps exactly these colours
    return (r & 0xF8, g & 0xF8, b & 0xF8)


# ── world palette (named; ranks resolved after sorting like the importer) ──────
W = {
    "sea": c8(32, 64, 104), "trough": c8(24, 48, 88),
    "deep0": c8(40, 72, 112), "deep1": c8(56, 96, 128), "deep2": c8(88, 128, 152), "deep3": c8(144, 168, 176),
    "glint0": c8(104, 120, 136), "glint1": c8(112, 128, 144),
    "shal0": c8(40, 120, 136), "shal1": c8(64, 144, 152), "shal2": c8(96, 168, 168),
    "foam0": c8(232, 224, 208), "foam1": c8(184, 200, 192),
    "sand0": c8(232, 200, 144), "sand1": c8(208, 168, 112), "sand2": c8(160, 128, 88),
    "grass0": c8(152, 168, 72), "grass1": c8(120, 144, 56), "grass2": c8(88, 112, 48), "grass3": c8(184, 184, 96),
    "tree0": c8(40, 64, 40), "tree1": c8(56, 88, 48), "tree2": c8(80, 112, 56), "tree3": c8(136, 152, 72),
    "shade": c8(48, 72, 64),
    "rock0": c8(144, 128, 120), "rock1": c8(96, 88, 88),
    "roof0": c8(200, 96, 64), "roof1": c8(152, 64, 48), "wall": c8(240, 216, 184),
    "sail": c8(248, 240, 224), "hull": c8(120, 72, 48),
}
WORLD_COLOURS = sorted(set(W.values()))
assert len(WORLD_COLOURS) == len(W), "two world colours collide in BGR555"
RANK = {name: WORLD_COLOURS.index(col) + 1 for name, col in W.items()}  # CGRAM index


def periodic_noise(n, freqs, seed):
    """Sum of integer-frequency sinusoids: seamless on an n x n torus."""
    r = np.random.default_rng(seed)
    y, x = np.mgrid[0:n, 0:n] / n * 2 * np.pi
    out = np.zeros((n, n))
    for f in freqs:
        for _ in range(3):
            kx, ky = r.integers(-f, f + 1, 2)
            if kx == 0 and ky == 0:
                continue
            out += np.cos(kx * x + ky * y + r.uniform(0, 2 * np.pi)) / f
    return out / np.abs(out).max()


# ── terrain corner grid: 128x128 torus, levels 0 deep .. 4 forest, neighbours differ <= 1 ──
N = 128
h = periodic_noise(N, [2, 3, 4, 5, 7, 9], 7)
lev = np.digitize(h, [0.12, 0.24, 0.31, 0.44]).astype(int)  # deep, shallow, sand, grass, forest
for _ in range(8):  # relax so every corner pair steps by at most one level
    lo = lev.copy()
    for dy, dx in ((0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)):
        lo = np.minimum(lo, np.roll(np.roll(lev, dy, 0), dx, 1) + 1)
    lev = lo

# periodic 8x8 detail noise (same for every tile, so tile content depends only on its key)
def tile_noise(seed):
    r = np.random.default_rng(seed)
    a = r.uniform(-1, 1, (8, 8))
    k = np.array([[1, 2, 1], [2, 4, 2], [1, 2, 1]]) / 16
    b = sum(k[i, j] * np.roll(np.roll(a, i - 1, 0), j - 1, 1) for i in range(3) for j in range(3))
    return b / np.abs(b).max()

NOISE = tile_noise(3)
NOISE2 = tile_noise(11)
yy, xx = np.mgrid[0:8, 0:8]
uu, vv = (xx + 0.5) / 8, (yy + 0.5) / 8


def dashes(variant):
    """Wave crests: short dashes whose colour slot (0..3) is rotated in Lua, a trough under each."""
    r = np.random.default_rng(500 + variant)
    d = np.full((8, 8), -1)
    for _ in range(3):
        y, x, n = r.integers(0, 7), r.integers(0, 8), r.integers(2, 4)
        slot = r.integers(0, 4)
        for k in range(n):
            d[y, (x + k) % 8] = slot
        d[y + 1, (x + n // 2) % 8] = max(d[y + 1, (x + n // 2) % 8], -2) if d[y + 1, (x + n // 2) % 8] < 0 else d[y + 1, (x + n // 2) % 8]
        if d[y + 1, (x + n // 2) % 8] == -1:
            d[y + 1, (x + n // 2) % 8] = -2
    return d

DASH = [dashes(v) for v in range(8)]


def render_tile(c, variant):
    """c = (tl, tr, bl, br) corner levels -> 8x8 array of colour names."""
    tl, tr, bl, br = c
    hgt = tl * (1 - uu) * (1 - vv) + tr * uu * (1 - vv) + bl * (1 - uu) * vv + br * uu * vv
    hgt = hgt + NOISE * 0.28
    t = np.empty((8, 8), dtype=object)
    vr = np.random.default_rng(1000 + variant * 7 + hash(c) % 997)
    for y in range(8):
        for x in range(8):
            v = hgt[y, x]
            n2 = NOISE2[y, x]
            if v < 0.62:
                dv = DASH[variant][y, x]
                t[y, x] = "sea" if dv == -1 else ("trough" if dv == -2 else f"deep{dv}")
            elif v < 1.5:
                if v > 1.38:
                    t[y, x] = "foam0" if n2 > -0.2 else "foam1"
                else:
                    t[y, x] = "shal2" if v > 1.2 else ("shal1" if (v > 0.95) ^ (n2 > 0.6) else "shal0")
            elif v < 2.5:
                t[y, x] = "sand2" if v < 1.62 else ("sand0" if n2 > 0.1 else "sand1")
            elif v < 3.45:
                t[y, x] = "grass3" if n2 > 0.7 else ("grass0" if n2 > -0.1 else ("grass1" if n2 > -0.6 else "grass2"))
            else:
                t[y, x] = "tree2" if n2 > 0.2 else "tree1"
    if c == (0, 0, 0, 0):  # open sea: sparse sun glints, a boat now and then
        for _ in range(variant % 3):
            gx, gy = vr.integers(0, 8, 2)
            t[gy, gx] = "glint0" if vr.random() < 0.5 else "glint1"
        if variant == 7:
            for x in range(2, 6):
                t[4, x] = "hull"
            t[3, 3], t[2, 3], t[3, 4] = "sail", "sail", "sail"
            t[5, 1], t[5, 0], t[4, 6], t[4, 7] = "foam1", "foam1", "foam0", "foam1"
    if c == (4, 4, 4, 4):  # canopy: round crowns lit from the sun ahead, shadow toward camera
        t[:, :] = "tree1"
        for _ in range(3 + variant % 2):
            cx, cy = vr.integers(1, 7, 2)
            for y in range(8):
                for x in range(8):
                    d = (x - cx) ** 2 + (y - cy) ** 2
                    if d <= 2:
                        t[y, x] = "tree3" if y < cy and d <= 1 else "tree2"
                    elif d <= 4 and y > cy:
                        t[y, x] = "tree0"
    if c == (3, 3, 3, 3) and variant >= 5:  # hamlets: white walls, terracotta roofs
        hx, hy = vr.integers(1, 4, 2)
        for y in range(3):
            for x in range(4):
                t[hy + y, hx + x] = "roof0" if y == 0 else ("roof1" if y == 1 else "wall")
        t[hy + 3, hx:hx + 4] = "shade"
    if c == (3, 3, 3, 3) and variant in (2, 3):  # lone trees with long shadows
        cx, cy = vr.integers(1, 6, 2)
        t[cy, cx], t[cy, cx + 1], t[cy + 1, cx], t[cy + 1, cx + 1] = "tree3", "tree2", "tree1", "tree1"
        t[cy + 2, cx], t[cy + 2, cx + 1] = "shade", "shade"
    if c == (2, 2, 2, 2) and variant == 4:
        t[3, 3], t[3, 4], t[4, 3], t[4, 4], t[5, 4] = "rock0", "rock0", "rock1", "rock1", "sand2"
    return t


VARIANTS = 8
tiles = {}
world = np.zeros((1024, 1024, 3), np.uint8)
for ty in range(N):
    for tx in range(N):
        c = (lev[ty, tx], lev[ty, (tx + 1) % N], lev[(ty + 1) % N, tx], lev[(ty + 1) % N, (tx + 1) % N])
        uniform = len(set(c)) == 1
        var = int(rng.integers(0, VARIANTS)) if uniform else 0
        if c == (0, 0, 0, 0) and var == 7 and rng.random() > 0.15:
            var = 6  # boats are rare
        key = (c, var)
        if key not in tiles:
            tiles[key] = np.array([[W[n] for n in row] for row in render_tile(c, var)], np.uint8)
        world[ty * 8:ty * 8 + 8, tx * 8:tx * 8 + 8] = tiles[key]

uniq = {world[y:y + 8, x:x + 8].tobytes() for y in range(0, 1024, 8) for x in range(0, 1024, 8)}
used = {tuple(p) for p in world.reshape(-1, 3)}
assert len(uniq) <= 256, f"{len(uniq)} unique Mode 7 tiles"
assert used <= set(WORLD_COLOURS)
os.makedirs(OUT, exist_ok=True)
Image.fromarray(world).save(os.path.join(OUT, "world.png"))
# The importer palette is the sorted set of colours actually present.
present = sorted(used)
RANK = {name: present.index(col) + 1 for name, col in W.items() if col in used}
print(f"world: {len(uniq)} tiles, {len(present)} colours, sea {np.mean(lev == 0):.0%}")


# ── sky layers ────────────────────────────────────────────────────────────────
def save_indexed(name, idx, pal):
    """idx: int array, 0 = transparent; pal: list of rgb for 1..n."""
    rgba = np.zeros(idx.shape + (4,), np.uint8)
    for i, col in enumerate(pal, 1):
        rgba[idx == i] = (*col, 255)
    Image.fromarray(rgba, "RGBA").save(os.path.join(OUT, name))


# Ceiling: long clean wisps lit from below by the low sun; 256 wide (wraps with heading), 64 rows repeat.
CEIL = [c8(80, 56, 120), c8(120, 72, 136), c8(176, 96, 136), c8(224, 128, 128), c8(248, 176, 136), c8(255, 216, 176)]
ny, nx = 64, 256
cy, cx = np.mgrid[0:ny, 0:nx]
lvl = np.zeros((ny, nx), int)
for k in range(15):
    y0, x0 = rng.uniform(0, ny), rng.uniform(0, nx)
    L, th = rng.uniform(30, 110), rng.uniform(1.2, 3.2)
    dx = (cx - x0 + nx / 2) % nx - nx / 2
    dy = (cy - y0 + ny / 2) % ny - ny / 2 - np.sin(dx / L * 2.2) * th * 0.8
    e = (dx / L) ** 2 + (dy / th) ** 2
    inside = e < 1
    # violet crown on top, gold belly underneath, hot rim at the very bottom
    band = np.clip(((dy / th) + 1) / 2, 0, 0.999)
    col = np.where(band < 0.35, 1, np.where(band < 0.6, 2, np.where(band < 0.82, 3, np.where(band < 0.93, 4, 5))))
    col = np.where((e > 0.55) & (col > 1), col - 1, col)  # thinner, cooler tips
    lvl = np.where(inside & (lvl == 0), col, lvl)
save_indexed("ceiling.png", lvl, CEIL)

bayer = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]) / 16 - 0.47

# Bank: towering cumulus far away, the sun sitting low behind a gap; 256 wide, 48 tall.
BANK = [c8(104, 72, 128), c8(136, 88, 136), c8(176, 104, 136), c8(216, 128, 128), c8(240, 160, 128),
        c8(255, 200, 152), c8(255, 232, 192), c8(255, 248, 224), c8(255, 216, 120), c8(255, 184, 96)]
ny, nx = 48, 256
by, bx = np.mgrid[0:ny, 0:nx]
SUNX, SUNY = 128, 34
top = np.full(nx, 60.0)
for k in range(18):
    x0 = rng.uniform(0, nx)
    w = rng.uniform(10, 30)
    hh = rng.uniform(8, 26) * (0.45 if abs((x0 - SUNX + 128) % 256 - 128) < 22 else 1)
    d = (np.arange(nx) - x0 + nx / 2) % nx - nx / 2
    top = np.minimum(top, ny - hh * np.sqrt(np.clip(1 - (d / w) ** 2, 0, 1)) - 2 * (np.abs(d) < w))
top = top + np.sin(np.arange(nx) / 3.1) * 0.8
idx = np.zeros((ny, nx), int)
# sun disc and corona behind the clouds
r = np.hypot(bx - SUNX, (by - SUNY) * 1.0)
dsun = r + bayer[by % 4, bx % 4] * 2.4
idx[dsun < 22] = 6
idx[dsun < 16] = 7
idx[dsun < 11] = 8
# cloud body: shaded top to bottom, rim-lit toward the sun
cloud = by >= top[None, :]
depth = by - top[None, :]
near_sun = np.clip(1 - np.abs((bx - SUNX + 128) % 256 - 128) / 70, 0, 1)
shade = 1 + (depth / 6 + bayer[by % 4, bx % 4]).astype(int)
body = np.clip(5 - shade, 1, 4)
rim = (depth < 1.6) & (near_sun > 0.15)
body = np.where(rim, np.where(near_sun > 0.6, 9, 5), body + (near_sun > 0.5))
body = np.where(by >= ny - 2, 3, body)  # haze-bright base hugging the horizon
idx = np.where(cloud, body, idx)
save_indexed("bank.png", idx, BANK)


# ── craft: a tiny software rasteriser, seen from behind and above ─────────────
def quad(a, b, c, d, col):
    return [(a, b, c, col), (a, c, d, col)]

def craft_mesh():
    T = []
    # fuselage: tapered box, nose at +z
    s = [(-1.25, 0.16, 0.10), (-0.4, 0.22, 0.18), (0.6, 0.20, 0.16), (1.5, 0.05, 0.04)]
    for (z0, w0, h0), (z1, w1, h1) in zip(s, s[1:]):
        T += quad((-w0, h0, z0), (w0, h0, z0), (w1, h1, z1), (-w1, h1, z1), "hull")      # top
        T += quad((-w0, -h0, z0), (-w0, h0, z0), (-w1, h1, z1), (-w1, -h1, z1), "hull")  # left
        T += quad((w0, h0, z0), (w0, -h0, z0), (w1, -h1, z1), (w1, h1, z1), "hull")      # right
    T += quad((-0.16, 0.10, -1.25), (0.16, 0.10, -1.25), (0.16, -0.1, -1.25), (-0.16, -0.1, -1.25), "dark")
    # canopy
    T += [((-0.12, 0.2, 0.2), (0.12, 0.2, 0.2), (0.0, 0.34, 0.55), "glass"), ((-0.12, 0.2, 0.2), (0.0, 0.34, 0.55), (0.0, 0.2, 0.95), "glass"),
          ((0.12, 0.2, 0.2), (0.0, 0.2, 0.95), (0.0, 0.34, 0.55), "glass")]
    # swept wings with an orange tip stripe
    for sx in (-1, 1):
        root_f, root_b, tip_f, tip_b = (0.2 * sx, 0.04, 0.45), (0.2 * sx, 0.04, -0.75), (1.75 * sx, 0.12, -0.55), (1.75 * sx, 0.12, -0.85)
        T += quad(root_b, root_f, tip_f, tip_b, "wing") if sx > 0 else quad(root_f, root_b, tip_b, tip_f, "wing")
        T += [((1.5 * sx, 0.11, -0.52), (1.78 * sx, 0.125, -0.55), (1.78 * sx, 0.125, -0.86), "stripe"),
              ((1.5 * sx, 0.11, -0.52), (1.78 * sx, 0.125, -0.86), (1.5 * sx, 0.11, -0.82), "stripe")]
        # tail fin, canted out
        T += [((0.18 * sx, 0.12, -0.55), (0.18 * sx, 0.12, -1.2), (0.6 * sx, 0.66, -1.3), "fin"),
              ((0.18 * sx, 0.12, -0.55), (0.6 * sx, 0.66, -1.3), (0.52 * sx, 0.66, -1.02), "fin")]
        # engine pod with a glowing nozzle at the back
        ex = 0.42 * sx
        T += quad((ex - 0.1, 0.0, -0.95), (ex + 0.1, 0.0, -0.95), (ex + 0.1, 0.0, -0.2), (ex - 0.1, 0.0, -0.2), "dark")
        T += quad((ex - 0.1, 0.0, -0.95), (ex - 0.1, -0.16, -0.95), (ex + 0.1, -0.16, -0.95), (ex + 0.1, 0.0, -0.95), "glow")
    return T

CRAFT_PAL = {  # name -> ramp of colours from shadow to light
    "hull": [c8(56, 64, 88), c8(96, 104, 128), c8(160, 160, 176), c8(224, 216, 216)],
    "wing": [c8(56, 64, 88), c8(96, 104, 128), c8(160, 160, 176), c8(224, 216, 216)],
    "fin": [c8(56, 64, 88), c8(96, 104, 128), c8(160, 160, 176), c8(224, 216, 216)],
    "dark": [c8(32, 32, 48), c8(32, 32, 48), c8(56, 64, 88), c8(56, 64, 88)],
    "stripe": [c8(176, 72, 40), c8(232, 112, 48), c8(255, 160, 72), c8(255, 160, 72)],
    "glass": [c8(40, 88, 104), c8(72, 144, 160), c8(160, 216, 216), c8(160, 216, 216)],
    "glow": [c8(120, 224, 255), c8(120, 224, 255), c8(232, 255, 255), c8(232, 255, 255)],
}
OUTLINE = c8(24, 24, 40)
CRAFT_COLOURS = sorted({c for ramp in CRAFT_PAL.values() for c in ramp} | {OUTLINE})
assert len(CRAFT_COLOURS) <= 15, CRAFT_COLOURS


def render_craft(bank_deg, W_=64, H_=64):
    th = np.radians(bank_deg)
    rot = np.array([[np.cos(th), -np.sin(th), 0], [np.sin(th), np.cos(th), 0], [0, 0, 1]])  # roll about z
    cam_y, cam_z, f = 2.2, -6.0, 92.0
    pitch = np.radians(17)
    light = np.array([0.35, 0.85, 0.4]); light /= np.linalg.norm(light)
    zbuf = np.full((H_, W_), np.inf)
    img = np.zeros((H_, W_, 3), np.uint8)
    mask = np.zeros((H_, W_), bool)
    for a, b, c, col in craft_mesh():
        P = [rot @ np.array(p) for p in (a, b, c)]
        n = np.cross(P[1] - P[0], P[2] - P[0])
        nl = np.linalg.norm(n)
        if nl == 0:
            continue
        n /= nl
        lam = abs(float(n @ light))
        if col == "glow":
            lam = 1.0
        ramp = CRAFT_PAL[col]
        shade = ramp[min(3, int(lam * 4))]
        S = []
        for p in P:
            y, z = p[1] - cam_y, p[2] - cam_z
            y, z = y * np.cos(pitch) + z * np.sin(pitch), -y * np.sin(pitch) + z * np.cos(pitch)
            S.append((W_ / 2 + p[0] / z * f, H_ / 2 - y / z * f - 6, z))
        xs, ys = [s[0] for s in S], [s[1] for s in S]
        x0, x1 = max(0, int(np.floor(min(xs)))), min(W_ - 1, int(np.ceil(max(xs))))
        y0, y1 = max(0, int(np.floor(min(ys)))), min(H_ - 1, int(np.ceil(max(ys))))
        (ax, ay, az), (bx_, by_, bz), (cx_, cy_, cz) = S
        den = (by_ - cy_) * (ax - cx_) + (cx_ - bx_) * (ay - cy_)
        if abs(den) < 1e-9:
            continue
        for py in range(y0, y1 + 1):
            for px in range(x0, x1 + 1):
                X, Y = px + 0.5, py + 0.5
                w0 = ((by_ - cy_) * (X - cx_) + (cx_ - bx_) * (Y - cy_)) / den
                w1 = ((cy_ - ay) * (X - cx_) + (ax - cx_) * (Y - cy_)) / den
                w2 = 1 - w0 - w1
                if min(w0, w1, w2) < -0.02:
                    continue
                z = w0 * az + w1 * bz + w2 * cz
                if z < zbuf[py, px]:
                    zbuf[py, px] = z
                    img[py, px] = shade
                    mask[py, px] = True
    # dark outline around the silhouette
    edge = np.zeros_like(mask)
    for dy, dx in ((0, 1), (1, 0), (0, -1), (-1, 0)):
        edge |= np.roll(np.roll(mask, dy, 0), dx, 1)
    edge &= ~mask
    img[edge] = OUTLINE
    return img, mask | edge


BANKS = [-27, -18, -9, 0, 9, 18, 27]
sheet = np.zeros((64, 64 * len(BANKS), 4), np.uint8)
shadows = []
for i, b in enumerate(BANKS):
    img, m = render_craft(b)
    sheet[:, i * 64:(i + 1) * 64, :3] = img
    sheet[:, i * 64:(i + 1) * 64, 3] = m * 255
Image.fromarray(sheet, "RGBA").save(os.path.join(OUT, "craft.png"))


def shadow_spans(bank_deg):
    """Top-down silhouette of the banked craft, foreshortened onto the sea: (dy, lo, hi) rows."""
    th = np.radians(bank_deg)
    pts = []
    for a, b_, c, _ in craft_mesh():
        for p in (a, b_, c):
            pts.append((p[0] * np.cos(th) - p[1] * np.sin(th), p[2]))
    rows = {}
    tris = [(a, b_, c) for a, b_, c, _ in craft_mesh()]
    for ry in range(-6, 7):
        z = -ry / 4.2  # 4.2 screen rows per unit of depth: grazing view of the sea
        xs = []
        for tri in tris:
            P = [(p[0] * np.cos(th) - p[1] * np.sin(th), p[2]) for p in tri]
            for (x0, z0), (x1, z1) in zip(P, P[1:] + P[:1]):
                if (z0 - z) * (z1 - z) <= 0 and z0 != z1:
                    xs.append(x0 + (x1 - x0) * (z - z0) / (z1 - z0))
        if xs:
            rows[ry] = (round(min(xs) * 15), round(max(xs) * 15))
    return rows


with open(os.path.join(HERE, "art.lua"), "w") as f:
    f.write("-- generated by build_art.py: world palette (CGRAM index = sorted rank + 1) and craft shadows\n")
    f.write("M7PAL = {\n")
    for name in sorted(RANK, key=RANK.get):
        r, g, b = W[name]
        f.write(f"  {name} = {{ i = {RANK[name]}, r = {r}, g = {g}, b = {b} }},\n")
    f.write("}\n")
    f.write("SHADOW = {\n")
    for b in BANKS:
        rows = shadow_spans(b)
        f.write("  { " + ", ".join(f"{{{dy}, {lo}, {hi}}}" for dy, (lo, hi) in sorted(rows.items())) + " },\n")
    f.write("}\n")
print("craft colours", len(CRAFT_COLOURS), "ranks", len(RANK))
