# Searchlights: procedural art. python3 build_art.py  (deterministic)
import numpy as np
from PIL import Image
from pathlib import Path
OUT = Path(__file__).parent / "assets"
rng = np.random.default_rng(7)

def c(r, g, b): return (r & 0xF8, g & 0xF8, b & 0xF8, 255)  # exact BGR555 colours, so Lua can find them in CGRAM

# ---- city (BG1). One palette, every colour exact: main.lua looks twinkle/beacon slots up by value.
FAR, FARW = c(24, 20, 48), c(64, 48, 72)
NEAR, EDGE, ROOF = c(8, 8, 24), c(32, 32, 56), c(56, 48, 80)
TW = [c(248, 200, 96), c(240, 168, 72), c(224, 216, 160), c(160, 208, 232), c(248, 224, 128), c(232, 144, 64)]
LENS, BEACON = c(248, 248, 240), c(232, 32, 24)
city = Image.new("RGBA", (256, 224), (0, 0, 0, 0)); px = city.load()
def rect(x0, y0, x1, y1, col):
    for y in range(max(0, y0), min(224, y1)):
        for x in range(max(0, x0), min(256, x1)): px[x, y] = col

# far skyline: dim silhouettes, static dim windows
x = -4
while x < 256:
    w = int(rng.integers(8, 22)); top = int(rng.integers(118, 150))
    rect(x, top, x + w, 224, FAR)
    for wy in range(top + 3, 200, 4):
        for wx in range(x + 2, x + w - 2, 3):
            if rng.random() < 0.18: px[wx % 256, wy] = FARW
    x += w + int(rng.integers(0, 3))

# near buildings: (x0, x1, roof)
near = [(-2, 30, 96), (30, 52, 158), (52, 92, 152), (92, 112, 176), (112, 140, 166), (140, 168, 184),
        (168, 214, 140), (214, 232, 170), (232, 258, 104)]
for bi, (x0, x1, top) in enumerate(near):
    rect(x0, top, x1, 224, NEAR)
    rect(x0, top, x0 + 1, 224, EDGE)            # rim light from the city glow
    rect(x0, top, x1, top + 1, ROOF)
    style = bi % 3
    if style == 0:                               # apartment grid
        for wy in range(top + 5, 214, 6):
            for wx in range(x0 + 3, x1 - 3, 5):
                if rng.random() < 0.3: rect(wx, wy, wx + 2, wy + 3, TW[int(rng.integers(0, 6))])
    elif style == 1:                             # office: lit floors as bands with mullions
        for wy in range(top + 6, 212, 5):
            if rng.random() < 0.35:
                col = TW[int(rng.integers(0, 6))]
                rect(x0 + 3, wy, x1 - 3, wy + 2, col)
                for mx in range(x0 + 5, x1 - 3, 4): rect(mx, wy, mx + 1, wy + 2, NEAR)
    else:                                        # tower: vertical window strips
        for wx in range(x0 + 3, x1 - 3, 6):
            col = TW[int(rng.integers(0, 6))]
            for wy in range(top + 4, 212, 3):
                if rng.random() < 0.3: rect(wx, wy, wx + 2, wy + 2, col)
    # rooftop clutter: a water tank or AC boxes
    w = x1 - x0
    if w > 22 and bi % 2 == 0:
        tx = x0 + w // 3
        rect(tx, top - 7, tx + 7, top - 2, EDGE); rect(tx + 1, top - 8, tx + 6, top - 7, ROOF)
        rect(tx + 1, top - 2, tx + 2, top, EDGE); rect(tx + 5, top - 2, tx + 6, top, EDGE)
    elif w > 18:
        for ax in range(x0 + 3, x1 - 6, 7): rect(ax, top - 3, ax + 4, top, EDGE)
# towers: antenna + beacon
for ax, top in [(14, 96), (245, 104)]:
    rect(ax, top - 22, ax + 1, top, ROOF); px[ax, top - 23] = BEACON
rect(20, 90, 22, 96, ROOF)
# searchlight housings (lens is where the beam is born)
for lx, ly in [(72, 152), (190, 140)]:
    rect(lx - 4, ly - 3, lx + 5, ly, ROOF); rect(lx - 2, ly - 6, lx + 3, ly - 3, EDGE)
    rect(lx - 1, ly - 7, lx + 2, ly - 6, LENS)
# street glow row
rect(0, 218, 256, 224, NEAR)
for sx in range(6, 256, 22): rect(sx, 216, sx + 2, 218, TW[1])
city.save(OUT / "city.png")

# ---- periodic noise helper
def noise(h, w, falloff, seed):
    r = np.random.default_rng(seed)
    f = np.fft.fft2(r.standard_normal((h, w)))
    fy = np.fft.fftfreq(h)[:, None]; fx = np.fft.fftfreq(w)[None, :]
    k = np.sqrt(fx ** 2 + (fy * 1.8) ** 2) + 1e-3          # squash vertically: stratus, not cumulus
    n = np.real(np.fft.ifft2(f / k ** falloff))
    return (n - n.min()) / (n.max() - n.min())

# ---- fog (BG2, sub screen): x-periodic stratus, holes are transparent so COLDATA glows through
n = noise(256, 256, 1.35, 11)[:144]
shades = [c(48, 56, 72), c(64, 72, 96), c(88, 96, 120), c(112, 120, 144), c(144, 152, 176), c(176, 184, 208), c(208, 216, 232)]
fog = np.zeros((256, 256, 4), np.uint8)
lv = np.clip(((n - 0.42) / 0.5 * 7).astype(int), -1, 6)
for i, s in enumerate(shades): fog[:144][lv == i] = s
Image.fromarray(fog).save(OUT / "fog.png")

# ---- rain (BG3, main screen): 128x128 tile of slanted streaks, repeated 2x2 so it dedups
rain = Image.new("RGBA", (128, 128), (0, 0, 0, 0)); rp = rain.load()
RC = [c(104, 120, 152), c(152, 168, 200), c(216, 224, 240)]
for _ in range(70):
    x0, y0, L = rng.random() * 128, rng.random() * 128, int(rng.integers(4, 9)); b = int(rng.integers(0, 3))
    for k in range(L):
        col = RC[min(2, b + (k > L // 2))] if b < 2 else RC[2] if k > 1 else RC[1]
        rp[int(x0 - k * 0.25) % 128, int(y0 + k) % 128] = col
r2 = Image.new("RGBA", (256, 256)); [r2.paste(rain, (x, y)) for x in (0, 128) for y in (0, 128)]
r2.save(OUT / "rain.png")

# ---- blimp (OBJ, two 64x64 cells, nose left)
bl = Image.new("RGBA", (128, 64), (0, 0, 0, 0)); bp = bl.load()
HULL = [c(24, 24, 40), c(40, 44, 64), c(64, 68, 88), c(88, 92, 112), c(112, 116, 136), c(144, 148, 168)]
PANEL, FIN, FIN2 = c(32, 32, 52), c(120, 24, 40), c(72, 16, 32)
GOND, GWIN, NAVG, NAVR = c(32, 32, 48), c(248, 200, 96), c(64, 224, 104), c(232, 40, 32)
cx, cy, rx, ry = 60, 26, 56, 17
for y in range(64):
    for x in range(128):
        u, v = (x - cx) / rx, (y - cy) / ry
        if x > cx: u = (x - cx) / (rx + 4)              # slightly fuller tail
        if u * u + v * v <= 1:
            light = 0.55 - 0.75 * v - 0.2 * u + 0.25 * (1 - u * u)   # lit from above-left
            bp[x, y] = HULL[int(np.clip(light * 5, 0, 5))]
            if (x - 6) % 14 == 0 and abs(v) < 0.92: bp[x, y] = PANEL
for x in range(96, 124):                                             # tail fins grow out of the hull
    reach = (x - 96) * 0.5
    for y in range(int(cy - 12 - reach), int(cy - 6)):
        if 0 <= y < 64 and bp[x, y][3] == 0: bp[x, y] = FIN if (x + y) % 6 else FIN2
    for y in range(int(cy + 7), int(cy + 13 + reach)):
        if 0 <= y < 64 and bp[x, y][3] == 0: bp[x, y] = FIN if (x + y) % 6 else FIN2
for y in range(42, 49):                                              # gondola
    for x in range(44 + (48 - y) // 2, 76 - (48 - y) // 2): bp[x, y] = GOND
for x in range(47, 73, 4): bp[x, 44] = GWIN; bp[x + 1, 44] = GWIN
bp[8, 30] = NAVR; bp[60, 9] = NAVG
for x in range(30, 92): bp[x, 22] = HULL[5] if x % 3 else HULL[4]      # name band highlight
bl.save(OUT / "blimp.png")
print("art ok")
