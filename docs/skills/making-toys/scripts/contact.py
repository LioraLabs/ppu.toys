#!/usr/bin/env python3
"""Render a toy at several times and tile the frames into one labelled sheet.

    python3 contact.py <toy dir | file.ppu.json> <out.png> <t> <t> ... [--cols 4]

The sheet is for review: look at it, critique it, fix, render again.
Needs Pillow. Set PPU to the ppu executable if it is not on PATH.
"""
import argparse, os, shutil, subprocess, sys, tempfile
from PIL import Image, ImageDraw

ap = argparse.ArgumentParser()
ap.add_argument("toy")
ap.add_argument("out")
ap.add_argument("times", nargs="+", type=float)
ap.add_argument("--cols", type=int, default=4)
a = ap.parse_args()

ppu = os.environ.get("PPU") or shutil.which("ppu") or sys.exit("ppu not found; install it or set PPU")
with tempfile.TemporaryDirectory() as tmp:
    subprocess.run([ppu, "render", a.toy, "--at", ",".join(map(str, a.times)), "-o", tmp],
                   check=True, stdout=subprocess.DEVNULL)
    shots = [(round(t * 60), t) for t in a.times]
    rows = (len(shots) + a.cols - 1) // a.cols
    sheet = Image.new("RGB", (a.cols * 260, rows * 240), (40, 40, 40))
    draw = ImageDraw.Draw(sheet)
    for i, (f, t) in enumerate(shots):
        x, y = i % a.cols * 260 + 2, i // a.cols * 240 + 14
        sheet.paste(Image.open(os.path.join(tmp, f"frame-{f:08d}.png")).convert("RGB"), (x, y))
        draw.text((x, y - 12), f"t={t:g}s", fill=(255, 255, 0))
    sheet.save(a.out)
print(f"wrote {a.out}: {len(shots)} frames")
