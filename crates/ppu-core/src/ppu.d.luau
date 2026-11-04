-- ppu.d.luau: the globals a ppu.toys program sees, as Luau declarations for editors and type
-- checkers (for luau-lsp, list it in "luau-lsp.types.definitionFiles"). They declare the API and
-- do not implement it. `ppu new` and `ppu unpack` write this file beside a Luau toy, and the
-- engine test tests/luau_declarations.rs keeps it equal to the runtime's globals.

-- The program ------------------------------------------------------------------------------------
-- A toy defines `init()` (once, optional) and `frame(t, f)` (every video frame: t in seconds, f the
-- frame index). A Luau toy's files load each other with `require("./name")` (Luau's own require,
-- not declared here, so checkers resolve each module's type). `require("@ppu/name")` loads a
-- library the engine ships; see Libraries at the end.

-- Numbers with contracts. Each initializes a named binding or record field directly; later stores
-- are checked against the bounds (ordered integer literals) without clamping. makeBounded keeps
-- whole numbers. makeFixed keeps fractions in steps of 1/denominator (a power of two from 1
-- through 1024, default 1024) and turns on fixed-point arithmetic.
declare function makeBounded(value: number, minimum: number, maximum: number): number
declare function makeFixed(value: number, minimum: number, maximum: number, denominator: number?): number

-- Save data: one record kept in battery-backed RAM. Fields are makeBounded(default, min, max) with
-- literal bounds, or true/false; read and assign them as plain fields. A first power-on, a changed
-- record or a damaged save starts from the defaults.
declare function saveData<T>(fields: T): T

-- Precomputed tables. A grid gives each parameter of the inline function {min, max, step?}; calls
-- use the nearest grid point. lookup returns the function's results (1/1024 precision); bakeHdma
-- runs line(y, ...) on each scanline from first to last, as hdma does.
type SampleGrid = {[string]: {number}}
declare function lookup<R...>(grid: SampleGrid, evaluate: (...number) -> R...): (...number) -> R...
declare function bakeHdma(grid: SampleGrid, first: number, last: number, line: (number, ...number) -> ()): (...number) -> ()

-- Routines: a body that runs a little on each step() and pauses at yield(updates).
type Routine<A...> = {
  start: (Routine<A...>, A...) -> boolean,
  step: (Routine<A...>) -> (),
  cancel: (Routine<A...>) -> (),
  running: (Routine<A...>) -> boolean,
}
declare function routine<A...>(body: (A...) -> ()): Routine<A...>
declare function yield(updates: number?): ()

-- Display ----------------------------------------------------------------------------------------
type Layers = {bg1: boolean, bg2: boolean, bg3: boolean, bg4: boolean, obj: boolean, backdrop: boolean}
type MapEntry = {tile: number, pal: number?, prio: number?, flip_x: boolean?, flip_y: boolean?}
type Background = {
  scroll: {x: number, y: number},
  visible: boolean,
  tile_size: number,
  map_base: number,
  screen_size: number,
  char_base: number,
  mosaic: boolean,
  map: {[number]: {[number]: MapEntry}},
}
type Sprite = {
  x: number, y: number, tile: number, pal: number, prio: number,
  on: boolean, large: boolean, flip_x: boolean, flip_y: boolean,
}
type WindowEdges = {lo: number, hi: number}
type WindowSelection = {
  w1: boolean, w2: boolean, invert: boolean,
  combine: "OR" | "AND" | "XOR" | "XNOR",
  main: boolean, sub: boolean,
}

declare mode: number
declare brightness: number
declare force_blank: boolean
declare mosaic: number
declare direct_color: boolean
declare bg3_priority: boolean
declare bg: {[number]: Background}
declare obj: {
  [number]: Sprite,
  char_base: number, size_sel: number, name_select: number,
  priority_rotate: boolean, oam_addr: number, first: number,
}
declare m7: {
  a: number, b: number, c: number, d: number, cx: number, cy: number,
  wrap: number, flip_x: boolean, flip_y: boolean, extbg: boolean,
  map: {[number]: {[number]: number}},
}
declare screen: {main: Layers, sub: Layers}
declare color: {
  fixed: number, on: Layers, op: "add" | "sub", half: boolean,
  addend: "fixed" | "sub",
  region: "everywhere" | "inside" | "outside" | "never",
}
declare win: {
  w1: WindowEdges, w2: WindowEdges,
  bg1: WindowSelection, bg2: WindowSelection, bg3: WindowSelection, bg4: WindowSelection,
  obj: WindowSelection, color: WindowSelection,
}
declare vram: {[number]: number}
declare cgram: {[number]: number}

-- Raw registers, as bytes.
declare TM: number
declare TS: number
declare TMW: number
declare TSW: number
declare WH0: number
declare WH1: number
declare WH2: number
declare WH3: number
declare W12SEL: number
declare W34SEL: number
declare WOBJSEL: number
declare WBGLOG: number
declare WOBJLOG: number
declare CGWSEL: number
declare CGADSUB: number
declare COLDATA: number
declare function coldata(byte: number): ()

-- Per-scanline changes: line(y) runs for each scanline first..last, and its writes change that line.
declare function hdma(first: number, last: number, line: (number) -> ()): ()
declare function scanline(first: number, last: number, line: (number) -> ()): ()

-- Graphics sources: dma places a named source and returns where it went.
type Placement = {char: number?, map: number?, pal: number?, addr: number?}
type Cell = {tile: number, pal: number, flip_x: boolean, flip_y: boolean}
type Graphics = {
  char: number, map: number, pal: number, tile: number, cells: {Cell},
  tiles_w: number, tiles_h: number, next_char: number, next_map: number, next_pal: number,
  id: number, addr: number, next_addr: number, cell_w: number?, cell_h: number?,
}
declare function dma(source: string, placement: Placement?): Graphics
declare function m7pixel(tile: number, x: number, y: number, index: number): ()

-- Colours and Mode 7 views.
declare function rgb(red: number, green: number, blue: number): number
declare function hsl(hue: number, saturation: number, lightness: number): number
type Mode7View = {
  a: number, b: number, c: number, d: number, cx: number, cy: number,
  scroll_x: number, scroll_y: number, visible: boolean,
}
declare function mode7_transform(angle: number, zoom: number, x: number, y: number): Mode7View
declare function mode7_floor(y: number, horizon: number, height: number, heading: number, x: number, z: number): Mode7View

-- The sprites library: art made of a sheet's cells, drawn in groups (the first made on top). A level may take its
-- cells from a sheet of its own; a draw may flip the art and pick one palette for all of it.
-- SpriteLevel is exported, so a module that keeps a table of levels can type it as {SpriteLevel}.
export type SpriteLevel = {size: number, columns: number, cells: {number}, sheet: SpriteSheet?}
type SpriteArt = {}
type SpriteOptions = {size: number?, prio: number?, key: number?, depth: number?, flip_x: boolean?, flip_y: boolean?, pal: number?}
type SpriteGroup = {
  draw: (self: SpriteGroup, art: SpriteArt, x: number, y: number, options: SpriteOptions?) -> (),
  keep: (self: SpriteGroup, key: number) -> (),
}
type SpriteSheet = {tile: number, pal: number, cells: {Cell}, cell_w: number?, cell_h: number?}
declare sprites: {
  flicker: boolean,
  art: (levels: {SpriteLevel}, sheet: SpriteSheet) -> SpriteArt,
  group: (options: {sort: boolean?, slots: number?}?) -> SpriteGroup,
  draw: (art: SpriteArt, x: number, y: number, options: SpriteOptions?) -> (),
  keep: (key: number) -> (),
  hold: () -> (),
}

-- Scenes: a module returning {init, frame}, registered at file level with scenes.add(require("./title")).
-- scenes.go(scene) enters it with a cold boot on the next update; its t and f count from 0 there.
export type SceneModule = {init: (() -> ())?, frame: (t: number, f: number) -> ()}
type Scene = number
declare scenes: {
  add: (scene: SceneModule) -> Scene,
  go: (scene: Scene) -> (),
}

-- HUD text on a tile map: zero-based cells, font tiles from the space character at tile zero.
-- hud_text, hud_number and hud_clear are deprecated: use @ppu/hud (hud.text, hud.field, hud.line). They run in
-- the preview only; a cartridge build refuses them.
-- palette (a number), padding (" " or "0") and wrap (a boolean) are optional: typed `any` so a
-- descriptor table fits with or without them (table fields are invariant in Luau).
type Hud = {map: number, width: number, height: number, palette: any, padding: any, wrap: any}
declare function hud_text(map: Hud, x: number, y: number, text: string, palette: number?): ()
declare function hud_number(map: Hud, x: number, y: number, value: number, width: number, palette: number?): ()
declare function hud_clear(map: Hud, x: number?, y: number?, width: number?, height: number?): ()

-- Input and save data.
type Pad = {
  up: boolean, down: boolean, left: boolean, right: boolean,
  a: boolean, b: boolean, x: boolean, y: boolean, l: boolean, r: boolean,
  start: boolean, select: boolean,
}
declare pad: Pad
declare pad2: Pad
declare sram: {[any]: any}

-- Sound ------------------------------------------------------------------------------------------
type Voice = {
  sample: number, pitch: number, vol: {l: number, r: number},
  adsr: {a: number, d: number, s: number, r: number},
  gain: number?, noise: boolean, pmod: boolean, echo: boolean,
  envx: number, outx: number, ended: boolean,
}
type Instrument = {
  sample: number?, adsr: {a: number, d: number, s: number, r: number}?, gain: number?,
  vol: number?, pan: number?, base: (string | number)?, pitch: number?,
  noise: boolean?, pmod: boolean?, echo: boolean?, addr: number?, next_addr: number?,
}
type Song = {
  tick: number, length: number, playing: boolean, loop: (boolean | number)?, song: string,
  play: () -> (), stop: () -> (),
}
declare voice: {[number]: Voice}
declare dsp: {
  mvol: {l: number, r: number}, evol: {l: number, r: number},
  echo: {delay: number, feedback: number, fir: {number}},
  noise_clock: number, mute: boolean,
}
declare aram: {[number]: number}
declare BANK: {[string]: Instrument}
declare function kon(...: number): ()
declare function koff(...: number): ()
declare function note(name: string | number, base: (string | number)?): number
declare function instrument(preset: Instrument): Instrument
declare function bank(preset: string, options: Instrument?): Instrument
declare function sfx(instrument: Instrument, voice: number, note: (string | number)?): ()
declare function score(options: {song: string, loop: (boolean | number)?, at: (number | string | boolean)?}): Song
declare function timer(n: number, divider: number, tick: (number, number) -> ()): ()

-- Maths ------------------------------------------------------------------------------------------
declare pi: number
declare function sin(x: number): number
declare function cos(x: number): number
declare function tan(x: number): number
declare function floor(x: number): number
declare function ceil(x: number): number
declare function abs(x: number): number
declare function sqrt(x: number): number
declare function min(x: number, ...: number): number
declare function max(x: number, ...: number): number
declare function clamp(value: number, minimum: number, maximum: number): number
declare function lerp(a: number, b: number, t: number): number
declare function lerp_color(a: number, b: number, t: number): number
type Easing = "linear" | "smooth" | "ease-in" | "ease-out" | "step"
type RampKeys = {{any}}
declare function ease(progress: number, easing: Easing?): number
declare function ramp(keys: RampKeys, position: number): number
declare function ramp_int(keys: RampKeys, position: number): number
declare function ramp_color(keys: RampKeys, position: number): number

-- Libraries --------------------------------------------------------------------------------------
-- require("@ppu/name") returns the engine library lib/name.luau: one copy shipped with the engine,
-- the same for every toy, built into the cartridge from the same source. Each library's value is
-- typed `ppu_<name>` in its own block below (a checker without the engine's aliases can annotate:
-- local blink: ppu_blink = require("@ppu/blink")). The Studio completes `name.` from these blocks:
-- keep one field per line.

-- @ppu/blink: on/off timing for flashing text and invulnerable sprites.
type ppu_blink = {
  on: (f: number, half: number) -> boolean,
}

-- @ppu/hud: a retained background HUD; static text is written once, a field or line redraws only what changed.
type ppu_hud = {
  text: (map: Hud, x: number, y: number, text: string, palette: number?) -> (),
  field: (map: Hud, x: number, y: number, width: number, palette: number?) -> HudField,
  line: (map: Hud, x: number, y: number, width: number, palette: number?) -> HudLine,
}
type HudField = {set: (self: HudField, value: number) -> ()}
type HudLine = {set: (self: HudLine, text: string) -> ()}

-- @ppu/pools: fixed pools of records; spawn returns a free record (nil when all are live), free releases it.
type ppu_pool<T> = {
  items: {T},
  spawn: (self: ppu_pool<T>) -> T?,
  free: (self: ppu_pool<T>, x: T) -> (),
  live: (self: ppu_pool<T>) -> () -> T?,
}
type ppu_pools = {
  new: <T>(n: number, factory: () -> T) -> ppu_pool<T>,
}
