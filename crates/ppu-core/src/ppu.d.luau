-- ppu.d.luau: the globals a ppu.toys program sees, as Luau declarations for editors and type
-- checkers (for luau-lsp, list it in "luau-lsp.types.definitionFiles"). They declare the API and
-- do not implement it. `ppu new` and `ppu unpack` write this file beside a Luau toy (`ppu new` and
-- `ppu types` also write the luau-lsp setup), and the engine test tests/luau_declarations.rs keeps
-- it equal to the runtime's globals. Types are `export type`: luau-lsp only shows exported ones.
-- This file is the one home of API prose: the comment block directly above a declaration (or a
-- function field of a declared table) is that API's documentation, so `ppu docs <api>` prints it.

-- The program ------------------------------------------------------------------------------------
-- A toy defines `init()` (once, optional) and `frame(t, f)` (every video frame: t in seconds, f the
-- frame index). t counts as a cartridge counts it, in whole 1/1024 s steps of f at 60 frames a
-- second, rounded down: at f = 2, t * 60 is just under 2. A scene's t is its frame's t less its
-- entry frame's. A Luau toy's files load each other with `require("./name")` (Luau's own require,
-- not declared here, so checkers resolve each module's type). `require("@ppu/name")` loads a
-- library the engine ships; see Libraries at the end.

-- Numbers with contracts --------------------------------------------------------------------------

-- A whole number that keeps to minimum..maximum (ordered integer literals). It initializes a named
-- binding or record field directly; later stores are checked against the bounds without clamping,
-- and a cartridge stops on a store outside them, so clamp or wrap the value itself.
declare function makeBounded(value: number, minimum: number, maximum: number): number
-- A number with a fraction, kept in steps of 1/denominator (a power of two from 1 through 1024,
-- default 1024) between minimum and maximum (ordered integer literals). It turns on fixed-point
-- arithmetic for the binding; like makeBounded, a later store outside the bounds is an error.
declare function makeFixed(value: number, minimum: number, maximum: number, denominator: number?): number

-- Save data: one record kept in battery-backed RAM. Fields are makeBounded(default, min, max) with
-- literal bounds, or true/false; read and assign them as plain fields. A first power-on, a changed
-- record or a damaged save starts from the defaults. Call it once, at file level.
declare function saveData<T>(fields: T): T

-- Precomputed tables ------------------------------------------------------------------------------

-- A grid gives each parameter of a lookup or bakeHdma function {min, max, step?}, by name; calls
-- use the nearest grid point. Up to five parameters.
export type SampleGrid = {[string]: {number}}
-- A function of numbers computed ahead of time over a grid: the returned function looks up the
-- nearest grid point's results (1/1024 precision) instead of running `evaluate`. Write `evaluate`
-- inline; it must not change game state, read the pad or yield.
declare function lookup<R...>(grid: SampleGrid, evaluate: (...number) -> R...): (...number) -> R...
-- A scanline effect computed ahead of time over a grid: the returned function, called in frame,
-- runs line(y, ...) on each scanline from first to last as hdma does, from a table baked for the
-- nearest grid point instead of running the Luau each frame.
declare function bakeHdma(grid: SampleGrid, first: number, last: number, line: (number, ...number) -> ()): (...number) -> ()

-- Routines ------------------------------------------------------------------------------------------

-- A handle on work spread over several updates: its body runs a little on each step() and pauses at
-- yield(updates). Locals and loop positions survive the pause.
export type Routine<A...> = {
  -- Captures the arguments and starts the body: true, or false (changing nothing) when it already runs.
  start: (Routine<A...>, A...) -> boolean,
  -- Runs the body to its next yield or its end. Call it once an update.
  step: (Routine<A...>) -> (),
  -- Drops the running invocation; writes it already made stay.
  cancel: (Routine<A...>) -> (),
  -- Whether the body runs: true from a successful start until it returns or is cancelled.
  running: (Routine<A...>) -> boolean,
}
-- A routine whose body is `body`; nothing runs until its start.
declare function routine<A...>(body: (A...) -> ()): Routine<A...>
-- Inside a routine's body: pause for `updates` steps (one when nil). An error outside a routine, and
-- in hdma callbacks and lookup functions.
declare function yield(updates: number?): ()

-- math.random keeps Luau's own types: math.random(n) is a whole number 1..n, math.random(m, n) one m..n,
-- math.randomseed(s) restarts from s's low 16 bits. One 16-bit xorshift in the preview and on a
-- cartridge (the same numbers from the same seed); a cartridge refuses math.random() without arguments.
-- math.hash(seed: number, x: number, y: number): number is 0 to 255 for a seed (0 to 65535) and two whole
-- numbers: no state, the same in the preview and on a cartridge, for repeatable scatter and level seeds.

-- Bit operations on whole numbers, as Lua's `& | ~ << >>`: signed, no 32-bit wrap (bnot(0) is -1).
-- Two operands each; a cartridge needs a constant shift count (0 to 30) and a right shift of a value
-- it can prove non-negative.
declare bit32: {
  -- a & b: the bits set in both.
  band: (number, number) -> number,
  -- a | b: the bits set in either.
  bor: (number, number) -> number,
  -- a ~ b: the bits set in exactly one.
  bxor: (number, number) -> number,
  -- ~a: every bit flipped, so bnot(x) is -x - 1.
  bnot: (number) -> number,
  -- a << n: a times 2^n, n a constant 0 to 30 on a cartridge.
  lshift: (number, number) -> number,
  -- a >> n: a non-negative a divided by 2^n, rounded down; n a constant 0 to 30 on a cartridge.
  rshift: (number, number) -> number,
}

-- Display ----------------------------------------------------------------------------------------

-- Which of the five layers and the backdrop a screen (or the colour math) takes part in.
export type Layers = {bg1: boolean, bg2: boolean, bg3: boolean, bg4: boolean, obj: boolean, backdrop: boolean}
-- One tilemap cell: its tile, palette (0-7), priority (0 or 1) and flips.
export type MapEntry = {tile: number, pal: number?, prio: number?, flip_x: boolean?, flip_y: boolean?}
-- One background layer, bg[1] to bg[4]: its scroll, tile size (8 or 16), VRAM map and character
-- bases (word addresses), screen size (0 one screen, 1 two wide, 2 two tall, 3 four), mosaic, and
-- its tilemap cells by map[y][x].
export type Background = {
  scroll: {x: number, y: number},
  -- Mode 2 only: columns[c] = dy moves map column c by dy pixels (added to scroll.y); nil or 0 puts it back.
  columns: {[number]: number?},
  visible: boolean,
  tile_size: number,
  map_base: number,
  screen_size: number,
  char_base: number,
  mosaic: boolean,
  -- map[x][y] is one cell; map:fill(x, y, w, h, entry) sets a w x h rectangle of cells to entry, inside the layer's map.
  map: {[number]: {[number]: MapEntry}, fill: (self: any, x: number, y: number, w: number, h: number, entry: MapEntry) -> ()},
}
-- One raw OAM entry. Programs draw sprites through sprites.art and sprites.group; obj[i] is for the
-- inspector and for reading what the groups wrote.
export type Sprite = {
  x: number, y: number, tile: number, pal: number, prio: number,
  on: boolean, large: boolean, flip_x: boolean, flip_y: boolean,
}
-- A window's left and right edges, pixels 0-255 inclusive.
export type WindowEdges = {lo: number, hi: number}
-- How a layer uses the two windows: which apply, inverted or not, how they combine, and on which screens.
export type WindowSelection = {
  w1: boolean, w2: boolean, invert: boolean,
  combine: "OR" | "AND" | "XOR" | "XNOR",
  main: boolean, sub: boolean,
}

-- The background mode, 0 to 7: how many backgrounds there are and their colour depths (Mode 1: two
-- 16-colour backgrounds and a 4-colour bg3; Mode 7: one rotating plane).
declare mode: number
-- The screen's brightness, 0 (black) to 15 (full).
declare brightness: number
-- True blanks the screen to black and leaves VRAM free to write.
declare force_blank: boolean
-- The mosaic block size, 0 (one pixel) to 15 (16 pixels), for backgrounds whose mosaic is on.
declare mosaic: number
-- True draws 8bpp backgrounds' pixel values as colours directly instead of through cgram.
declare direct_color: boolean
-- In Mode 1, true puts bg3's priority-1 tiles in front of everything, sprites included.
declare bg3_priority: boolean
-- The four background layers, bg[1] to bg[4].
declare bg: {[number]: Background}
-- Raw OAM: obj[0] to obj[127] and the sprite settings (character base, size pair, name select,
-- priority rotation). Draw through sprites.group instead of writing entries.
declare obj: {
  [number]: Sprite,
  char_base: number, size_sel: number, name_select: number,
  priority_rotate: boolean, oam_addr: number, first: number,
}
-- The Mode 7 plane: its matrix (a, b, c, d) and centre (cx, cy), wrap, flips, EXTBG, and its 128x128
-- map of tile numbers by map[y][x]. mode7_transform and mode7_floor compute matrices for it.
declare m7: {
  a: number, b: number, c: number, d: number, cx: number, cy: number,
  wrap: number, flip_x: boolean, flip_y: boolean, extbg: boolean,
  map: {[number]: {[number]: number}},
}
-- The layers on the main screen and on the sub screen (the one colour math blends in).
declare screen: {main: Layers, sub: Layers}
-- Colour math: the fixed colour, which layers it applies to, add or subtract, halved or not, whether
-- it adds the fixed colour or the sub screen, and where (by the colour window).
declare color: {
  fixed: number, on: Layers, op: "add" | "sub", half: boolean,
  addend: "fixed" | "sub",
  region: "everywhere" | "inside" | "outside" | "never",
}
-- The two windows' edges and each layer's window selection, colour math's included.
declare win: {
  w1: WindowEdges, w2: WindowEdges,
  bg1: WindowSelection, bg2: WindowSelection, bg3: WindowSelection, bg4: WindowSelection,
  obj: WindowSelection, color: WindowSelection,
}
-- Video RAM by word address, 0x0000 to 0x7FFF. Place art with dma(); vram[] loops in init() keep a
-- console black for seconds.
declare vram: {[number]: number}
-- The 256 palette colours, 15-bit BGR: cgram[0] is the backdrop. Write rgb(), hsl() or packed values.
declare cgram: {[number]: number}

-- Raw registers, as bytes ----------------------------------------------------------------------------

-- $212C: the main screen's layers, one bit each (bg1 bit 0 to obj bit 4).
declare TM: number
-- $212D: the sub screen's layers, bits as TM.
declare TS: number
-- $212E: the layers the windows mask on the main screen, bits as TM.
declare TMW: number
-- $212F: the layers the windows mask on the sub screen, bits as TM.
declare TSW: number
-- $2126: window 1's left edge.
declare WH0: number
-- $2127: window 1's right edge.
declare WH1: number
-- $2128: window 2's left edge.
declare WH2: number
-- $2129: window 2's right edge.
declare WH3: number
-- $2123: bg1 and bg2's window enables and inversions.
declare W12SEL: number
-- $2124: bg3 and bg4's window enables and inversions.
declare W34SEL: number
-- $2125: the sprites' and colour window's enables and inversions.
declare WOBJSEL: number
-- $212A: how the two windows combine for bg1 to bg4.
declare WBGLOG: number
-- $212B: how the two windows combine for the sprites and the colour window.
declare WOBJLOG: number
-- $2130: colour math's clip, prevent and addend (fixed colour or sub screen) settings.
declare CGWSEL: number
-- $2131: colour math's layers, add or subtract, and halving.
declare CGADSUB: number
-- The fixed colour as one 15-bit value; coldata writes it one channel at a time, as $2132 does.
declare COLDATA: number
-- A $2132 write: bits 5, 6, 7 pick red, green, blue, and each picked channel becomes bits 0-4; the
-- others keep their value.
declare function coldata(byte: number): ()

-- Per-scanline changes --------------------------------------------------------------------------------

-- Runs line(y) for each scanline y from first to last (0 to 223), and the registers it writes change
-- from that line down: HDMA. Call it in frame; to precompute an effect, use bakeHdma.
declare function hdma(first: number, last: number, line: (number) -> ()): ()
-- The same as hdma, under its other name.
declare function scanline(first: number, last: number, line: (number) -> ()): ()

-- Graphics sources -------------------------------------------------------------------------------

-- Where dma puts a source: character, map and palette (CGRAM row) addresses, a sample's ARAM address,
-- or another palette source to colour it with. Every field is optional; dma picks free space.
export type Placement = {char: number?, map: number?, pal: number?, addr: number?, palette: string?}
-- One cell of a placed sheet: its first tile, palette and flips.
export type Cell = {tile: number, pal: number, flip_x: boolean, flip_y: boolean}
-- Where a placed source went: its addresses, first tile and palette, its cells (a sheet's, for
-- sprites.art), its size in tiles, and the next free char, map, palette and ARAM addresses.
export type Graphics = {
  char: number, map: number, pal: number, tile: number, cells: {Cell},
  tiles_w: number, tiles_h: number, next_char: number, next_map: number, next_pal: number,
  id: number, addr: number, next_addr: number, cell_w: number?, cell_h: number?,
}
-- Places the named image, palette, tileset or sample source in VRAM, CGRAM or ARAM and returns where it
-- went. Setup-only (file level or init): big graphics go in this way, never through vram[] loops.
declare function dma(source: string, placement: Placement?): Graphics
-- Sets pixel (x, y), 0 to 7, of Mode 7 tile `tile` to colour index `index`, leaving the map's byte of
-- that VRAM word alone. For drawing Mode 7 art in code; an image source placed with dma is faster.
declare function m7pixel(tile: number, x: number, y: number, index: number): ()

-- Colours and Mode 7 views -------------------------------------------------------------------------

-- A 15-bit colour from red, green and blue 0 to 255 (each rounded to 5 bits).
declare function rgb(red: number, green: number, blue: number): number
-- A 15-bit colour from a hue in degrees (wraps), saturation and lightness 0 to 1 (clamped).
declare function hsl(hue: number, saturation: number, lightness: number): number
-- A Mode 7 matrix, centre and scroll, as mode7_transform and mode7_floor compute them: assign the
-- fields to m7 and bg[1].scroll to take effect.
export type Mode7View = {
  a: number, b: number, c: number, d: number, cx: number, cy: number,
  scroll_x: number, scroll_y: number, visible: boolean,
}
-- The view that shows texture point (x, y) at the screen's centre (128, 112), turned `angle` degrees
-- and zoomed (2 shows the texture twice as large; at least 1/127). Centres wrap at 1024.
declare function mode7_transform(angle: number, zoom: number, x: number, y: number): Mode7View
-- One perspective floor scanline: line y of a floor whose horizon is at line `horizon`, seen from
-- `height` above texture point (x, z) facing `heading` degrees, focal length 128. Lines at and above the
-- horizon are not visible. Preview only: a cartridge bakes floors with bakeHdma.
declare function mode7_floor(y: number, horizon: number, height: number, heading: number, x: number, z: number): Mode7View

-- Sprites ------------------------------------------------------------------------------------------
-- Art made of a sheet's cells, drawn in groups (the first made on top). A level may take its cells
-- from a sheet of its own and one palette for all of them, or `palette = "name"` (a palette source,
-- with `pal` the OBJ palette its colours go to) to colour them as a variant; a draw may flip the art
-- and pick one palette for it. SpriteLevel and SpriteArt are exported, so a module can type a table of
-- levels as {SpriteLevel} and a helper's art as SpriteArt.

-- One size of a piece of art: its size in pixels (8, 16, 32 or 64), how many cells wide it is, and
-- which of the sheet's cells it is made of; optionally its own sheet and palette.
export type SpriteLevel = {size: number, columns: number, cells: {number}, sheet: (SpriteSheet | Graphics)?, pal: number?, palette: string?}
-- Art made with sprites.art: a handle to draw.
export type SpriteArt = {}
-- An animation made with sprites.animation: draws like art.
export type SpriteAnimation = {}
-- A draw's options: size (which level), prio (0-3), key (a number: the same key keeps its slots
-- across frames), depth (a sorted group's order), flips, pal, and `at` (an animation's clock).
export type SpriteOptions = {size: number?, prio: number?, key: number?, depth: number?, flip_x: boolean?, flip_y: boolean?, pal: number?, at: number?}
-- A group of sprites made with sprites.group: draw into it every frame.
export type SpriteGroup = {
  -- Draws art or an animation with its top left at (x, y) this frame; a sprite not drawn (or kept)
  -- this frame is gone.
  draw: (self: SpriteGroup, art: SpriteArt | SpriteAnimation, x: number, y: number, options: SpriteOptions?) -> (),
  -- Leaves key `key`'s sprites exactly as they were last frame, for a sprite that did not move.
  keep: (self: SpriteGroup, key: number) -> (),
}
-- A sheet: what dma returns for an image source cut into cells (or the same fields written out).
export type SpriteSheet = {name: string?, tile: number, pal: number, cells: {Cell}, cell_w: number?, cell_h: number?}
-- The sprites library: art, animations and groups, and the settings every group shares.
declare sprites: {
  -- True rotates each unsorted group's unkeyed sprites every frame, so a line with more than 32
  -- sprites flickers instead of losing the same ones.
  flicker: boolean,
  -- Art from a sheet's cells: one level per size it draws at, the sheet usually what dma returned.
  -- Made at file level.
  art: (levels: {SpriteLevel}, sheet: SpriteSheet | Graphics) -> SpriteArt,
  -- An animation, made at file level, shows each art for `frames` video frames (one number, or a list
  -- with one per art, as Tiled's tile animations), looping; {once = true} holds the last. It draws
  -- like art, `flip_x`, `pal` and the rest passing through. Its clock counts video frames: `at` (the
  -- game's own count: `{at = f}`, an actor's age), else its key's, from 0 when the key starts drawing
  -- it; drawing the key with other art (walk, then jump) or leaving it undrawn an update restarts it,
  -- and so does a scene's entry. At 30 Hz the clock moves 2 an update. An unkeyed draw needs `at`. A
  -- cartridge follows an animation through file-level locals and tables, a module's returned table,
  -- locals and the program's own function parameters; storing it anywhere else (a table at run time, a
  -- return) is refused.
  animation: (arts: {SpriteArt}, frames: number | {number}, options: {once: boolean?}?) -> SpriteAnimation,
  -- A group of sprites, made at file level; the first group made draws on top. sort orders its
  -- draws by depth, slots sets its sprite count where its draws can't tell it, keep = true makes a group that keeps what it drew (a
  -- sprite HUD).
  group: (options: {sort: boolean?, slots: number?, keep: boolean?}?) -> SpriteGroup,
  -- Draws into the default group, as group:draw.
  draw: (art: SpriteArt | SpriteAnimation, x: number, y: number, options: SpriteOptions?) -> (),
  -- Keeps a key's sprites in the default group, as group:keep.
  keep: (key: number) -> (),
  -- Keeps every group's sprites as they were, for a frame that draws none (a pause screen). A frame
  -- that holds may not also draw.
  hold: () -> (),
  -- Keeps the rest of the frame's draws, in every group, inside rows top up to but not including
  -- bottom (a split screen). The next frame starts with the whole screen again.
  rows: (top: number, bottom: number) -> (),
}

-- Scenes ---------------------------------------------------------------------------------------------

-- A scene: a module returning {init, frame} and optionally preview. preview stages the scene for the
-- Studio's Scene view (State.hp = 1): it runs after file level and every scenes.add, before init, when
-- the switcher enters the scene, Restart scene restarts it, or an edit reloads the chosen scene;
-- power-on and scenes.go never run it. init and preview are optional: typed `any` so a module's table
-- fits with or without them (table fields are invariant in Luau).
export type SceneModule = {init: any, frame: (t: number, f: number) -> (), preview: any}
-- A scene's handle, from scenes.add.
export type Scene = number
-- The scenes library: register scenes and switch between them.
declare scenes: {
  -- Registers a scene at file level, scenes.add(require("./title")), and returns its handle. The program
  -- starts in the first scene added.
  add: (scene: SceneModule) -> Scene,
  -- Enters a scene with a cold boot on the next update; its t and f count from 0 there.
  -- scenes.go(scene, {fade = n}) fades the switch: the scene keeps running while the screen darkens over
  -- n video frames (1 to 255; at 30 Hz an update spans two), then the cold boot, and the entered scene
  -- brightens over n more. The engine owns brightness meanwhile (a scene's own writes show again once it
  -- is over, at 15); while a fade runs, scenes.go changes nothing.
  go: (scene: Scene, how: {fade: number}?) -> (),
}

-- HUD text ---------------------------------------------------------------------------------------------

-- A HUD descriptor: the map's VRAM word address and its size in cells (width and height 1 to 32), and
-- optionally palette (0 to 7), padding (a field's leading " " or "0"), wrap (a boolean), priority (the
-- cells' tile priority, 0 or 1), align ("left", "center", "right"), glyph ("8x8", "8x16", "16x16") and
-- font (the big font's first tile). Font tiles start from the space character at tile zero; cells are
-- zero-based. The optional fields are typed `any` so a descriptor table fits with or without them
-- (table fields are invariant in Luau). See @ppu/hud in Libraries.
export type Hud = {map: number, width: number, height: number, palette: any, padding: any, wrap: any, priority: any, align: any, glyph: any, font: any}
-- Deprecated: use @ppu/hud's hud.text. Writes text at cell (x, y). Preview only; a cartridge build refuses it.
declare function hud_text(map: Hud, x: number, y: number, text: string, palette: number?): ()
-- Deprecated: use @ppu/hud's hud.field. Writes a number `width` cells wide. Preview only; a cartridge
-- build refuses it.
declare function hud_number(map: Hud, x: number, y: number, value: number, width: number, palette: number?): ()
-- Deprecated: @ppu/hud redraws only what changed. Blanks a rectangle of cells.
-- Preview only; a cartridge build refuses it.
declare function hud_clear(map: Hud, x: number?, y: number?, width: number?, height: number?): ()

-- Input and save data -------------------------------------------------------------------------------

-- The twelve buttons, true while held (or, in pressed and released, on this update's edge).
export type PadButtons = {
  up: boolean, down: boolean, left: boolean, right: boolean,
  a: boolean, b: boolean, x: boolean, y: boolean, l: boolean, r: boolean,
  start: boolean, select: boolean,
}
-- pad.a is held; pad.pressed.a went down this update, pad.released.a came up. The first update of
-- the program and of each scene has no edges: a button held across scenes.go is not a press.
export type Pad = PadButtons & {pressed: PadButtons, released: PadButtons}
-- Controller 1.
declare pad: Pad
-- Controller 2.
declare pad2: Pad
-- Raw battery-backed RAM, kept per toy. Prefer saveData's typed record.
declare sram: {[any]: any}

-- Sound ------------------------------------------------------------------------------------------

-- One of the eight S-DSP voices: its sample, 14-bit pitch (0x1000 plays at the recorded rate), volume,
-- envelope (ADSR or gain), noise, pitch modulation and echo; envx, outx and ended read its state.
export type Voice = {
  sample: number, pitch: number, vol: {l: number, r: number},
  adsr: {a: number, d: number, s: number, r: number},
  gain: number?, noise: boolean, pmod: boolean, echo: boolean,
  envx: number, outx: number, ended: boolean,
}
-- An instrument preset, as instrument and bank make: sample, envelope, vol (0 to 127, default 127),
-- pan (-1 to 1), base (the note it was recorded at, default "C4"), pitch (what sfx plays with no note,
-- default 0x1000), noise, pmod, echo, and len (how long sfx lets it sound, seconds, or false for
-- until koff).
export type Instrument = {
  sample: number?, adsr: {a: number, d: number, s: number, r: number}?, gain: number?,
  vol: number?, pan: number?, base: (string | number)?, pitch: number?,
  noise: boolean?, pmod: boolean?, echo: boolean?, addr: number?, next_addr: number?,
  len: (number | boolean)?,
}
-- A playing song, from score: its tick (250 a second), length, whether it plays, its loop, its
-- name, and play and stop. One score plays at a time: play() stops whichever other one plays.
export type Song = {
  tick: number, length: number, playing: boolean, loop: (boolean | number)?, song: string,
  play: () -> (), stop: () -> (),
}
-- The eight voices, voice[0] to voice[7].
declare voice: {[number]: Voice}
-- The S-DSP's chip-wide state: master and echo volume, echo delay, feedback and filter, the noise
-- clock, and mute. Master volume starts at 0: nothing sounds until it is set, except that a score
-- started while it is 0 opens it to full.
declare dsp: {
  mvol: {l: number, r: number}, evol: {l: number, r: number},
  echo: {delay: number, feedback: number, fir: {number}},
  noise_clock: number, mute: boolean,
}
-- Sound RAM (64 KB) by byte address. Place samples with dma or bank.
declare aram: {[number]: number}
-- The built-in instrument presets by name ("piano", "kick", ...), as bank places them; set fields a
-- song row can't carry (adsr, echo) here before score.
declare BANK: {[string]: Instrument}
-- Keys on the given voices (0 to 7): each starts its sample from the top.
declare function kon(...: number): ()
-- Keys off the given voices: each fades by its release.
declare function koff(...: number): ()
-- The 14-bit pitch of a note name ("C4", "C#4", "Db4", "A-1") or MIDI number, relative to `base`
-- (default "C4", the note the sample was recorded at).
declare function note(name: string | number, base: (string | number)?): number
-- An instrument preset from its fields, defaults filled in (see Instrument).
declare function instrument(preset: Instrument): Instrument
-- Places a built-in sample (or an uploaded one, by name) with dma and returns its preset; options
-- override any field (an uploaded sample needs an adsr). Setup-only: `zap = bank("pluck")` at file level.
declare function bank(preset: string, options: Instrument?): Instrument
-- Plays a sound effect: applies the preset to a voice and keys it on at `note` (else the preset's pitch).
-- Without `voice` the sound driver picks one: a voice the song does not use, one it rests on longer than
-- the effect, else the highest song voice not kept; with every voice held by effects, the oldest automatic
-- effect's. Returns the voice (nil when none is left). Name `voice` for effects that should cut each other
-- off. Either way the effect borrows its voice from the song until it ends, the song's notes there silent
-- meanwhile. It keys off `len` seconds later (the call's, else the preset's); with neither a looping sample
-- stops after 0.15 s and a one-shot plays to its end. len = false has no end: in the preview it sounds until
-- koff; a cartridge has no koff, so the effect holds its voice until another effect takes it and the song
-- loses that voice meanwhile. Give an automatic effect a len instead.
declare sfx: ((instrument: Instrument, note: string?, options: {len: (number | boolean)?}?) -> number?)
    & ((instrument: Instrument, voice: number, note: (string | number)?, options: {len: (number | boolean)?}?) -> number?)
-- Plays a song source on the timeline, choosing voices per note: loop (false plays it once through),
-- `at` the second tick 0 falls on (false: set up, not playing). Setup-only; returns the song's handle.
declare function score(options: {song: string, loop: (boolean | number)?, at: (number | boolean)?}): Song
-- The audio clock: SPC700 timer n (0 and 1 at 8 kHz, 2 at 64 kHz) fires every `divider` counts and
-- calls tick(off, t) at its sample offset and time; divider is 1 to 255. A hook may not write PPU registers, VRAM or CGRAM.
-- Setup-only.
declare function timer(n: number, divider: number, tick: (number, number) -> ()): ()

-- Maths ------------------------------------------------------------------------------------------

-- 3.14159..., half a turn in radians.
declare pi: number
-- The sine of x radians.
declare function sin(x: number): number
-- The cosine of x radians.
declare function cos(x: number): number
-- The tangent of x radians.
declare function tan(x: number): number
-- The largest whole number not above x.
declare function floor(x: number): number
-- The smallest whole number not below x.
declare function ceil(x: number): number
-- x without its sign.
declare function abs(x: number): number
-- The square root of x.
declare function sqrt(x: number): number
-- The smallest of its arguments.
declare function min(x: number, ...: number): number
-- The largest of its arguments.
declare function max(x: number, ...: number): number
-- value held to minimum..maximum.
declare function clamp(value: number, minimum: number, maximum: number): number
-- a + (b - a) * t; extrapolates outside t = 0..1.
declare function lerp(a: number, b: number, t: number): number
-- A blend of two 15-bit colours, each 5-bit channel interpolated and rounded, t clamped to 0..1.
declare function lerp_color(a: number, b: number, t: number): number
-- An easing curve's name.
export type Easing = "linear" | "smooth" | "ease-in" | "ease-out" | "step"
-- Keyframes {{position, value}, ...} in rising position order.
export type RampKeys = {{any}}
-- Eased progress for progress 0 to 1 (linear when easing is nil).
declare function ease(progress: number, easing: Easing?): number
-- The keyframes' value at `position`, interpolated, fractions kept.
declare function ramp(keys: RampKeys, position: number): number
-- ramp rounded to a whole number (floor(value + 0.5)): for brightness, scroll and window edges.
declare function ramp_int(keys: RampKeys, position: number): number
-- The keyframes' 15-bit colour at `position`, each RGB channel interpolated.
declare function ramp_color(keys: RampKeys, position: number): number

-- Libraries --------------------------------------------------------------------------------------
-- require("@ppu/name") returns the engine library lib/name.luau: one copy shipped with the engine,
-- the same for every toy, built into the cartridge from the same source. Each library's value is
-- typed `ppu_<name>` in its own block below (a checker without the engine's aliases can annotate:
-- local blink: ppu_blink = require("@ppu/blink")). The Studio completes `name.` from these blocks:
-- keep one field per line.

-- @ppu/blink: on/off timing for flashing text and invulnerable sprites.
export type ppu_blink = {
  -- Whether frame f is in the "on" half of a blink: on for `half` frames, then off for `half`, starting
  -- on at f = 0. blink.on(p.safe, 4) flashes a sprite every 4 frames while p.safe counts.
  on: (f: number, half: number) -> boolean,
}

-- @ppu/hud: a retained background HUD on a Hud descriptor's map. Static text is written once; a field or
-- line is a record whose set costs one compare when nothing changed, and nothing runs at frame end.
-- Make fields and lines once (init), set them every frame.
export type ppu_hud = {
  -- Writes text at cell (x, y), once: a label. With align it is centred or right-aligned in the cells
  -- from x to the map's edge; with glyph each character is 1x2 or 2x2 cells from tile font.
  text: (map: Hud, x: number, y: number, text: string, palette: number?) -> (),
  -- A number field `width` cells wide (1 to 9), right-aligned and padded with the map's padding; set
  -- redraws only the digits that changed. A field refuses align.
  field: (map: Hud, x: number, y: number, width: number, palette: number?) -> HudField,
  -- A text line `width` characters wide, blank until its first set; set redraws only the cells that changed.
  line: (map: Hud, x: number, y: number, width: number, palette: number?) -> HudLine,
}
-- A HUD number field, from hud.field.
export type HudField = {
  -- Shows a whole number that fits the field's width (a value too wide is an error).
  set: (self: HudField, value: number) -> (),
}
-- A HUD text line, from hud.line.
export type HudLine = {
  -- Shows the first `count` characters of text (all when nil), no wider than the line, aligned as its map
  -- says; the cells the last text covered beyond them are blanked. Raising count over the same text
  -- types it out, writing only the new cells.
  set: (self: HudLine, text: string, count: number?) -> (),
}

-- Deprecated: @ppu/pools. Keep actors that come and go in a fixed list of records with a live flag, made
-- once at file level (ppu docs actors-that-come-and-go-are-a-fixed-list). Existing games keep working, in the
-- preview and on a cartridge. Fixed pools of records reused without bookkeeping; a recycled record keeps its
-- old fields: reset them after spawn.
export type ppu_pool<T> = {
  -- Deprecated: a fixed list of records holds them itself. The pool's records, live or free, in the order
  -- they were made.
  items: {T},
  -- Deprecated: take the first record of a fixed list whose live flag is off. A free record, now live, or nil
  -- when every record is live.
  spawn: (self: ppu_pool<T>) -> T?,
  -- Deprecated: set the record's live flag off. Marks a live record free again; freeing twice is harmless.
  -- A record from another pool, or nil, is not checked: it errors or marks that record free.
  free: (self: ppu_pool<T>, x: T) -> (),
  -- Deprecated: loop over the fixed list and skip records that are not live. An iterator over the live
  -- records: for s in shots:live() do ... end. Freeing the current record
  -- inside the loop is fine.
  live: (self: ppu_pool<T>) -> () -> T?,
}
-- Deprecated: the @ppu/pools library's value (see ppu_pool).
export type ppu_pools = {
  -- Deprecated: fill a list with `for i = 1, n do list[i] = factory() end`. A pool of n records, each made
  -- once by factory(), all free. Made at file level:
  -- local shots = Pools.new(8, make_shot).
  new: <T>(n: number, factory: () -> T) -> ppu_pool<T>,
}

-- @ppu/sound: named sound effects, one song at a time, and the master volume. The sound driver picks each
-- effect's voice (see sfx); a scene switch restarts the sound chip, so each scene starts its own song in init.

-- Named sound effects, from sound.cues.
export type ppu_cues = {
  -- Plays the named cue: its preset at its note, on its voice or the one the sound driver picks. A cue with a priority plays only above the
  -- highest priority already played this update: two ships firing in one update key one shot.
  play: (self: ppu_cues, name: string) -> (),
}
-- Named songs, from sound.songs: one plays at a time.
export type ppu_songs = {
  -- Starts the named song, stopping whichever score plays (from any set or score{}).
  play: (self: ppu_songs, name: string) -> (),
  -- Stops every song.
  stop: (self: ppu_songs) -> (),
}
-- The @ppu/sound library's value.
export type ppu_sound = {
  -- Named sound effects, each {preset, note, priority?} (the sound driver picks the voice) or
  -- {preset, voice, note, priority?}, made at file level from a literal table.
  cues: (list: {[string]: {any}}) -> ppu_cues,
  -- Named songs, each a score handle (score{..., at = false}), made at file level; one plays at a time.
  songs: (list: {[string]: Song}) -> ppu_songs,
  -- Sets the master volume, 0 (silent) to 127, both sides.
  volume: (v: number) -> (),
}

-- @ppu/world: a Tiled map (.tmj project file) larger than the screen, streamed into backgrounds as the camera moves.
export type ppu_world = {
  -- Loads the Tiled map `name` at file level and returns its world. Options: radius (stream a map past
  -- 128x128 cells through the Mode 7 plane, r cells either side of the camera), view = {ahead, behind}
  -- (a perspective plane, with speed 1 to 8, default 8), period and pick (generated ground: the map is
  -- templates, and chunk (cx, cy) of ground `period` cells round shows template pick(cx, cy), numbered
  -- from 0), and chunk = {w, h} (a generated map on backgrounds: templates of 16 or 32 cells each). Any
  -- other option is an error.
  load: (name: string, options: {speed: number?, radius: number?, view: {ahead: number, behind: number}?, period: number?, pick: ((cx: number, cy: number) -> number)?, chunk: {number}?}?) -> World,
  -- Deprecated: math.hash under its old name.
  hash: (seed: number, x: number, y: number) -> number,
}
-- One object from the map's object layers: its name, Tiled type (class), position and size in map pixels,
-- and its custom properties by name.
export type WorldObject = {name: string, type: string, x: number, y: number, w: number, h: number, props: {[string]: any}}
-- w:bind's options for a background: parallax (the layer's scroll rate, a multiple of 1/16 from 1/16 to 1),
-- wrap (repeat the painted part), map (the background map's VRAM word address, a multiple of 0x400 below
-- 0x8000; else the map_base already set) and priority (every streamed cell's tile priority, 0 or 1).
export type WorldBindOptions = {parallax: number?, wrap: boolean?, map: number?, priority: number?}
-- A loaded map: its size in cells, the camera's range, its tile kinds and objects, and the methods
-- that bind, move and change it.
export type World = {
  name: string,
  width: number,
  height: number,
  -- The camera's stops on a background world: w:follow(x, y) takes x in 0..max_x and y in 0..max_y (pixels; 0 for a map no bigger than the screen).
  max_x: number,
  max_y: number,
  -- A view world's speed: the most px a frame on each axis its window keeps up with (a farther move redraws it whole).
  speed: number?,
  radius: number?,
  -- A view world (loaded with `view`): its ahead and behind, the cells either side of the camera that are always loaded, and the follows far rows may lag.
  ahead: number?,
  behind: number?,
  near: number?,
  catch_up: number?,
  -- A generated world (loaded with `period` and `pick`): its size in cells round, and how many templates its map holds.
  period: number?,
  templates: number?,
  -- Each Tiled tile class's number, by name, in order of first appearance; a tile with no class is 0.
  kinds: {[string]: number},
  -- The objects of the map w shows.
  objects: {WorldObject},
  -- Binds a tile layer to a background, w:bind("ground", bg[1], {map = 0x0000}), with its options; the
  -- background's tile_size must be 8. Each layer binds to its own background and one follow streams
  -- them all. A layer drawn from an m7 source binds to the plane instead, w:bind("track", m7) with no
  -- options, and loads whole; a map past 128x128 cells streams through it (load with radius). A bind
  -- starts the map as authored again.
  bind: (self: World, layer: string, background: Background | typeof(m7), options: WorldBindOptions?) -> (),
  -- Moves the camera to map pixel (x, y) and draws every row and column the move uncovers; call it once
  -- a frame. A view world takes the heading, in 256ths of a turn (0 up the plane), and only a view world
  -- does. {cut = true} (after the angle on a view world) redraws the whole window, a teleport.
  follow: (self: World, x: number, y: number, angle: (number | {cut: boolean?})?, options: {cut: boolean?}?) -> (),
  -- The kind (w.kinds number) of the tile under map pixel (x, y) on a layer, or 0 off the map: collision.
  tile: (self: World, layer: string, x: number, y: number) -> number,
  -- The Tiled tile under map pixel (x, y) becomes tile id `tile` of the layer's tileset (Tiled's ID, 0 first): its class for w:tile, redrawn now if on screen.
  set: (self: World, layer: string, x: number, y: number, tile: number) -> (),
  -- A hard cut, under force_blank, to another map the same scene (or program) loaded: same backgrounds,
  -- camera at (x, y). Afterwards the world's fields and w:tile are the new map's.
  enter: (self: World, name: string, x: number, y: number) -> (),
  -- The one object named `name` on the map w shows, w:find("start"): an error when no object or more
  -- than one has that name.
  find: (self: World, name: string) -> WorldObject,
}
