"""Builds the fixture project the runner opens: ppu.json, .gpl palettes, indexed PNGs.

Every PNG that names a palette carries PLTE == its .gpl and tRNS on entry 0 of each row, the
way `ppu` writes them, so the live check starts from a clean PLTE rule.
Usage: make_fixtures.py <dir>
"""
import json
import os
import random
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pngio import write_gpl, write_png, write_rgba_png

root = sys.argv[1]
os.makedirs(f"{root}/palettes", exist_ok=True)
os.makedirs(f"{root}/art", exist_ok=True)
x5 = lambda v: (v << 3) | (v >> 2)  # RGB555 -> RGB888, exact
rng = random.Random(7)


def row_palette(rows, row):
    cols = []
    for r in range(rows):
        for e in range(row):
            cols.append((x5(31), 0, x5(31)) if e == 0 else tuple(x5(rng.randrange(32)) for _ in range(3)))
    return cols


sprites = row_palette(4, 16)
for r in range(1, 4):
    sprites[r * 16 + 1] = sprites[1]  # an outline colour shared by every row: its indices must stay distinct
write_gpl(f"{root}/palettes/sprites_pal.gpl", "sprites_pal", sprites, 16)
trns16 = bytes(0 if i % 16 == 0 else 255 for i in range(len(sprites)))

# bg 256x256: tile (tx,ty) draws only in row (tx+ty)%4; three deliberately mixed tiles
W = 256
bg = [0] * (W * W)
for ty in range(32):
    for tx in range(32):
        row = (tx + ty) % 4
        for y in range(8):
            for x in range(8):
                v = row * 16 + 1 + (x + y + tx) % 15
                if (x + y) % 7 == 0:
                    v = row * 16  # its own entry 0
                bg[(ty * 8 + y) * W + tx * 8 + x] = v
MIXED = [(3, 5), (10, 10), (20, 7)]
for tx, ty in MIXED:
    row = (tx + ty) % 4
    bg[(ty * 8 + 3) * W + tx * 8 + 3] = ((row + 1) % 4) * 16 + 5
bg[0 * W + 1] = 1
bg[0 * W + 9] = 17
write_png(f"{root}/art/bg.png", W, W, bg, sprites, trns16)

hero = [32] * (32 * 32)
for y in range(8, 24):
    for x in range(8, 24):
        hero[y * 32 + x] = 33 + (x + y) % 15
write_png(f"{root}/art/hero.png", 32, 32, hero, sprites, trns16)

# m7 1024x1024: own 256-colour palette, uses index 255 and 0
track = [(x5(31), 0, x5(31))] + [tuple(x5(rng.randrange(32)) for _ in range(3)) for _ in range(255)]
for i in (2, 3, 100, 254):
    track[i] = track[1]  # repeated colours in one 256-colour block
write_gpl(f"{root}/palettes/track_pal.gpl", "track_pal", track, 256)
m7 = [0] * (1024 * 1024)
for y in range(1024):
    for x in range(1024):
        m7[y * 1024 + x] = (x // 8 + y // 8) % 255 + 1  # 1..255
for y in range(64):
    for x in range(64):
        m7[y * 1024 + x] = 255
m7[1023 * 1024 + 1023] = 0
write_png(f"{root}/art/track.png", 1024, 1024, m7, track, bytes([0]) + bytes([255] * 255))

# Fix tile rows: four tiles. Tile 1 (row 1) has a stray pixel of row 2 whose colour row 1 also holds (the outline
# colour, entry 1 of every row): fixable. Tile 2 (row 3) has a stray pixel of row 0 entry 7: no colour in row 3.
RF = [0] * (32 * 8)
for t, row in enumerate((0, 1, 3, 2)):
    for y in range(8):
        for x in range(8):
            RF[y * 32 + t * 8 + x] = row * 16 + 1 + (x + y) % 15
RF[3 * 32 + 8 + 3] = 2 * 16 + 1
RF[3 * 32 + 16 + 3] = 7
write_png(f"{root}/art/rowsfix.png", 32, 8, RF, sprites, trns16)

# Mode 7 EXTBG with a 128-colour .gpl: the PNG holds it mirrored to 256, entries 0 and 128 transparent
ext = [(x5(31), 0, x5(31))] + [tuple(x5(rng.randrange(32)) for _ in range(3)) for _ in range(127)]
write_gpl(f"{root}/palettes/ext_pal.gpl", "ext_pal", ext, 256)
write_png(f"{root}/art/world.png", 128, 128, [(x * 3 + y * 5) % 256 for y in range(128) for x in range(128)],
          ext + ext, bytes([0]) + bytes([255] * 127) + bytes([0]) + bytes([255] * 127))

# not listed in ppu.json: an indexed PNG and a full-colour PNG, both opened the normal way
os.makedirs(f"{root}/unlisted", exist_ok=True)
write_png(f"{root}/unlisted/indexed.png", 16, 16, [i % 4 for i in range(256)],
          [(0, 0, 0), (255, 0, 0), (0, 255, 0), (0, 0, 255)], bytes([255] * 4))
write_rgba_png(f"{root}/unlisted/rgba.png", 8, 8, bytes([10, 200, 30, 255] * 64))

# listed without a palette and not SNES art: the import quantizes it, so it opens the normal way
write_png(f"{root}/art/quant.png", 16, 16, [i % 4 for i in range(256)],
          [(0, 0, 0), (255, 0, 0), (0, 255, 0), (0, 0, 255)], bytes([255] * 4))

# full-colour art for Convert to SNES art: one listed in ppu.json (bg, no palette), two the project does not list
def photo(seed):
    r = random.Random(seed)
    cols = [bytes([r.randrange(256) for _ in range(3)] + [255]) for _ in range(5)]
    return b"".join(cols[(x // 4 + y // 4 + seed) % 5] for y in range(16) for x in range(16))


write_rgba_png(f"{root}/art/full.png", 16, 16, photo(1))
write_rgba_png(f"{root}/unlisted/photo.png", 16, 16, photo(2))
write_rgba_png(f"{root}/unlisted/clash.png", 16, 16, photo(3))
write_rgba_png(f"{root}/unlisted/layers.png", 16, 16, photo(4))

json.dump({
    "title": "snesart-test",
    "files": ["main.luau"],
    "sources": [
        {"name": "sprites_pal", "kind": "palette", "file": "palettes/sprites_pal.gpl"},
        {"name": "track_pal", "kind": "palette", "file": "palettes/track_pal.gpl"},
        {"name": "sky", "kind": "bg", "file": "art/bg.png", "options": {"palette": "sprites_pal"}},
        {"name": "hero", "kind": "obj", "file": "art/hero.png",
         "options": {"palette": "sprites_pal", "cell_size": 16}},
        {"name": "track", "kind": "m7", "file": "art/track.png", "options": {"palette": "track_pal"}},
        {"name": "ext_pal", "kind": "palette", "file": "palettes/ext_pal.gpl"},
        {"name": "rowsfix", "kind": "bg", "file": "art/rowsfix.png", "options": {"palette": "sprites_pal"}},
        {"name": "world", "kind": "m7", "file": "art/world.png", "options": {"palette": "ext_pal", "extbg": True}},
        {"name": "quant", "kind": "bg", "file": "art/quant.png"},
        {"name": "full", "kind": "bg", "file": "art/full.png"},
    ],
}, open(f"{root}/ppu.json", "w"), indent=2)
# the row and New SNES art tests write PNGs and ppu.json: they work on their own copy of the project
if os.path.basename(root) != "rows":
    shutil.copytree(root, f"{root}/rows", ignore=shutil.ignore_patterns("rows"))
print(json.dumps({"mixed_tiles": MIXED}))
