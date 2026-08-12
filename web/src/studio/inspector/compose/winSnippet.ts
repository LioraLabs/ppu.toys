/** Window `hdma()` codegen — the write side of per-scanline windows.
 *
 *  Pokes are frame-wide by construction (one line in `apply_pokes()`), so a
 *  swept window cannot be poked; it has to be Lua. The Mode-7 panel solved the
 *  same problem with a copyable perspective snippet, and this is the window
 *  twin: pick a shape, drag the params, paste the generated hook.
 *
 *  The generated code is deliberately in the friendly `win.*` dialect and reads
 *  like the bundled `spotlight` demo — that demo IS the iris shape, so a user
 *  who starts here and then opens spotlight sees the same idiom. */

export type WinShape = "iris" | "wipe" | "bars";

export const WIN_SHAPES: readonly { id: WinShape; label: string; hint: string }[] = [
  { id: "iris", label: "iris", hint: "circular spotlight — W1 spans each row's chord" },
  { id: "wipe", label: "wipe", hint: "diagonal edge — W1's left edge marches down the frame" },
  { id: "bars", label: "bars", hint: "horizontal bands — W1 alternates full width and empty" },
];

export interface WinShapeParams {
  /** Inclusive scanline range the hook covers. */
  y0: number;
  y1: number;
  /** iris: centre + radius. */
  cx: number;
  cy: number;
  r: number;
  /** wipe: x of the edge at `y0`, and columns gained per scanline. */
  x0: number;
  slope: number;
  /** bars: scanlines per band. */
  period: number;
}

export const WIN_SHAPE_DEFAULTS: WinShapeParams = {
  y0: 0,
  y1: 223,
  cx: 128,
  cy: 112,
  r: 70,
  x0: 64,
  slope: 0.5,
  period: 8,
};

/** Integers bare, fractions to 3 dp with trailing zeros trimmed (same rule as
 *  the M7 snippet, so the two panels emit numbers that look alike). */
const fmt = (v: number) =>
  Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");

/** An empty span: `lo > hi` makes the raw range test false for every column,
 *  which is how the hardware expresses "nothing inside" on this line. */
const EMPTY_SPAN = "    win.w1.lo, win.w1.hi = 1, 0   -- empty span (lo > hi) -> nothing inside";

function body(shape: WinShape, p: WinShapeParams): string[] {
  switch (shape) {
    case "iris":
      return [
        `  local dy = y - ${fmt(p.cy)}`,
        `  local inside = ${fmt(p.r)}*${fmt(p.r)} - dy*dy`,
        `  if inside < 0 then`,
        EMPTY_SPAN,
        `  else`,
        `    local hw = floor(sqrt(inside))`,
        `    win.w1.lo, win.w1.hi = ${fmt(p.cx)} - hw, ${fmt(p.cx)} + hw`,
        `  end`,
      ];
    case "wipe":
      return [
        `  local x = floor(${fmt(p.x0)} + (y - ${fmt(p.y0)}) * ${fmt(p.slope)})`,
        `  win.w1.lo, win.w1.hi = x, 255`,
      ];
    case "bars":
      return [
        `  if floor(y / ${fmt(p.period)}) % 2 == 0 then`,
        `    win.w1.lo, win.w1.hi = 0, 255`,
        `  else`,
        EMPTY_SPAN,
        `  end`,
      ];
  }
}

/** The pasteable hook. Mirrors `Mode7Panel`'s `snippet`: a header comment
 *  naming the panel and the call site, then the `hdma()` block. */
export function winSnippet(shape: WinShape, p: WinShapeParams): string {
  const label = WIN_SHAPES.find((s) => s.id === shape)?.label ?? shape;
  return [
    `-- Windows editor: ${label} (call inside frame(), after apply_pokes())`,
    `-- point a layer at window 1 first, e.g. win.color.w1 = true or win.bg1.w1 = true`,
    `hdma(${fmt(p.y0)}, ${fmt(p.y1)}, function(y)`,
    ...body(shape, p),
    `end)`,
  ].join("\n");
}
