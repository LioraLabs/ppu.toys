import { useMemo, useRef } from "react";
import { ppuCore } from "../../ppu/instance";
import { useTransport } from "../transport/transport";
import { pokeMany, unpokeMany } from "../pokes/pokeStore";
import { Mode7Panel, M7_LVALUES, type Mode7ViewData } from "./Mode7Panel";

/** Wired container: feeds the M7 editor from the shared core — segments every
 *  frame (cheap: 224 rows of f32), the full plane image every ~30 frames
 *  (VRAM rarely changes mid-toy) — and routes edits into the poke store. */
export function Mode7PanelWired() {
  const { f } = useTransport();
  const mapRef = useRef<{ at: number; map: Uint8ClampedArray | null }>({ at: -1, map: null });

  const data: Mode7ViewData = useMemo(() => {
    if (mapRef.current.at < 0 || f - mapRef.current.at >= 30) {
      mapRef.current = { at: f, map: ppuCore.m7MapView() };
    }
    return { map: mapRef.current.map, segments: ppuCore.m7Scanlines() };
    // f advances every transport frame — exactly the refresh signal we want
  }, [f]);

  return (
    <Mode7Panel data={data} onPoke={pokeMany} onClearPokes={() => unpokeMany([...M7_LVALUES])} />
  );
}
