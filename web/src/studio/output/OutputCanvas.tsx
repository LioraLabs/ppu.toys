import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { WIDTH, HEIGHT } from "../../ppu/core";
import { clockToScrub, integerScale } from "./clock";
import { transport, useTransport } from "../transport/transport";
import { Presenter } from "./presenter";
import { loadFx, saveFx, type PresentFx } from "./fx";
import { DropZone } from "./DropZone";

/** Right-column Output: presents the SHARED core's framebuffer through a WebGL
 *  present pass (integer upscale + toggleable CRT/scanline/pixel-grid FX) and
 *  drives the SHARED transport (play/pause + scrubber). No private core or clock. */
export function OutputCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const displayRef = useRef<HTMLDivElement>(null);
  const presenterRef = useRef<Presenter | null>(null);
  const [fx, setFx] = useState<PresentFx>(() => loadFx());
  const fxRef = useRef<PresentFx>(fx);
  // Once WebGL setup fails, the canvas is tainted to 'webgl' and can't give a 2D
  // context. Flip this to remount a FRESH canvas (key change) and re-init the
  // presenter in Canvas2D mode, so the framebuffer still shows (effects off).
  const [forceCanvas2d, setForceCanvas2d] = useState(false);

  const { t, f, playing, fps, frame } = useTransport();

  // init the presenter, integer-scale sizing + initial paint. Re-runs once if
  // WebGL fails and we fall back to a remounted Canvas2D canvas.
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = displayRef.current;
    if (!canvas || !container) return;
    const presenter = new Presenter();
    const ok = presenter.init(canvas, forceCanvas2d);
    presenterRef.current = presenter;

    // WebGL failed → this canvas is unusable for 2D. Drop it and remount fresh.
    if (!ok && !forceCanvas2d) {
      presenter.dispose();
      presenterRef.current = null;
      setForceCanvas2d(true);
      return;
    }

    const draw = () =>
      presenter.render(transport.getSnapshot().frame.framebuffer, fxRef.current);
    const resize = () => {
      presenter.resize(integerScale(container.clientWidth, container.clientHeight));
      draw();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);
    return () => {
      ro.disconnect();
      presenter.dispose();
      presenterRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [forceCanvas2d]);

  // repaint whenever the shared frame advances or the effect set changes
  useLayoutEffect(() => {
    fxRef.current = fx;
    presenterRef.current?.render(frame.framebuffer, fx);
  }, [frame, fx]);

  // persist effect toggles across sessions
  useEffect(() => {
    saveFx(fx);
  }, [fx]);

  const scrub = clockToScrub({ t, f });

  return (
    <div className="output">
      <div className="output-header">
        <span className="output-title">LIVE OUTPUT</span>
        <div className="tb-spacer" />
        <FxToggle label="CRT" on={fx.crt} onClick={() => setFx((s) => ({ ...s, crt: !s.crt }))} />
        <FxToggle label="SCAN" on={fx.scanline} onClick={() => setFx((s) => ({ ...s, scanline: !s.scanline }))} />
        <FxToggle label="GRID" on={fx.pixelGrid} onClick={() => setFx((s) => ({ ...s, pixelGrid: !s.pixelGrid }))} />
        <span className="pill">MODE 1</span>
        <span className="pill">256×224</span>
      </div>
      <div className="output-row">
        <div className="display" ref={displayRef}>
          <canvas
            ref={canvasRef}
            // key flips on WebGL failure to mount a pristine canvas for Canvas2D
            // (a canvas that once held a webgl context can't yield a 2D one).
            key={forceCanvas2d ? "canvas2d" : "webgl"}
            className="display-canvas"
            width={WIDTH}
            height={HEIGHT}
          />
          <span className="display-badge">
            {(forceCanvas2d ? "canvas" : "webgl") + " · wasm-ppu"}
          </span>
        </div>
        <div className="output-side">
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
          </div>
          <div className="readout">
            <span>t={t.toFixed(1)}s</span>
            <span>frame {f}</span>
            <span>{fps}fps</span>
          </div>
          <DropZone />
        </div>
      </div>
    </div>
  );
}

function FxToggle({ label, on, onClick }: { label: string; on: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      className={`fx-toggle${on ? " fx-toggle--active" : ""}`}
      aria-pressed={on}
      onClick={onClick}
    >
      {label}
    </button>
  );
}
