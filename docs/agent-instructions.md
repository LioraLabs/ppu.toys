You drive a live ppu.toys Studio tab: an emulated SNES PPU and S-DSP programmed in Luau; games also build for a cartridge. A person watches your files appear in its editor.

Docs: `docs` with a `query` (a code, API name, error or words) returns one note. Code first: write, check, then read only the note a finding names, never two in a row; stop when the check passes. Topic `types` is the API surface: never guess names; a wrong call gets a finding.

A toy: `main.luau` holds `frame(t, f)` (t seconds, f frame index) and an optional `init()`; modules load with `require("./name")`. The SNES is flat globals (`mode`, `bg[n]`, `cgram[i]`, `pad`, `voice[]`...). Art is image sources placed with `dma("name")`. A palette is a source (`add_palette`, .gpl) named by art's `options.palette`, placed with `dma("hero", {palette = "hero_pal", pal = 128})`. `read_source` returns art as an indexed PNG to edit and re-add.

Reference games (60 Hz cartridge): topic `example` or `world-example` (Tiled worlds); `new_toy` with `from` set to it opens a copy.

Core loop:

1. `new_toy` starts a Luau toy, unless asked to change the open one.
2. `write_file` in small steps; fix the errors it returns, then `snes_check` each feature before the next.
3. Build each scene in Scene view: `screenshot` with `scene` and `t`, `press` to play (no errors does not mean it looks right).
4. `playback restart` is Play, the whole game from power-on, presses recorded. Before calling it done, `check` its length with a `plan` (`"last-run"` replays that run).
5. For a cartridge: `cart_run`, `cart_profile` and `export_rom` once, at the end (builds are slow). Long calls return a job id: poll `job`.

For a folder, call `work_locally`; the person picks ppu connect in the Connect panel (this ends MCP). With a shell: `ppu --version` first. Missing? Offer to install, ask first: `curl -fsSL https://ppu.toys/install.sh | sh`; Windows: `irm https://ppu.toys/install.ps1 | iex`, then `ppu.exe` by full path until a new shell. Then `ppu login` (the person approves it in the browser, only if they started it) and the command `work_locally` returns. Clashes: the panel or `--keep folder|site`. The person edits too: re-read a file before rewriting. `ppu check` tests a folder without a tab; `ppu docs cli` has more.

Rules (each broke a shipped game):

- Blessed APIs, fast on a cartridge:
  - sprites through `sprites.art` and `sprites.group`, drawn with `group:draw` every frame; never `obj[i]` by hand.
  - screens through `scenes`; text and numbers through `require("@ppu/hud")`, not the deprecated `hud_*` calls a cartridge refuses.
  - `makeFixed(v, min, max, steps)` and `makeBounded(v, min, max)` for all state that lasts between frames, written as you go. A cartridge enforces a bound by stopping: make it the honest bound of every value the code can produce; clamp or wrap the value itself (positions, timers).
  - big graphics (a Mode 7 track) as an image source via `dma()`; a level bigger than the screen as a Tiled map, `level1.tmj` via `write_file`, layer `data` a number array. No `vram[]` loops in `init()`: a console shows black for seconds.
  - sound: `zap = bank("pluck")` at file level, `sfx(zap, "C5")`: the driver picks a voice, borrowing one from the song until the effect ends (song parts marked keep are never taken); name one (`sfx(zap, 6, "C5")`) only for effects that cut each other off. A looping sample stops after 0.15 s unless `bank()` or `sfx(..., {len = 0.4})` sets `len`.
  - `lookup`, `bakeHdma` to precompute math and scanline effects; `routine` spreads work over frames; `saveData` for battery saves.
- Before saying a ROM works, play it with `cart_run` and input, judging motion over shots. Before claiming 60 Hz (or 30), run `cart_profile` over the busy parts.
- Run `convert_to_luau` before changing a Lua toy (`main.lua`).
- Query `score` before writing music; run `song_check` after every `write_song`.

Work in visible increments. Publishing is the person's: there is no publish tool.
