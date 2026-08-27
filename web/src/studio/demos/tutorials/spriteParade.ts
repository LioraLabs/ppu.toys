/** sprite-parade — tutorial 4/10 of the launch arc: sprites (OBJ) and OAM.
 *  The heavily commented Lua below IS the tutorial; the asset generators here
 *  are pure + node-safe RGBA, mirrored byte-for-byte by
 *  crates/ppu-core/tests/tutorial_sprite_parade.rs (edit both).
 *
 *  The sheet is 16 CELLS wide on purpose. The cell_size-8 OBJ importer
 *  reserves tile 0 as blank and numbers each new unique 8x8 cell in order, so
 *  with cell 0 left blank and every used cell unique (under flips — the
 *  importer dedups flipped tiles too), sheet cell N = OBJ tile N. And the OBJ
 *  name table is 16 tiles wide (a large sprite fetches base, base+1, base+16,
 *  base+17), so at 16 cells a 2x2 block of cells in the image IS one large
 *  16x16 sprite. tutorial_sprite_parade.rs pins the cell->tile map.
 */
import { demo } from "../kit";
import type { Demo, DemoAsset } from "../kit";

// ── the parade sheet ─────────────────────────────────────────────────────────
// 14 colours: everything fits one OBJ sub-palette, so every sprite is pal 0
// and palette 1 stays free for the Lua-authored gold shades.
const PARADE_PAL = [
  0x000000, //  0 = transparent, never placed
  0x101820, //  1 outline / near-black
  0x405068, //  2 steel
  0x98a8c0, //  3 light steel
  0xf0f0f0, //  4 white
  0xd03830, //  5 red
  0xf8a800, //  6 gold
  0x3878d0, //  7 blue
  0x48a048, //  8 green
  0xe870a8, //  9 pink
  0x805030, // 10 (a) brown
  0xf8d8b0, // 11 (b) skin
  0x6858b8, // 12 (c) purple
  0xf86800, // 13 (d) orange
  0x78e0e8, // 14 (e) cyan visor
];

/** Bunting pennant, triangle cut, colour C. */
const TRI = `
  11111111
  0CCCCCC0
  0CCCCCC0
  00CCCC00
  00CCCC00
  000CC000
  000CC000
  00000000
`;
/** Bunting pennant, swallowtail cut, colour C. */
const SWAL = `
  11111111
  0CCCCCC0
  0CCCCCC0
  0CCCCCC0
  0CC00CC0
  0C0000C0
  00000000
  00000000
`;

/** 24 8x8 cells in row-major SHEET order (16 cells/row): cell N is what
 *  `obj[i].tile = N` draws. Cell 0 is blank deliberately (see header); cells
 *  1..23 are all pairwise unique under flips so the numbering holds. Cells
 *  24..31 of the 16x2 image are never written -> they dedup onto blank tile 0. */
const PARADE_CELLS = [
  //  0 blank — pins "cell N = tile N" for everything after it
  `
    00000000
    00000000
    00000000
    00000000
    00000000
    00000000
    00000000
    00000000
  `,
  //  1 robot marcher, pose A (legs apart) — antenna off-centre so flips read
  `
    00060000
    00111100
    001ee200
    00122200
    02233220
    00233200
    00200200
    02200220
  `,
  //  2 robot marcher, pose B (legs together)
  `
    00060000
    00111100
    001ee200
    00122200
    00233200
    00233200
    00022000
    00022000
  `,
  //  3 drummer (blue shako, red drum)
  `
    00060000
    00777700
    001bb100
    07777770
    04444440
    0a5555a0
    0aaaaaa0
    01000100
  `,
  //  4 flag-bearer 16x16, top-left (cloth)
  `
    00000000
    00555555
    05555555
    05555665
    05555555
    00555555
    00005555
    00004444
  `,
  //  5 flag-bearer top-right (cloth end + pole)
  `
    00060000
    555a0000
    555a0000
    555a0000
    555a0000
    555a0000
    555a0000
    000a0000
  `,
  //  6 big robot 16x16, top-left (head)
  `
    00000000
    00011111
    00012222
    0001ee22
    00012222
    00011222
    00001111
    00000022
  `,
  //  7 big robot top-right
  `
    00600000
    11111000
    22222100
    ee221000
    22222100
    22221100
    11110000
    22000000
  `,
  //  8..15 bunting, alternating cuts so no two cells collide under dedup
  TRI.replace(/C/g, "5"), //  8 red triangle
  SWAL.replace(/C/g, "7"), //  9 blue swallowtail
  TRI.replace(/C/g, "6"), // 10 gold triangle
  SWAL.replace(/C/g, "8"), // 11 green swallowtail
  TRI.replace(/C/g, "9"), // 12 pink triangle
  SWAL.replace(/C/g, "c"), // 13 purple swallowtail
  TRI.replace(/C/g, "d"), // 14 orange triangle
  SWAL.replace(/C/g, "6"), // 15 gold swallowtail (differs from 10 by cut)
  // 16 balloon
  `
    00999900
    09499990
    09999990
    09999990
    00999900
    00090000
    00010000
    00001000
  `,
  // 17 confetti burst A
  `
    05500066
    05500066
    00000000
    000cc000
    000cc000
    09000000
    09000dd0
    00000dd0
  `,
  // 18 confetti burst B
  `
    00770000
    00770000
    00000990
    00000990
    00000000
    55000000
    55000dd0
    00000dd0
  `,
  // 19 gold sparkle
  `
    00066000
    00066000
    06666660
    06666660
    00066000
    00066000
    00000000
    00000000
  `,
  // 20 flag-bearer bottom-left (face + body): tile 4+16 — the name-table row
  `
    0004bb10
    00555550
    00555555
    00055550
    00011110
    00044440
    00040040
    00110110
  `,
  // 21 flag-bearer bottom-right (arm out to the pole, pole ends at the grip)
  `
    000a0000
    000a0000
    555a0000
    000a0000
    000a0000
    00000000
    00000000
    00000000
  `,
  // 22 big robot bottom-left (arm + chest): tile 6+16
  `
    00222222
    02233333
    02233366
    01133366
    00022222
    00022200
    00022200
    00122210
  `,
  // 23 big robot bottom-right
  `
    22222200
    33333220
    33333220
    33333110
    22222000
    00222000
    00222000
    00122210
  `,
].map((c) => c.replace(/\s/g, ""));

function paradeSheet(): DemoAsset {
  const cols = 16;
  const w = cols * 8,
    h = 16; // 16x2 cells; cells 24..31 stay transparent (-> blank tile 0)
  const data = new Uint8ClampedArray(w * h * 4);
  PARADE_CELLS.forEach((cell, n) => {
    const ox = (n % cols) * 8,
      oy = Math.floor(n / cols) * 8;
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        const idx = parseInt(cell[y * 8 + x], 16);
        if (!idx) continue; // index 0 -> transparent
        const c = PARADE_PAL[idx];
        const i = ((oy + y) * w + ox + x) * 4;
        data[i] = c >> 16;
        data[i + 1] = (c >> 8) & 0xff;
        data[i + 2] = c & 0xff;
        data[i + 3] = 255;
      }
    }
  });
  return { id: "parade", width: w, height: h, data, kind: "obj", options: { cell_size: 8 } };
}

// ── the street ───────────────────────────────────────────────────────────────
// One assembled BG import: transparent sky (backdrop shows through), a sun,
// clouds, the kerb + cobbles from y=180 down, and the picket fence the
// priority lesson marches through. The fence gaps are TRANSPARENT columns, so
// the prio-0 marcher shows in slivers between pickets.
const STREET_PAL = [
  0x000000, // 0 = transparent (sky)
  0xf0f0f8, // 1 picket white
  0x9898b8, // 2 picket shade / rails
  0x685878, // 3 cobble A
  0x585068, // 4 cobble B
  0xa898b0, // 5 kerb
  0xf8d060, // 6 sun
  0xf8e8a8, // 7 sun core
  0xd8c8e8, // 8 cloud (kept well off picket white: near-whites merge under nearest-match)
];

const GROUND = 180; // marchers' feet sit exactly on this row
const FENCE_X0 = 92,
  FENCE_X1 = 164; // 9 pickets, 8px period

/** Street palette index at (x, y) — integer math only, mirrored in Rust. */
function streetIndex(x: number, y: number): number {
  if (y >= GROUND) {
    if (y < GROUND + 4) return 5; // kerb
    return (Math.floor(x / 8) + Math.floor((y - GROUND) / 8)) % 2 === 0 ? 3 : 4; // cobbles
  }
  if (y === 4) return 2; // the bunting rope — sprite pennants hang off it
  if (x >= FENCE_X0 && x < FENCE_X1) {
    const fx = (x - FENCE_X0) % 8;
    if (y >= 156 && y < 160 && fx >= 1 && fx < 3) return 1; // picket tip
    if (y >= 160 && fx < 4) return fx === 3 ? 2 : 1; // picket (shaded edge)
    if (y >= 168 && y < 172) return 2; // one rail spanning the gaps
  }
  const sx = x - 204,
    sy = y - 44;
  if (sx * sx + sy * sy < 100) return 7; // sun core
  if (sx * sx + sy * sy < 256) return 6; // sun
  // puffy flat-bottomed clouds: three discs clipped at a straight base line
  const cloud = (cx: number, cy: number): boolean => {
    if (y > cy + 6) return false;
    const a = (x - cx + 13) * (x - cx + 13) + (y - cy) * (y - cy) < 81;
    const b = (x - cx) * (x - cx) + (y - cy + 7) * (y - cy + 7) < 144;
    const c = (x - cx - 13) * (x - cx - 13) + (y - cy) * (y - cy) < 64;
    return a || b || c;
  };
  if (cloud(56, 60) || cloud(150, 84) || cloud(224, 110)) return 8;
  return 0;
}

function street(): DemoAsset {
  const w = 256,
    h = 224;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const idx = streetIndex(x, y);
      if (!idx) continue;
      const c = STREET_PAL[idx];
      const i = (y * w + x) * 4;
      data[i] = c >> 16;
      data[i + 1] = (c >> 8) & 0xff;
      data[i + 2] = c & 0xff;
      data[i + 3] = 255;
    }
  }
  return { id: "street", width: w, height: h, data, kind: "bg", options: { bit_depth: 4 } };
}

// ── the Lua (verbatim in tutorial_sprite_parade.rs — edit both) ──────────────
const MAIN_SRC = `-- ppu.toys :: sprite-parade — tutorial 4/10: sprites (OBJ) and OAM
--
-- The SNES draws sprites out of OAM: 128 slots, each an x/y, a tile number,
-- a palette, a priority and two flip bits. Here that table is obj[0..127].
-- What this toy walks through, in reading order:
--   1. BINDING A SHEET   obj.sheet + obj.char_base put your art in OBJ VRAM
--   2. SIZES             obj.size_sel picks ONE small/large pair per frame;
--                        obj[i].large flips a single sprite to the large size
--   3. PLACING           obj[i].x/y/tile/pal/on — that is a sprite on screen
--   4. FLIPPING          obj[i].flip_x mirrors (the marcher walking back)
--   5. PALETTES          OBJ palettes live at CGRAM 128+, 16 entries each
--   6. PRIORITY          obj[i].prio 0..3 interleaves with the BG planes —
--                        watch the fence: prio 0 marches BEHIND it, prio 3 over
-- (tutorial 1 first-light covers frame(t,f); 2 parallax-skyline covers BG
--  layers; 5 cavern-camera covers tilesheets; 10 sprite-limits pushes OAM
--  until the hardware starts dropping sprites.)

SPEED = 32           -- parade pace, pixels per second

function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  cgram[0] = rgb(96, 64, 128)   -- backdrop = dusk sky (CGRAM entry 0)

  -- The street + picket fence: one assembled BG import. Its palette lands at
  -- CGRAM 0; sprites never touch it — OBJ palettes live in the 128+ half.
  bg[1].source = "street"
  -- Power-on defaults turn every layer on, and an unbound layer rasterizes
  -- whatever VRAM holds. Keep BG1 + OBJ, drop the rest (screen.main = TM).
  screen.main.bg2 = false; screen.main.bg3 = false; screen.main.bg4 = false

  -- 1. BIND THE SHEET. The cell_size-8 OBJ importer reserves tile 0 as blank
  -- and numbers each new unique 8x8 cell in order; this sheet leaves cell 0
  -- blank ON PURPOSE so sheet cell N = OBJ tile N from there on.
  obj.char_base = 0x6000        -- OBJ chars live apart from the BG chars
  obj.sheet = "parade"

  -- 2. SIZES. OBSEL holds one pair for the whole frame; each sprite picks
  -- its half of the pair with obj[i].large:
  --   size_sel  0: 8x8/16x16   1: 8/32   2: 8/64   3: 16/32   4: 16/64
  --             5: 32/64   6: 16x32/32x64   7: 16x32/32x32  <- 6+7 NON-square
  obj.size_sel = 0              -- small = 8x8, large = 16x16
  -- A large 16x16 sprite fetches tiles base, base+1, base+16, base+17: the
  -- OBJ name table is 16 tiles wide. The sheet is 16 CELLS wide for exactly
  -- that reason — a 2x2 block of cells in the image is one large sprite.

  -- 5. PALETTES. OBJ palette p starts at CGRAM 128 + p*16 (entry 0 of each
  -- is transparent). The import filled palette 0; palette 1 is ours — 15
  -- shades of gold, so one marcher parades as a statue of itself.
  for i = 1, 15 do cgram[128 + 16 + i] = rgb(96 + i * 10, 72 + i * 9, 8 + i * 3) end

  local function march(x0)      -- drift right, wrapping just off both edges
    return ((x0 + t * SPEED) % 272) - 16
  end
  local step = floor(t * 6)     -- walk cycle: poses 1 and 2 alternate
  local function pose(i) return 1 + (step + i) % 2 end
  local function bob(i) return -abs(sin(t * 6 + i)) * 2 end

  -- 3. PLACE THE PARADE. Feet on the kerb at y=180, so small 8x8 marchers
  -- stand at y 172 and large 16x16 ones at y 164. obj[i].on = true or the
  -- slot stays empty.
  obj[0].tile = 4; obj[0].large = true    -- flag-bearer: ONE tile number (4)
  obj[0].x = march(160); obj[0].y = 164 + bob(0)   -- addresses the 2x2 block
  obj[0].prio = 2; obj[0].on = true
  obj[1].tile = 6; obj[1].large = true    -- big robot, the other 2x2 block
  obj[1].x = march(32); obj[1].y = 164 + bob(1)
  obj[1].prio = 2; obj[1].on = true
  obj[2].tile = pose(0); obj[2].x = march(64)      -- two small robots,
  obj[2].y = 172 + bob(2); obj[2].prio = 2; obj[2].on = true
  obj[3].tile = pose(1); obj[3].x = march(188)     -- walking out of phase
  obj[3].y = 172 + bob(3); obj[3].prio = 2; obj[3].on = true

  -- 4. FLIP: same tiles, flip_x = true, marching the other way.
  obj[4].tile = pose(0); obj[4].flip_x = true
  obj[4].x = ((280 - t * SPEED) % 272) - 16
  obj[4].y = 172 + bob(4); obj[4].prio = 2; obj[4].on = true

  obj[5].tile = pose(1); obj[5].pal = 1   -- the gold marcher: same tiles,
  obj[5].x = march(-4); obj[5].y = 172 + bob(5)    -- palette 1 from above
  obj[5].prio = 2; obj[5].on = true
  obj[6].tile = 3; obj[6].x = march(16)   -- drummer
  obj[6].y = 172 + bob(6); obj[6].prio = 2; obj[6].on = true

  -- 6. PRIORITY. In mode 1 a prio-0 sprite sits UNDER the low-priority BG
  -- planes (an assembled import's tilemap priority bit is 0); prio 1+ sit
  -- over them, prio 3 over everything. Same tiles, one number apart:
  obj[7].tile = pose(0); obj[7].prio = 0  -- BEHIND the fence — slivers of it
  obj[7].x = march(104); obj[7].y = 172   -- show through the picket gaps
  obj[7].on = true
  obj[8].tile = pose(1); obj[8].prio = 3  -- in FRONT of the same fence
  obj[8].x = march(128); obj[8].y = 172
  obj[8].on = true

  -- Set dressing: bunting on slots 9..16 (tiles 8..15), drifting confetti,
  -- one escaped balloon. All plain small sprites.
  for k = 0, 7 do
    obj[9 + k].tile = 8 + k
    obj[9 + k].x = 12 + k * 32; obj[9 + k].y = 4
    obj[9 + k].prio = 3; obj[9 + k].on = true
  end
  for k = 0, 2 do
    obj[17 + k].tile = 17 + k
    obj[17 + k].x = 40 + k * 70; obj[17 + k].y = 36 + sin(t * 2 + k) * 12
    obj[17 + k].prio = 3; obj[17 + k].on = true
  end
  obj[20].tile = 16
  obj[20].x = 30 + sin(t * 3) * 3; obj[20].y = 132 - t * 16
  obj[20].prio = 2; obj[20].on = true

  -- Try: set obj.size_sel = 3 and watch every small marcher double in size;
  -- make the flag-bearer bob twice as hard; give obj[8] pal = 1 and prio = 0
  -- so the gold statue is the one stuck behind the fence.
end
`;

export const spriteParade: Demo = demo(
  "sprite-parade",
  "sprite-parade",
  [{ name: "main.lua", source: MAIN_SRC }],
  [paradeSheet(), street()],
);
