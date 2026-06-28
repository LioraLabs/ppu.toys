import { useEffect, useLayoutEffect, useRef } from "react";
import { WIDTH, HEIGHT } from "../../ppu/core";
import { clockToScrub, integerScale } from "./clock";
import { transport, useTransport } from "../transport/transport";

/** Right-column Output: blits the SHARED core's framebuffer to a native
 *  256x224 canvas (integer-upscaled, pixelated) and drives the SHARED transport
 *  (play/pause + scrubber). No private core or clock. */
export function OutputCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const displayRef = useRef<HTMLDivElement>(null);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const imageRef = useRef<ImageData | null>(null);

  const { t, f, playing, frame } = useTransport();

  // one-time canvas setup + integer-scale sizing + initial paint
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = displayRef.current;
    if (!canvas || !container) return;
    const ctx = canvas.getContext("2d");
    const image = new ImageData(WIDTH, HEIGHT);
    ctxRef.current = ctx;
    imageRef.current = image;

    const resize = () => {
      const k = integerScale(container.clientWidth, container.clientHeight);
      canvas.style.width = `${WIDTH * k}px`;
      canvas.style.height = `${HEIGHT * k}px`;
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);

    if (ctx) {
      image.data.set(transport.getSnapshot().frame.framebuffer);
      ctx.putImageData(image, 0, 0);
    }
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // paint whenever the shared frame advances
  useLayoutEffect(() => {
    const ctx = ctxRef.current;
    const image = imageRef.current;
    if (!ctx || !image) return;
    image.data.set(frame.framebuffer);
    ctx.putImageData(image, 0, 0);
  }, [frame]);

  const scrub = clockToScrub({ t, f });

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
          onClick={() => transport.toggle()}
        >
          {playing ? "⏸" : "▶"}
        </button>
        <div className="scrubber">
          <div className="scrubber-fill" style={{ width: `${scrub * 100}%` }} />
          <div className="scrubber-handle" style={{ left: `${scrub * 100}%` }} />
          <input
            type="range"
            min={0}
            max={1}
            step={0.001}
            value={scrub}
            onChange={(e) => transport.scrub(Number(e.target.value))}
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
        <span className="time">t={t.toFixed(1)}s</span>
        <span className="fullscreen">⛶</span>
      </div>
    </div>
  );
}
