import { useEffect, useState } from "react";
import { ppuCore } from "../../ppu/instance";
import { bgr555ToHex } from "./decode";
import { advanceClock, Clock } from "../output/clock";

/** CGRAM dock section: 16x16 grid of the 256 palette entries, decoded live from
 *  frame().cgram. Driven here by a local rAF clock; NOTE U8 replaces this with
 *  the shared transport clock so the dock and the Output canvas share one tick. */
export function CgramPalette() {
  const [cgram, setCgram] = useState<Uint16Array>(() => ppuCore.frame(0, 0).cgram);

  useEffect(() => {
    let raf = 0;
    let clock: Clock = { t: 0, f: 0 };
    let last = performance.now();
    const tick = (now: number) => {
      clock = advanceClock(clock, now - last);
      last = now;
      setCgram(ppuCore.frame(clock.t, clock.f).cgram);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="cgram-section">
      <div className="section-header">CGRAM</div>
      <div className="palette-grid">
        {Array.from(cgram, (v, idx) => (
          <div className="swatch" key={idx} style={{ background: bgr555ToHex(v) }} />
        ))}
      </div>
      <div className="palette-footer">
        <span>
          pal <span className="pal-num">0</span> · 16×16
        </span>
        <span className="bits">15-bit</span>
      </div>
    </div>
  );
}
