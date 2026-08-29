import { useEffect, useRef, useState, type Ref } from "react";
import { WIDTH, HEIGHT } from "../ppu/core";
import { ppuCore } from "../ppu/instance";
import { transport, useTransport } from "../studio/transport/transport";
import { Presenter } from "../studio/output/presenter";
import { integerScale } from "../studio/output/clock";
import type { PresentFx } from "../studio/output/fx";
import "./player.css";

export interface PlayerSource {
  name: string;
  payload: Uint8Array;
}

/** Read-only live player: pushes a published toy's program into the SHARED
 *  transport/core and presents its framebuffer through the same WebGL Presenter
 *  the Studio uses. No editor, no scrubber, no drop zone — pure playback. Both
 *  /studio and this route drive the single shared core; each pushes its program
 *  on mount, so navigating between them re-establishes the right render state. */
export function ReadOnlyPlayer({
  files,
  sources,
}: {
  files: { name: string; source: string }[];
  sources: PlayerSource[];
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const displayRef = useRef<HTMLDivElement>(null);
  const [forceCanvas2d, setForceCanvas2d] = useState(false);
  const { playing, runtimeError } = useTransport();

  useEffect(() => {
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      transport.setPlaying(false);
    }
  }, []);

  // Push the toy's program into the shared core: source payloads FIRST, then
  // the files — setSources runs the setup stage, whose dma() placements only
  // see sources already registered (same order as the Studio's restore path
  // and the core's own tests). Skip when no core is loaded (e.g. a fixture
  // never calls initCore) so the frame renders blank instead of throwing on
  // the unset singleton.
  useEffect(() => {
    if (!ppuCore) return;
    for (const s of sources) transport.addSource(s.name, s.payload);
    transport.setSources(files);
  }, [files, sources]);

  // Init the presenter, size to the container, paint the shared frame; repaint
  // as the shared transport advances.
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = displayRef.current;
    if (!canvas || !container || !ppuCore) return;
    const presenter = new Presenter();
    const ok = presenter.init(canvas, forceCanvas2d);
    if (!ok && !forceCanvas2d) {
      presenter.dispose();
      setForceCanvas2d(true);
      return;
    }
    const fx: PresentFx = { crt: false, scanline: false, pixelGrid: false };
    const draw = () => presenter.render(transport.getSnapshot().frame.framebuffer, fx);
    const resize = () => {
      presenter.resize(integerScale(container.clientWidth, container.clientHeight));
      draw();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);
    const unsub = transport.subscribe(draw);
    return () => {
      ro.disconnect();
      unsub();
      presenter.dispose();
    };
  }, [forceCanvas2d]);

  return (
    <PlayerFrame
      displayRef={displayRef}
      canvasRef={canvasRef}
      canvasKey={forceCanvas2d ? "canvas2d" : "webgl"}
      playing={playing}
      onToggle={transport.toggle}
      error={ppuCore ? runtimeError?.message : undefined}
    />
  );
}

/** Presentational player frame: the black letterbox + the native-res pixelated
 *  canvas, nothing else. Pure markup with no transport/core coupling, so it can
 *  be storied and screenshotted without booting wasm. The wired `ReadOnlyPlayer`
 *  drives it via refs; a story can paint the canvas through a callback ref. */
export function PlayerFrame({
  displayRef,
  canvasRef,
  canvasKey,
  playing,
  onToggle,
  error,
}: {
  displayRef?: Ref<HTMLDivElement>;
  canvasRef?: Ref<HTMLCanvasElement>;
  canvasKey?: string;
  playing?: boolean;
  onToggle?: () => void;
  error?: string;
}) {
  return (
    <div className="player" ref={displayRef}>
      <canvas
        ref={canvasRef}
        key={canvasKey}
        className="player-canvas"
        width={WIDTH}
        height={HEIGHT}
        role="img"
        aria-label="Live SNES toy output"
      />
      {onToggle && (
        <button type="button" className="player-toggle" onClick={onToggle}>
          {playing ? "Pause" : "Play"}
        </button>
      )}
      {error && (
        <div className="player-error" role="alert">
          This toy stopped: {error}
        </div>
      )}
    </div>
  );
}
