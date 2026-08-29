# ppu.toys web studio

React + Vite authoring workspace over the WASM-compiled `ppu-core` engine. The
architecture covers the studio layout, live authoring loop, sketch model,
multi-file semantics, and inspector tab set.

## Layout

`web/src/studio/Studio.tsx` composes four regions:

- `Toolbar.tsx` — top bar, `--toolbar-h: 50px`.
- `ActivityRail.tsx` — left icon rail, `--rail-w: 54px` (Files toggles the
  sketch `LibraryPanel`; the other items are selection state only).
- `EditorPane.tsx` — the flexible-width code editor (tabs + CodeMirror).
- `RightColumn.tsx` — fixed `--right-w: 600px` column stacking the output
  canvas (`output/OutputCanvas.tsx`) over the inspector (`inspector/Inspector.tsx`).

These dimensions are CSS custom properties defined once in
`web/src/styles/tokens.css`, which also defines the full dark (default,
`:root`) and light (`[data-theme="light"]`) palettes; the toolbar's theme
button flips the attribute via `theme.ts`.

## Authoring loop

Editing is live with error grace, not edit-then-recompile:

- Keystrokes push the whole multi-file program through a debounced pusher
  (`web/src/studio/editor/sourcePush.ts`, `SOURCE_PUSH_MS = 200`) into
  `transport.setSources`.
- A failed compile does **not** touch the running program: `LuaEngine::set_sources`
  (`crates/ppu-core/src/lua.rs`) builds the new VM and executes chunks into a
  local variable, only swapping it onto `self.lua`/`self.frame_fn` once every
  chunk and `frame`/`init` resolution succeeds — a syntax or runtime error
  during chunk load leaves the previous program (and its `frame_fn`) in place,
  so the last good frame keeps rendering (a runtime error thrown later, inside
  `init()`/`frame()`, surfaces on the new program instead). The error is surfaced as a per-file inline
  diagnostic (`web/src/studio/editor/diagnostics.ts`: `routeErrorsByFile` maps
  `{file, line, message}` onto the owning tab, falling back to the active file
  when unattributed).
- A successful recompile builds a fresh Lua VM and re-executes all chunks
  (fresh globals each time — `Lua::core()` is rebuilt on every `set_sources`
  call), but the clock is untouched: `Transport.setSources` re-renders at the
  _current_ `t`/`f` (`web/src/studio/transport/transport.ts`: "recompile never
  resets t/f"). Pokes (see below) live in the ordinary `pokes.lua` file, so a
  recompile carries them along like any other edit — there is no separate
  override layer to invalidate.
- ▶ Run (`Toolbar.tsx` → `transport.restart()`) is the deterministic reset: it
  re-pushes the last sources into a fresh program and rewinds the clock to
  `t=0, f=0` before resuming playback. It does **not** touch `pokes.lua` —
  pokes are a file, not session state, so Run and a page reload both leave
  them in place; only poking/un-poking/clear-all edit the file.

## Sketch model

A `Sketch` (`web/src/studio/sketches/sketchStore.ts`) is
`{ id, name, createdAt, updatedAt, files: {name, source}[], assets: {name, png}[], forkedFrom? }`,
persisted in IndexedDB (`ppu-toys` DB, `sketches` store). `files` is ordered —
that order is chunk execution order. `forkedFrom` records the demo id a sketch
was lazily forked from, so restoring it can re-run that demo's procedural
assets instead of storing copies.

- Autosave is debounced (`web/src/studio/sketches/openSketch.ts`,
  `AUTOSAVE_MS = 800`) after any edit; the toolbar shows an unsaved dot
  (`Toolbar.tsx`'s `dirty` prop) while a flush is pending.
- The library panel (`web/src/studio/sketches/LibraryPanel.tsx`), opened from
  the Files rail item, lists bundled demos (read-only) and stored sketches
  with New / Rename / Duplicate / Delete actions.
- Demos are read-only templates. The first real edit — `editFile` with changed
  content, `addFile`, `renameFile`, `deleteFile`, `moveFile`, or `addAsset` —
  forks the demo into a new in-memory sketch named `"<demo label> (copy)"`
  (`web/src/studio/sketches/openSketch.ts`: `forkFromDemo`). A no-op write-back
  of unchanged content does not fork.

## Multi-file semantics

Multi-file sketches follow PICO-8 scoping: `LuaEngine::set_sources`
(`crates/ppu-core/src/lua.rs`) loads each `(name, source)` pair **in list
order** as a chunk named after its file, executing all of them into one shared
global environment. `frame` (and `init`, run once per successful compile if
present) are resolved only _after_ every chunk has run, so any file can
reference globals defined in another file regardless of naming — `main.lua` is
a UI convention, not a special-cased entry point. Errors carry
`{file, line?, message}` attributed to the chunk that raised them.

Tab order in the editor IS execution order: `openSketchStore.moveFile`
(drag-reorder) directly reorders the `files` array that gets pushed to
`setSources`.

The flagship example is `dusk-parallax` (`web/src/studio/demos/demos.ts`),
shipped as `main.lua` (`frame()`, references `SPEED` and `dusk_palette`) +
`palette.lua` (`SPEED` and `dusk_palette` definitions). A Rust golden test
(`crates/ppu-core/tests/golden_demos.rs`:
`dusk_parallax_multi_file_matches_single_file_concat`) proves the two-file
split renders byte-identical to the single-file concatenation
(`dusk_concat()` = `main.lua` source + `"\n"` + `palette.lua` source).

## The Lua DSL: two tiers

The authoring surface is two-tier by design:

- **Friendly namespaces** — `bg[n]`/`obj`/`m7` plus the compositing trio
  `screen` (TM/TS main/sub layer designation), `color` (CGWSEL/CGADSUB/COLDATA
  colour math) and `win` (WH0-3, W12SEL/W34SEL/WOBJSEL, WBGLOG/WOBJLOG,
  TMW/TSW windowing) — are the **primary** authoring and codegen surface.
  Fields are per-bit-group (`screen.main.bg1 = true`, `color.op = "add"`,
  `win.w1.lo = 40`), so a write moves only the bits it names; the core packs
  them into the real registers (`crates/ppu-core/src/lua.rs`), preserving
  neighboring bits. The bundled demos author compositing this way. **Both
  screens power on empty** (`TM`/`TS` = 0, `LineTableRow::default`): a layer
  draws only where a toy designates it, so a layer the toy never set up can
  never rasterize whatever VRAM happens to hold.
- **Raw hardware mnemonics** (`TM = 0x03`, `CGADSUB = 0x41`, `WH0 = 10`, …)
  are the kept low-level layer: whole-register byte writes, always valid,
  never going away — they are the pedagogy bridge to real SNES docs and the
  escape hatch for the few register bits no friendly field owns (e.g. CGWSEL's
  bits 6-7 clip-to-black, used raw by the spotlight demo). The Registers
  inspector tab stays raw-truth: hardware names + `$21xx` addresses.

Both tiers write the same registers and can be mixed freely in one script;
friendly namespaces fold last in `read_state`, and each folds only the bits
its fields moved.

## Inspector map

`web/src/studio/inspector/tabs.ts` defines the permanent inspector tab set —
all seven are part of the current architecture:

- **Workspace tabs**: Trace, Memory, Compose, Windows.
- **Full-screen overlays** (⤢ Expand): Trace/Memory open the **Memory & Layers**
  overlay; Compose/Windows open the **Compositor** overlay
  (`overlayForTab` in `tabs.ts`).
- **Aux detail tabs**: Registers, Sprites, VRAM — kept per the tab file's own
  rationale: VRAM's decoded tile/tilemap previews aren't replicated by Memory
  (which shows address-space regions + CGRAM ownership), Sprites carries the
  load-bearing Mode-7 RANGE/TIME-OVER badges, and Registers is the raw
  register truth. Aux tabs have no overlay and no distinct styling today —
  the marker is informational.

## Per-scanline editors (Mode 7, Windows)

`hdma(y0, y1, fn)` can move any `LineTableRow` field per scanline
(`crates/ppu-core/src/linetable.rs`), but the inspector's `registers` are
`derive_registers(&lt.rows[0], …)` (`crates/ppu-core/src/wasm.rs`) — **scanline
0 only**. So a swept register is invisible to a tab that reads only the frame
snapshot. Two panels solve that the same way, and a third would follow the same
three parts:

1. **A read-back seam** over the resolved `LineTable`, exported alongside the
   frame. Mode 7 has `mode7_scanline_segments` → `m7Scanlines()` (4 f32 per row,
   NaN = not mode 7); Windows has `window_scanline_bytes` → `winScanlines()`
   (`WIN_SCANLINE_STRIDE` = 11 bytes per row — WH0-3, W12SEL, W34SEL, WOBJSEL,
   WBGLOG, WOBJLOG, TMW, TSW). Both read `last_lt` and return empty before the
   first frame; both are cheap enough to pull every transport frame in a
   `…Wired` container (`Mode7PanelWired`, `WindowsTabWired`).
2. **A per-scanline view**. The M7 panel draws the fan — one sampling segment
   per row over the plane image. The Windows panel resolves its mask and its
   four WH edges per row (`compose/winScanlines.ts`), so an iris draws as a
   circle; a scanline scrubber then pins every control and readout to one row
   via `readAt`, a `ReadReg` lens that takes window registers from that row and
   falls through to the frame snapshot for everything else. `varyingRegs`
   drives the ⚡HDMA badges — which registers this frame actually sweeps.
3. **A generated `hdma()` snippet**, because the write path cannot express this.
   A poke is one line in `apply_pokes()` and therefore frame-wide; there is no
   per-scanline poke. So both panels emit copyable Lua instead — the M7
   perspective tool (`Mode7Panel.tsx`) and the window shape tool
   (`compose/winSnippet.ts`: iris / wipe / bars). The iris shape is the bundled
   `spotlight` demo, and `golden_demos.rs`
   (`spotlight_window_scanlines_trace_the_iris_chords`) pins that the demo's
   hook really does reach the feed.

Note the write asymmetry this creates: a control still pokes frame-wide, so on a
register the hook drives, the poke only survives outside the hook's range. That
is what the ⚡ chip warns about.

Pokes (`web/src/studio/pokes/`): Compose/Windows controls, CGRAM cell colors,
and register readout rows all poke through one path — `poke()`/`unpoke()`/
`clearPokes()` (`pokeStore.ts`) parse and regenerate the reserved, read-only
`pokes.lua` file (`POKES_FILE`, always tab 0) from a `{lvalue, expr, note?}`
list (`pokes.ts`). Pokes come in three dialects sharing that one file format:
friendly field assignments (`color.op = "sub"`, `screen.main.bg1 = true`,
`win.w1.lo = 40`) where the field is the poke's identity, so each touched
field owns its line and neighboring bits are preserved by the core's namespace
fold, raw whole-register writes (`TM = 0x13`) — the register-readout hex
editor still pokes raw — and **scanline** keyframes (see below). Which dialect
NEW pokes emit is the **POKE AS friendly|raw|scanline** toggle
(`DialectToggle`, `inspector/compose/chrome.tsx`, shown above the editor while
the `pokes.lua` tab is active) — default friendly,
persisted at localStorage `ppu.toys:poke-dialect`
(`inspector/compose/dialect.ts`). Emission-only: `parsePokes` is
dialect-agnostic, so a saved `pokes.lua` in any or mixed dialect still
loads. The two frame-wide dialects never coexist on one register: writing a poke evicts
the other dialect's pokes for that same register (`evictCrossDialect`,
`inspector/compose/model.ts`) — so re-poking a control after flipping the
toggle migrates its line to the current dialect, and a raw `CGADSUB = 0x80`
never lingers under a friendly `color.op = "add"` that would silently win at
fold time. The FILE is the source of truth: every poke rewrites the
whole generated `apply_pokes()` function body, entries sorted by lvalue for
byte-stable output. Script wins by convention, not by a separate override
layer: `apply_pokes()` runs as `frame()`'s first line (every bundled demo and
the new-sketch template call it there — see Demos below), so a later
assignment in the script to the same lvalue overrides the poke for that
frame. A poked control carries a dot marker (`PokeDot`,
`inspector/compose/chrome.tsx`) — solid while the live register still reads
the poked value, hollow ("poked · live value differs (script write or
quantization)") once a later script write has moved it — or when the engine
masked an out-of-range poked value down to the register's real width. To save
a configuration beyond the session, copy
the generated `apply_pokes()` source (`PokeBar`'s "copy fn") into a file of
your own under a new name — hand-edits to `pokes.lua` itself are overwritten
by the next poke. Poking a bundled demo forks it like any other edit. Pokes
are a file, not session state: ▶ Run and a reload both leave `pokes.lua`
untouched; a warning chip appears if pokes exist but no file calls
`apply_pokes()`.

### The scanline dialect

A frame-wide poke is one assignment. A **scanline** poke is a keyframe list
that the generated file interpolates inside an `hdma()` hook — one assignment
per line. The `Poke` record is unchanged: the keyframes ride in `expr` as
`{{0,128},{112,58},{223,128}}`, and an expr starting with `{` IS the
discriminator (no scalar register value can be a table), so `upsertPoke`,
`evictCrossDialect` and the poke chips all keep working without knowing the
dialect exists (`pokes/scanlinePokes.ts`).

Each scanline poke carries an inclusive **range** (`Poke.range`, default the
whole frame) — the lines its hook covers. `pokesToLua` groups the pokes by
range and emits one `hdma()` per distinct range, after the frame-wide lines;
the range lives on the hook header, which is where it belongs in readable Lua,
so `parsePokes` reads it back off that line rather than having it smuggled into
every keyframe expr. Several scoped hooks each doing their own thing is the
normal case, not a workaround —
`crates/ppu-core/tests/hook_composition.rs` pins that hooks compose: writes
don't leak past a hook's range, hooks touching different fields of one register
both survive, and a later hook sees an earlier one's write on the same row.

The `pk`/`pki`/`pkf` helpers are emitted **only when a scanline poke exists** —
a sketch with none generates byte-identical output to before, which matters
because every bundled demo ships a generated (empty) `pokes.lua`.
`pki` rounds and `pkf` does not: the DSL silently ignores a fractional write to
an integer register (`to_integer()` rejects it), so an interpolated `127.375`
would leave the register at its default instead of at 127. `pkHelper` picks per
field; `keyframeValueAt` mirrors that rounding so the panel reads back exactly
what the engine will write.

In the Windows tab the selected scanline binds writes as well as reads
(`WindowsTab.tsx`), so in this dialect dragging an edge keyframes the line you
are looking at; `KeyframeTrack` lists the resulting curves. Where no scanline
is selected (Compose), the dialect degrades to a frame-wide friendly poke, and
a field with no numeric value (`screen.main.bg1`, `color.op`) has no curve to
keyframe so it stays frame-wide too.

**Two precedence rules, and they differ.** `apply_pokes()` runs first in
`frame()`, so its hooks register before any hook the script registers later —
a script `hdma()` still wins, matching the frame-wide convention. But a hook
always beats the frame defaults, so _within its range_ a scanline poke is not
overridden by a plain later assignment in the script. That is not a DSL wart:
real HDMA behaves the same way, since a channel rewriting a register every
scanline overrides whatever was written once during vblank. Narrowing a poke's
range is how you hand those lines back to the script — outside the range the
hook never runs. `crates/ppu-core/tests/scanline_pokes.rs` pins all of it
against the real engine, along with the generated file's exact shape.

Converting dialects is lossy in one direction: any → scanline makes each value
a single held keyframe (same rendered result), but scanline → friendly/raw
collapses the curve to its line-0 value, because a sweep cannot survive the
whole-register byte model `regeneratePokes` projects through. The poke marker
stays neutral on a scanline poke rather than claiming a match — `registers` is
scanline 0 only, so there is no single live value to compare.

## Demos + assets

Bundled demos live in `web/src/studio/demos/demos.ts` as `{id, label, source,
files, assets}`. Every demo ships `files` explicitly, generated `pokes.lua`
first (empty, read-only) then `main.lua` (and, for `dusk-parallax`,
`palette.lua`) — `demoFiles()` returns that ordered list; `source` is the same
files joined tab-order with `"\n"`, kept for the single-string call sites.
Each demo's `main.lua` calls `apply_pokes()` as `frame()`'s first line, same
as the new-sketch template, so poking a demo behaves exactly like poking a
sketch (see Pokes above). Demo sources author compositing in the friendly
dialect (`screen`/`color`/`win`) with raw mnemonics only where no friendly
field exists; the Rust golden tests render the identical sources, so the
committed PNGs prove the friendly form byte-equals the old raw-register form.
Each demo's procedural pixel assets (raw RGBA,
generated in TS mirroring the generators in
`crates/ppu-core/tests/golden_demos.rs` — tuned for how the demos look on
screen, not byte-identity with the fixtures) are uploaded into the live core
by `web/src/studio/demos/loadDemo.ts` when a demo is opened.

Users can also drop a PNG onto the output canvas
(`web/src/studio/output/DropZone.tsx`): it is quantized into VRAM tiles + a
CGRAM sub-palette and imported into the open sketch.

## Dev commands

Run from the repo root (`Cookfile`):

- `cook dev-wasm` — Vite dev server against the real WASM core (`cook dev`
  runs against a mock core instead, no wasm build needed).
- `cook check` — pre-commit umbrella: `typecheck` (tsc) + `test-core` (cargo
  test) + `test-web` (vitest).
- `cook build` — production pipeline: builds the wasm module, then `web/dist`.
- Golden regen: `cargo test -p ppu-core regen_golden -- --ignored` (the
  `crates/ppu-core/tests/golden_demos.rs` `regen_golden_*` tests rewrite the
  committed golden PNGs).
