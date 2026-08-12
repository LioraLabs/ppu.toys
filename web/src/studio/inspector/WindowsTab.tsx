import { useMemo, useState } from "react";
import { HEIGHT } from "../../ppu/core";
import { compositorWrite, useCompositor } from "./compose/useCompositor";
import type { FieldWrite } from "./compose/model";
import { makeSweep, readAt } from "./compose/winScanlines";
import {
  BoundCards,
  KeyframeTrack,
  LayerMaskRows,
  ScanlinePicker,
  WindowControls,
  WindowPreview,
  WindowReadout,
  WindowShapeTool,
  type ScanlineTrack,
} from "./compose/WindowSections";
import { usePokeDialect } from "./compose/dialect";
import { removeKeyframeAt, setKeyframeRange, usePokes } from "../pokes/pokeStore";
import { pokeKeyframes, pokeRange } from "../pokes/scanlinePokes";
import "./compose/compose.css";

/** WINDOWS — the two hardware window masks (W1/W2) and their combine logic,
 *  read PER SCANLINE. `rows` is the core's `winScanlines()` feed (11 bytes per
 *  line); the frame's own `registers` are scanline 0 only, so without the feed
 *  an `hdma()`-swept window is invisible here.
 *
 *  The scanline lens is the whole trick: `readAt` wraps the compositor's `read`
 *  so window registers come from the selected row and everything else falls
 *  through. Every section below is unchanged in how it reads state — it just
 *  now reports the scanline you picked.
 *
 *  Writes stay frame-wide (a poke is one line in `apply_pokes()`); per-scanline
 *  authoring is the generated `hdma()` hook in WindowShapeTool. Presentational:
 *  the feed is injected (wired: WindowsTabWired; stories: fixture bytes). */
export function WindowsTab({ rows }: { rows: Uint8Array }) {
  const base = useCompositor();
  // Mid-frame, not row 0: on a static window the two are identical, and on a
  // swept one row 0 is the least representative line there is (an iris is still
  // closed up there). It also puts the cursor where you can see it.
  const [y, setY] = useState(Math.floor(HEIGHT / 2));
  const w = useMemo(() => makeSweep(rows, y), [rows, y]);
  // The lens: same Compositor, reads pinned to scanline y — and writes bound to
  // it too, so in the scanline dialect every control keyframes the row you are
  // looking at instead of writing frame-wide.
  const c = useMemo(
    () => ({
      ...base,
      read: readAt(rows, y, base.read),
      write: (write: FieldWrite) => compositorWrite([write], y),
      writeMany: (writes: readonly FieldWrite[]) => compositorWrite(writes, y),
    }),
    [base, rows, y],
  );
  // The scanline pokes currently in pokes.lua, decoded for the keyframe track.
  const pokes = usePokes();
  const dialect = usePokeDialect();
  const tracks: ScanlineTrack[] = useMemo(
    () =>
      pokes.flatMap((p) => {
        const kf = pokeKeyframes(p);
        return kf ? [{ lvalue: p.lvalue, kf, range: pokeRange(p) }] : [];
      }),
    [pokes],
  );
  return (
    <div className="insp-scroll">
      <div className="winp-wrap">
        <WindowPreview c={c} w={w} onScanline={setY} />
        <div className="winp-caption">
          orange = W1 edges · cyan = W2 edges · click preview to pick a scanline and drag the
          nearest edge
        </div>
        <ScanlinePicker w={w} onScanline={setY} />
        <KeyframeTrack
          tracks={tracks}
          dialect={dialect}
          y={y}
          onScanline={setY}
          onRemove={removeKeyframeAt}
          onRange={setKeyframeRange}
        />
        <WindowControls c={c} />
        <BoundCards c={c} w={w} />
        <div className="cmp-ctl-label">PER-LAYER WINDOW MASK</div>
        <LayerMaskRows c={c} />
        <WindowReadout c={c} w={w} />
        <WindowShapeTool />
      </div>
    </div>
  );
}
