import type { CompletionContext, CompletionResult, Completion } from "@codemirror/autocomplete";

/** Flat-global Lua DSL surface — audited against install_bindings() in
 *  crates/ppu-core/src/lua.rs as of M8. Details carry the SNES register. */
const GLOBALS: Completion[] = [
  { label: "mode", type: "variable", detail: "int 0..7 background mode · BGMODE $2105" },
  { label: "brightness", type: "variable", detail: "int 0..15 · INIDISP $2100" },
  { label: "mosaic", type: "variable", detail: "int 0..15 block size · MOSAIC $2106; enable per layer via bg[n].mosaic" },
  { label: "direct_color", type: "variable", detail: "bool 8bpp direct colour · CGWSEL.0 $2130" },
  { label: "force_blank", type: "variable", detail: "bool · INIDISP.7 $2100" },
  { label: "bg", type: "variable", detail: "bg[1..4] layers — scroll/map/char_base/tile_size/mosaic…" },
  { label: "m7", type: "variable", detail: "Mode 7 .a .b .c .d .cx .cy .extbg · $211A-$2120" },
  { label: "color", type: "variable", detail: "color math .op .half .on .addend .region .fixed · CGWSEL/CGADSUB/COLDATA $2130-$2132" },
  { label: "cgram", type: "variable", detail: "cgram[0..255] palette, 15-bit BGR · $2121/$2122" },
  { label: "vram", type: "variable", detail: "vram[0..0x7FFF] raw 16-bit words · $2116-$2119" },
  { label: "obj", type: "variable", detail: "obj[0..127] sprites; obj.sheet · OAM, OBSEL $2101" },
  { label: "TM", type: "variable", detail: "main screen designation bitmask · $212C" },
  { label: "TS", type: "variable", detail: "sub screen designation bitmask · $212D" },
  { label: "TMW", type: "variable", detail: "window mask enable, main · $212E" },
  { label: "TSW", type: "variable", detail: "window mask enable, sub · $212F" },
  { label: "WH0", type: "variable", detail: "window 1 left edge · $2126" },
  { label: "WH1", type: "variable", detail: "window 1 right edge · $2127" },
  { label: "WH2", type: "variable", detail: "window 2 left edge · $2128" },
  { label: "WH3", type: "variable", detail: "window 2 right edge · $2129" },
  { label: "W12SEL", type: "variable", detail: "window enable/invert BG1-2 · $2123" },
  { label: "W34SEL", type: "variable", detail: "window enable/invert BG3-4 · $2124" },
  { label: "WOBJSEL", type: "variable", detail: "window enable/invert OBJ+color · $2125" },
  { label: "WBGLOG", type: "variable", detail: "window combine logic, BG · $212A" },
  { label: "WOBJLOG", type: "variable", detail: "window combine logic, OBJ+color · $212B" },
  { label: "CGWSEL", type: "variable", detail: "color-math control A · $2130" },
  { label: "CGADSUB", type: "variable", detail: "color-math control B (add/sub, layers) · $2131" },
  { label: "COLDATA", type: "variable", detail: "fixed color, 15-bit · $2132" },
  { label: "coldata", type: "function", detail: "coldata(byte) authentic $2132 channel write" },
  { label: "hdma", type: "function", detail: "hdma(y0,y1,fn) per-scanline hook · HDMA" },
  { label: "scanline", type: "function", detail: "alias of hdma · HDMA" },
  { label: "m7pixel", type: "function", detail: "m7pixel(tile,x,y,index) Mode 7 char pixel (8bpp)" },
  { label: "rgb", type: "function", detail: "rgb(r,g,b) 0..255 -> 15-bit" },
  { label: "hsl", type: "function", detail: "hsl(h,s,l) -> 15-bit" },
  { label: "frame", type: "function", detail: "frame(t,f) — required entry point" },
  { label: "init", type: "function", detail: "init() — optional one-time setup" },
  { label: "apply_pokes", type: "function", detail: "apply_pokes() — inspector pokes (generated pokes.lua)" },
  { label: "t", type: "variable", detail: "seconds (float)" },
  { label: "f", type: "variable", detail: "frame index (int)" },
  { label: "math", type: "namespace", detail: "math.* library" },
  { label: "pi", type: "constant", detail: "math.pi as a flat global" },
  ...["sin", "cos", "tan", "floor", "ceil", "abs", "min", "max", "sqrt"].map(
    (n): Completion => ({ label: n, type: "function", detail: "math global" }),
  ),
];

/** math.* members (a subset is also aliased as bare globals above). */
const MATH_MEMBERS: Completion[] = [
  "sin", "cos", "tan", "asin", "acos", "atan", "floor", "ceil", "abs",
  "min", "max", "sqrt", "pi", "huge", "random", "fmod", "exp", "log",
].map((n): Completion => ({ label: n, type: n === "pi" || n === "huge" ? "constant" : "function" }));

/** bg[n].* layer members. */
const BG_MEMBERS: Completion[] = [
  { label: "scroll", type: "property", detail: ".x/.y · BGnHOFS/BGnVOFS $210D-$2114" },
  { label: "source", type: "property", detail: "asset id — quantized into tiles+map" },
  { label: "visible", type: "property", detail: "bool — playground layer toggle" },
  { label: "tile_size", type: "property", detail: "8 or 16 · BGMODE $2105" },
  { label: "map_base", type: "property", detail: "tilemap VRAM word addr · BGnSC $2107-$210A" },
  { label: "screen_size", type: "property", detail: "0..3 (32x32..64x64) · BGnSC" },
  { label: "char_base", type: "property", detail: "char VRAM word addr · BG12NBA/BG34NBA $210B/$210C" },
  { label: "mosaic", type: "property", detail: "bool per-layer enable · MOSAIC $2106" },
  { label: "map", type: "property", detail: "map[col][row] = {tile,pal,prio,flip_x,flip_y}" },
];

/** obj.* members (the sheet/OBSEL surface — NOT the per-sprite fields). */
const OBJ_MEMBERS: Completion[] = [
  { label: "sheet", type: "property", detail: "OBJ tile sheet asset id" },
  { label: "char_base", type: "property", detail: "OBJ char base (VRAM word addr) · OBSEL $2101" },
  { label: "size_sel", type: "property", detail: "size pair 0..7 (small/large WxH) · OBSEL $2101" },
  { label: "name_select", type: "property", detail: "name-select 0..3 (2nd table gap) · OBSEL $2101" },
  { label: "priority_rotate", type: "property", detail: "bool OAM priority rotation · OAMADDH.7" },
  { label: "oam_addr", type: "property", detail: "priority-rotation base sprite · OAMADD $2102" },
  { label: "first", type: "property", detail: "sugar: rotate priority from sprite N · OAMADD $2102" },
];

/** obj[n].* per-sprite members (OAM fields). */
const OBJ_SPRITE_MEMBERS: Completion[] = [
  { label: "x", type: "property", detail: "sprite x · OAM" },
  { label: "y", type: "property", detail: "sprite y · OAM" },
  { label: "tile", type: "property", detail: "char index 0..511 · OAM" },
  { label: "pal", type: "property", detail: "palette 0..7 (CGRAM 128+) · OAM" },
  { label: "prio", type: "property", detail: "priority 0..3 · OAM" },
  { label: "flip_x", type: "property", detail: "bool · OAM" },
  { label: "flip_y", type: "property", detail: "bool · OAM" },
  { label: "on", type: "property", detail: "bool — sprite enabled" },
  { label: "large", type: "property", detail: "bool — use the large OBSEL size" },
];

/** m7.* members. */
const M7_MEMBERS: Completion[] = [
  { label: "a", type: "property", detail: "affine matrix a · M7A $211B" },
  { label: "b", type: "property", detail: "affine matrix b · M7B $211C" },
  { label: "c", type: "property", detail: "affine matrix c · M7C $211D" },
  { label: "d", type: "property", detail: "affine matrix d · M7D $211E" },
  { label: "cx", type: "property", detail: "rotation center x · M7X $211F" },
  { label: "cy", type: "property", detail: "rotation center y · M7Y $2120" },
  { label: "wrap", type: "property", detail: "screen-over 0..3 · M7SEL $211A" },
  { label: "flip_x", type: "property", detail: "flip plane horizontally · M7SEL $211A" },
  { label: "flip_y", type: "property", detail: "flip plane vertically · M7SEL $211A" },
  { label: "extbg", type: "property", detail: "Mode 7 per-pixel priority (EXTBG) · SETINI.6 $2133" },
  { label: "map", type: "property", detail: "m7.map[ty][tx] = tile#" },
];

/** color.* members (friendly color-math namespace). */
const COLOR_MEMBERS: Completion[] = [
  { label: "op", type: "property", detail: '"add"|"sub" · CGADSUB.7 $2131' },
  { label: "half", type: "property", detail: "bool half result · CGADSUB.6 $2131" },
  { label: "on", type: "property", detail: ".bg1..bg4 .obj .backdrop math enables · CGADSUB.0-5 $2131" },
  { label: "addend", type: "property", detail: '"sub"|"fixed" math addend · CGWSEL.1 $2130' },
  { label: "region", type: "property", detail: '"everywhere"|"inside"|"outside"|"never" · CGWSEL.4-5 $2130' },
  { label: "fixed", type: "property", detail: "fixed color, 15-bit (rgb(...)) · COLDATA $2132" },
];

/** color.on.* per-layer math enables. */
const COLOR_ON_MEMBERS: Completion[] = [
  { label: "bg1", type: "property", detail: "bool math enable · CGADSUB.0 $2131" },
  { label: "bg2", type: "property", detail: "bool math enable · CGADSUB.1 $2131" },
  { label: "bg3", type: "property", detail: "bool math enable · CGADSUB.2 $2131" },
  { label: "bg4", type: "property", detail: "bool math enable · CGADSUB.3 $2131" },
  { label: "obj", type: "property", detail: "bool math enable · CGADSUB.4 $2131" },
  { label: "backdrop", type: "property", detail: "bool math enable · CGADSUB.5 $2131" },
];

function memberOptions(text: string): Completion[] {
  if (text.startsWith("bg")) return BG_MEMBERS;
  if (/^obj\s*\[/.test(text)) return OBJ_SPRITE_MEMBERS;
  if (text.startsWith("obj")) return OBJ_MEMBERS;
  if (text.startsWith("math")) return MATH_MEMBERS;
  if (/^color\s*\.\s*on/.test(text)) return COLOR_ON_MEMBERS;
  if (text.startsWith("color")) return COLOR_MEMBERS;
  return M7_MEMBERS;
}

export function ppuCompletions(ctx: CompletionContext): CompletionResult | null {
  // member access: `bg[1].` / `obj[0].` / `math.` / `obj.` / `m7.`
  // (optionally with a partial word after the dot)
  // the lookbehind anchors the base name: `myobj.` / `subbg[1].` must NOT
  // complete as obj/bg (nested-bracket indices like bg[t[1]] degrade to the
  // plain-globals path — acceptable)
  const member = ctx.matchBefore(
    /(?<![\w.\]])((?:bg|obj)\s*\[[^\]]*\]|math|obj|m7|color\s*\.\s*on|color)\s*\.\w*/,
  );
  if (member) {
    const from = member.from + member.text.lastIndexOf(".") + 1;
    return { from, options: memberOptions(member.text) };
  }

  const word = ctx.matchBefore(/\w+/);
  // No word before the cursor: only surface globals on an explicit request
  // (Ctrl-Space), never auto-pop on whitespace.
  if (!word) return ctx.explicit ? { from: ctx.pos, options: GLOBALS } : null;
  return { from: word.from, options: GLOBALS };
}
