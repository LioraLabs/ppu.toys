# Examples

Five finished toys, one hardware idea each, made for the ppu.toys intro video.
Each folder is a complete project: `build_art.py` generates every PNG (and any
generated Lua) deterministically, so the art can be changed as code. Each
README says what is on screen and which registers do the work;
[../recipes.md](../recipes.md) explains the techniques.

| Folder | Idea | Length |
|---|---|---|
| `mode7` | Mode 7 floor under a Mode 1 sky, per-line haze, window-shaped shadow | 12 s loop |
| `hdma` | a lake whose reflection is per-line scroll, not art | 12 s loop |
| `colour-math` | light shafts and caustics added through the sub screen | 12 s loop |
| `windows` | searchlight beams as per-line windows; fog and an airship exist only in the light | 12 s loop |
| `finale` | 128 sprites form a logo on a `score{}` downbeat | 22 s, one pass, with music |

Rebuild, check, preview and pack any of them (run from this folder; needs
Python with numpy and Pillow, and ffmpeg for the MP4):

```sh
python3 mode7/build_art.py
ppu check mode7 --duration 12 --loop 12 --seek 0,3,6,9
python3 ../scripts/contact.py mode7 mode7-sheet.png 0 2 4 6 8 10
python3 ../scripts/film.py mode7 mode7.mp4 12
ppu pack mode7 -o mode7.ppu.json
```

The finale's music only plays in the Studio; `film.py` is silent.
