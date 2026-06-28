import { useEffect, useRef, useState } from "react";
import { ppuCore } from "../../ppu/instance";
import type { FrameResult } from "../../ppu/core";

/** Drives the SHARED ppuCore singleton with a light local rAF clock and returns
 *  the latest frame() output for the inspector tabs (registers / cgram).
 *
 *  NOTE (U8): this local clock is a stand-in. U8 replaces it with the shared
 *  transport clock so the OUTPUT canvas and this inspector advance off ONE clock
 *  instead of each running their own rAF. Keep this intentionally minimal. */
export function useInspectorFrame(): FrameResult | null {
  const [frame, setFrame] = useState<FrameResult | null>(null);
  const fRef = useRef(0);

  useEffect(() => {
    let raf = 0;
    const tick = () => {
      const f = fRef.current++;
      setFrame(ppuCore.frame(f / 60, f));
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return frame;
}
