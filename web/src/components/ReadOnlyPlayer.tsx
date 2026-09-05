import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type Ref,
  type ReactNode,
} from "react";
import { WIDTH, HEIGHT } from "../ppu/core";
import { ppuCore } from "../ppu/instance";
import { transport, useTransport } from "../studio/transport/transport";
import { padKeyHandlers, PAD_HINT } from "../studio/transport/pad";
import { Presenter } from "../studio/output/presenter";
import { integerScale } from "../studio/output/clock";
import type { PresentFx } from "../studio/output/fx";
import "./player.css";
import { TouchController } from "./TouchController";

export interface PlayerSource {
  name: string;
  payload: Uint8Array;
}

let activeSourceNames = new Set<string>();

/** Read-only live player: pushes a published toy's program into the SHARED
 *  transport/core and presents its framebuffer through the same WebGL Presenter
 *  the Studio uses. No editor, no scrubber, no drop zone — pure playback. Both
 *  /studio and this route drive the single shared core; each pushes its program
 *  on mount, so navigating between them re-establishes the right render state. */
export function ReadOnlyPlayer({
  files,
  sources,
  raw = false,
  rawKeys = true,
  controls = false,
  children,
}: {
  files: { name: string; source: string }[];
  sources: PlayerSource[];
  raw?: boolean;
  rawKeys?: boolean;
  controls?: boolean;
  children?: ReactNode;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const displayRef = useRef<HTMLDivElement>(null);
  const presenterRef = useRef<Presenter | null>(null);
  const [forceCanvas2d, setForceCanvas2d] = useState(false);
  const [crt, setCrt] = useState(!raw);
  const crtRef = useRef(crt);
  const { playing, runtimeError } = useTransport();
  const input = useRef({ keys: 0, touch: 0 });
  const [pad] = useState(() =>
    padKeyHandlers((mask) => {
      input.current.keys = mask;
      transport.setPad(mask | input.current.touch);
    }),
  );
  const setTouch = useCallback((mask: number) => {
    input.current.touch = mask;
    transport.setPad(mask | input.current.keys);
  }, []);
  useEffect(() => {
    const clear = pad.onBlur;
    window.addEventListener("blur", clear);
    document.addEventListener("visibilitychange", clear);
    return () => {
      window.removeEventListener("blur", clear);
      document.removeEventListener("visibilitychange", clear);
      input.current = { keys: 0, touch: 0 };
      transport.setPad(0);
    };
  }, [pad]);

  useEffect(() => {
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      transport.setPlaying(false);
    }
  }, []);

  useEffect(() => {
    if (!raw || !rawKeys) return;
    const control = (event: KeyboardEvent) => {
      if (event.repeat) return;
      if (event.code === "Space") transport.toggle();
      else if (event.code === "KeyR") transport.seek(0);
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", control);
    return () => window.removeEventListener("keydown", control);
  }, [raw, rawKeys]);

  // Push the toy's program into the shared core: source payloads FIRST, then
  // the files — setSources runs the setup stage, whose dma() placements only
  // see sources already registered (same order as the Studio's restore path
  // and the core's own tests). Skip when no core is loaded (e.g. a fixture
  // never calls initCore) so the frame renders blank instead of throwing on
  // the unset singleton.
  useEffect(() => {
    if (!ppuCore) return;
    const nextNames = new Set(sources.map((source) => source.name));
    for (const name of activeSourceNames) if (!nextNames.has(name)) transport.removeSource(name);
    for (const s of sources) transport.addSource(s.name, s.payload);
    activeSourceNames = nextNames;
    transport.seek(0);
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
    presenterRef.current = presenter;
    const fx = (): PresentFx => ({ crt: crtRef.current, scanline: false, pixelGrid: false });
    const draw = () => presenter.render(transport.getSnapshot().frame.framebuffer, fx());
    const resize = () => {
      presenter.resize(raw ? 1 : integerScale(container.clientWidth, container.clientHeight));
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
      presenterRef.current = null;
    };
  }, [forceCanvas2d, raw]);

  // repaint on CRT toggle without re-initing the presenter
  useLayoutEffect(() => {
    crtRef.current = crt;
    presenterRef.current?.render(transport.getSnapshot().frame.framebuffer, {
      crt,
      scanline: false,
      pixelGrid: false,
    });
  }, [crt]);

  return (
    <div className={controls ? "player-console" : undefined}>
      <PlayerFrame
        displayRef={displayRef}
        canvasRef={canvasRef}
        canvasKey={forceCanvas2d ? "canvas2d" : "webgl"}
        playing={raw ? undefined : playing}
        onToggle={raw ? undefined : transport.toggle}
        crt={crt}
        // CRT is a WebGL pass; the Canvas2D fallback ignores fx, so hide the toggle there.
        onCrtToggle={raw || forceCanvas2d ? undefined : () => setCrt((v) => !v)}
        error={ppuCore ? runtimeError?.message : undefined}
        pad={pad}
        raw={raw}
      >
        {children}
      </PlayerFrame>
      {controls && (
        <TouchController onChange={setTouch}>
          <PlayerControls
            playing={playing}
            onToggle={transport.toggle}
            crt={crt}
            onCrtToggle={forceCanvas2d ? undefined : () => setCrt((v) => !v)}
          />
        </TouchController>
      )}
    </div>
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
  crt,
  onCrtToggle,
  error,
  pad,
  raw = false,
  children,
}: {
  displayRef?: Ref<HTMLDivElement>;
  canvasRef?: Ref<HTMLCanvasElement>;
  canvasKey?: string;
  playing?: boolean;
  onToggle?: () => void;
  crt?: boolean;
  onCrtToggle?: () => void;
  error?: string;
  /** Controller key handlers (pad.ts) — the frame becomes focusable and playable. */
  pad?: ReturnType<typeof padKeyHandlers>;
  raw?: boolean;
  children?: ReactNode;
}) {
  return (
    <div
      className={`player${raw ? " player--raw" : ""}`}
      ref={displayRef}
      title={pad && `Click to focus · ${PAD_HINT}`}
      {...pad}
    >
      <canvas
        ref={canvasRef}
        key={canvasKey}
        className="player-canvas"
        width={WIDTH}
        height={HEIGHT}
        role="img"
        aria-label="Live SNES toy output"
      />
      {children}
      <PlayerControls playing={playing} onToggle={onToggle} crt={crt} onCrtToggle={onCrtToggle} />
      {error && (
        <div className="player-error" role="alert">
          This toy stopped: {error}
        </div>
      )}
    </div>
  );
}

export function PlayerControls({
  playing,
  onToggle,
  crt,
  onCrtToggle,
}: {
  playing?: boolean;
  onToggle?: () => void;
  crt?: boolean;
  onCrtToggle?: () => void;
}) {
  return (
    <div className="player-controls">
      {onToggle && (
        <button type="button" className="player-toggle" onClick={onToggle}>
          {playing ? "Pause" : "Play"}
        </button>
      )}
      {onCrtToggle && (
        <button
          type="button"
          className="player-toggle player-crt"
          aria-pressed={crt}
          onClick={onCrtToggle}
        >
          CRT
        </button>
      )}
    </div>
  );
}
