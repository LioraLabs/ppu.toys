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
  *current* `t`/`f` (`web/src/studio/transport/transport.ts`: "recompile never
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
present) are resolved only *after* every chunk has run, so any file can
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
  neighboring bits. The bundled demos author compositing this way.
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

`web/src/studio/inspector/tabs.ts` defines the M9 done-gate tab set — the full
seven are permanent:

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

Pokes (`web/src/studio/pokes/`): Compose/Windows controls, CGRAM cell colors,
and register readout rows all poke through one path — `poke()`/`unpoke()`/
`clearPokes()` (`pokeStore.ts`) parse and regenerate the reserved, read-only
`pokes.lua` file (`POKES_FILE`, always tab 0) from a `{lvalue, expr, note?}`
list (`pokes.ts`). Pokes come in two dialects sharing that one file format:
friendly field assignments (`color.op = "sub"`, `screen.main.bg1 = true`,
`win.w1.lo = 40`) where the field is the poke's identity, so each touched
field owns its line and neighboring bits are preserved by the core's namespace
fold, and raw whole-register writes (`TM = 0x13`) — the register-readout hex
editor still pokes raw. Which dialect NEW pokes emit is the **POKE AS
friendly|raw** toggle (`DialectToggle`, `inspector/compose/chrome.tsx`, shown
in the Compose/Windows tabs and the Compositor overlay) — default friendly,
persisted at localStorage `ppu.toys:poke-dialect`
(`inspector/compose/dialect.ts`). Emission-only: `parsePokes` is
dialect-agnostic, so a saved `pokes.lua` in either or mixed dialect still
loads. The two dialects never coexist on one register: writing a poke evicts
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
