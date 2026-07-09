import type { CompletionContext, CompletionResult, Completion } from "@codemirror/autocomplete";

/** Flat-global Lua DSL surface (locked project spec). */
const GLOBALS: Completion[] = [
  { label: "mode", type: "variable", detail: "int 0..7 — background mode" },
  { label: "brightness", type: "variable", detail: "int 0..15 — INIDISP" },
  { label: "bg", type: "variable", detail: "bg[1..4].scroll/.source/.visible" },
  { label: "m7", type: "variable", detail: "Mode 7 affine .a .b .c .d .cx .cy" },
  { label: "cgram", type: "variable", detail: "cgram[0..255] palette" },
  { label: "obj", type: "variable", detail: "obj[0..127] sprites; obj.sheet" },
  { label: "hdma", type: "function", detail: "hdma(y0,y1,fn) per-scanline hook" },
  { label: "scanline", type: "function", detail: "alias of hdma" },
  { label: "rgb", type: "function", detail: "rgb(r,g,b) 0..255 -> 15-bit" },
  { label: "hsl", type: "function", detail: "hsl(h,s,l) -> 15-bit" },
  { label: "frame", type: "function", detail: "frame(t,f) — required entry point" },
  { label: "init", type: "function", detail: "init() — optional one-time setup" },
  { label: "t", type: "variable", detail: "seconds (float)" },
  { label: "f", type: "variable", detail: "frame index (int)" },
  { label: "math", type: "namespace", detail: "math.* library" },
  ...["sin", "cos", "tan", "floor", "ceil", "abs", "min", "max", "sqrt", "pi"].map(
    (n): Completion => ({ label: n, type: "function", detail: "math global" }),
  ),
];

/** math.* members (also available as bare globals above). */
const MATH_MEMBERS: Completion[] = [
  "sin", "cos", "tan", "asin", "acos", "atan", "floor", "ceil", "abs",
  "min", "max", "sqrt", "pi", "huge", "random", "fmod", "exp", "log",
].map((n): Completion => ({ label: n, type: n === "pi" || n === "huge" ? "constant" : "function" }));

/** obj.* members. */
const OBJ_MEMBERS: Completion[] = [
  { label: "sheet", type: "property", detail: "OBJ tile sheet asset id" },
  { label: "char_base", type: "property", detail: "OBSEL char base (VRAM word addr)" },
  { label: "size_sel", type: "property", detail: "OBSEL size pair 0..7 (small/large WxH)" },
  { label: "name_select", type: "property", detail: "OBSEL name-select 0..3 (2nd table gap)" },
];

export function ppuCompletions(ctx: CompletionContext): CompletionResult | null {
  // member access: `math.` / `obj.` (optionally with a partial word after the dot)
  const member = ctx.matchBefore(/(math|obj)\.\w*/);
  if (member) {
    const dot = member.text.indexOf(".");
    const base = member.text.slice(0, dot);
    const from = member.from + dot + 1;
    const options = base === "math" ? MATH_MEMBERS : OBJ_MEMBERS;
    return { from, options };
  }

  const word = ctx.matchBefore(/\w+/);
  // No word before the cursor: only surface globals on an explicit request
  // (Ctrl-Space), never auto-pop on whitespace.
  if (!word) return ctx.explicit ? { from: ctx.pos, options: GLOBALS } : null;
  return { from: word.from, options: GLOBALS };
}
