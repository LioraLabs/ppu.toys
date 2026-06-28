import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { WIDTH, HEIGHT } from "../../ppu/core";
import { clockToScrub, integerScale } from "./clock";
import { transport, useTransport } from "../transport/transport";
import { Presenter } from "./presenter";
import { loadFx, saveFx, type PresentFx } from "./fx";

/** Right-column Output: presents the SHARED core's framebuffer through a WebGL
 *  present pass (integer upscale + toggleable CRT/scanline/pixel-grid FX) and
 *  drives the SHARED transport (play/pause + scrubber). No private core or clock. */
export function OutputCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const displayRef = useRef<HTMLDivElement>(null);
  const presenterRef = useRef<Presenter | null>(null);
  const [fx, setFx] = useState<PresentFx>(() => loadFx());
  const fxRef = useRef<PresentFx>(fx);

  const { t, f, playing, frame } = useTransport();

  // one-time: init the presenter, integer-scale sizing + initial paint
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = displayRef.current;
    if (!canvas || !container) return;
    const presenter = new Presenter();
    presenter.init(canvas);
    presenterRef.current = presenter;

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
  }, []);

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
        <span className="section-header" style={{ padding: 0 }}>OUTPUT</span>
        <div className="tb-spacer" />
        <FxToggle label="CRT" on={fx.crt} onClick={() => setFx((s) => ({ ...s, crt: !s.crt }))} />
        <FxToggle label="SCAN" on={fx.scanline} onClick={() => setFx((s) => ({ ...s, scanline: !s.scanline }))} />
        <FxToggle label="GRID" on={fx.pixelGrid} onClick={() => setFx((s) => ({ ...s, pixelGrid: !s.pixelGrid }))} />
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
