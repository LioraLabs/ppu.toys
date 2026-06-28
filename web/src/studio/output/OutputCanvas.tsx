import { useEffect, useRef, useState } from "react";
import { MockPpuCore } from "../../ppu/mock";
import { PpuCore, WIDTH, HEIGHT } from "../../ppu/core";
import {
  Clock,
  advanceClock,
  scrubToClock,
  clockToScrub,
  integerScale,
} from "./clock";

/** Right-column Output: blits the core's framebuffer to a native 256x224
 *  canvas (integer-upscaled, pixelated) and drives it with a play/pause +
 *  scrubber transport. Owns its own MockPpuCore (see issue notes). */
export function OutputCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const displayRef = useRef<HTMLDivElement>(null);
  const coreRef = useRef<PpuCore | null>(null);
  if (coreRef.current === null) coreRef.current = new MockPpuCore();
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const imageRef = useRef<ImageData | null>(null);
  const clockRef = useRef<Clock>({ t: 0, f: 0 });

  const [playing, setPlaying] = useState(true);
  // mirror of clockRef for rendering the label/handle (kept off the hot path)
  const [clock, setClock] = useState<Clock>({ t: 0, f: 0 });

  // paint the current clock's frame onto the canvas
  function renderFrame() {
    const ctx = ctxRef.current;
    const image = imageRef.current;
    if (!ctx || !image) return;
    const { framebuffer } = coreRef.current!.frame(
      clockRef.current.t,
      clockRef.current.f,
    );
    image.data.set(framebuffer);
    ctx.putImageData(image, 0, 0);
  }

  // one-time canvas setup + integer-scale sizing (ResizeObserver)
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = displayRef.current;
    if (!canvas || !container) return;
    ctxRef.current = canvas.getContext("2d");
    imageRef.current = new ImageData(WIDTH, HEIGHT);

    const resize = () => {
      const k = integerScale(container.clientWidth, container.clientHeight);
      canvas.style.width = `${WIDTH * k}px`;
      canvas.style.height = `${HEIGHT * k}px`;
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);

    renderFrame(); // show frame 0 immediately, even while paused
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // play/pause loop — advances by real elapsed time for stable 60fps
  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      clockRef.current = advanceClock(clockRef.current, now - last);
      last = now;
      renderFrame();
      setClock(clockRef.current);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing]);

  // scrub: set the clock and paint a single frame (works paused or playing)
  function onScrub(e: React.ChangeEvent<HTMLInputElement>) {
    clockRef.current = scrubToClock(Number(e.target.value));
    setClock(clockRef.current);
    renderFrame();
  }

  const scrub = clockToScrub(clock);

  return (
    <div className="output">
      <div className="output-header">
        <span className="section-header" style={{ padding: 0 }}>OUTPUT</span>
        <div className="tb-spacer" />
        <span className="pill">MODE 1</span>
        <span className="pill">256×224</span>
      </div>
      <div
        className="display"
        ref={displayRef}
        style={{ display: "grid", placeItems: "center" }}
      >
        <canvas
          ref={canvasRef}
          className="display-canvas"
          width={WIDTH}
          height={HEIGHT}
        />
        <span className="display-badge">mock-ppu</span>
      </div>
      <div className="transport">
        <button
          className="play-btn"
          aria-label={playing ? "Pause" : "Play"}
          onClick={() => setPlaying((p) => !p)}
        >
          {playing ? "⏸" : "▶"}
        </button>
        <div className="scrubber">
          <div className="scrubber-fill" style={{ width: `${scrub * 100}%` }} />
          <div
            className="scrubber-handle"
            style={{ left: `${scrub * 100}%` }}
          />
          <input
            type="range"
            min={0}
            max={1}
            step={0.001}
            value={scrub}
            onChange={onScrub}
            aria-label="Timeline scrubber"
            style={{
              position: "absolute",
              inset: 0,
              width: "100%",
              margin: 0,
              opacity: 0,
              cursor: "pointer",
            }}
          />
        </div>
        <span className="time">t={clock.t.toFixed(1)}s</span>
        <span className="fullscreen">⛶</span>
      </div>
    </div>
  );
}
