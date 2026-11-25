#!/usr/bin/env python3
"""Render a toy to an MP4 through the ppu CLI.

    python3 film.py <toy dir | file.ppu.json> <out.mp4> <seconds> [--scale 3]

Every frame comes from `ppu render` (real engine output, 60 fps, 256x224),
upscaled nearest-neighbour by ffmpeg. The video is silent: the CLI does not
export audio yet. Set PPU to the ppu executable if it is not on PATH.
"""
import argparse, os, shutil, subprocess, sys, tempfile

ap = argparse.ArgumentParser()
ap.add_argument("toy")
ap.add_argument("out")
ap.add_argument("seconds", type=float)
ap.add_argument("--scale", type=int, default=3)
a = ap.parse_args()

ppu = os.environ.get("PPU") or shutil.which("ppu") or sys.exit("ppu not found; install it or set PPU")
shutil.which("ffmpeg") or sys.exit("ffmpeg not found")
frames = round(a.seconds * 60)
with tempfile.TemporaryDirectory() as tmp:
    times = ",".join(f"{f / 60:.6f}" for f in range(frames))
    subprocess.run([ppu, "render", a.toy, "--at", times, "-o", tmp], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", "60", "-start_number", "0",
                    "-i", os.path.join(tmp, "frame-%08d.png"),
                    "-vf", f"scale=iw*{a.scale}:ih*{a.scale}:flags=neighbor",
                    "-c:v", "libx264", "-crf", "16", "-pix_fmt", "yuv420p", "-movflags", "+faststart", a.out],
                   check=True)
print(f"wrote {a.out}: {frames} frames, silent")
