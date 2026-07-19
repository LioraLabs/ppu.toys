import { useSyncExternalStore } from "react";
import { ppuCore } from "../../ppu/instance";
import type {
  FrameResult, LuaError, PpuCore, SourceFile,
  SourceKind, ConvertSourceOptions, ConvertSourceResult,
} from "../../ppu/core";
import { advanceClock, scrubToClock, type Clock } from "../output/clock";
import { WIDTH, HEIGHT } from "../../ppu/core";

/** Power-on floor frame: black framebuffer, no registers (readers fall back to
 *  power-on defaults), empty OAM/CGRAM. NOT a mock PPU — it is the honest
 *  "no frame rendered yet" value, so `frame` keeps its non-null contract even
 *  when the transport is constructed before the core is live (the app never
 *  does — main.tsx boots the core first — but Cosmos evaluates the studio
 *  module graph eagerly, and safeFrame's "keep last good frame" catch needs a
 *  floor to keep). */
function powerOnFrame(): FrameResult {
  return {
    framebuffer: new Uint8ClampedArray(WIDTH * HEIGHT * 4),
    registers: [],
    cgram: new Uint16Array(256),
    oam: [],
    objOverflow: { rangeOver: false, timeOver: false, maxSprites: 0, maxTiles: 0 },
  };
}

export interface TransportState {
  t: number;
  f: number;
  playing: boolean;
  fps: number;
  frame: FrameResult;
  runtimeError?: LuaError;
}

function toLuaError(e: unknown): LuaError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as { message: unknown; line?: unknown; file?: unknown };
    return {
      message: String(o.message),
      line: typeof o.line === "number" ? o.line : undefined,
      file: typeof o.file === "string" ? o.file : undefined,
    };
  }
  return { message: String(e) };
}

function luaErrorEq(a: LuaError | undefined, b: LuaError | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return a.message === b.message && a.line === b.line && a.file === b.file;
}

/** ONE shared transport: owns the single rAF clock and drives the single shared
 *  ppuCore. Canvas, inspector, dock and status bar all read this via
 *  useTransport(); transport actions are the only writers to the core. */
export class Transport {
  private clock: Clock = { t: 0, f: 0 };
  private playing = true;
  private fps = 0;
  private runtimeError: LuaError | undefined;
  private frame: FrameResult;
  private snapshot: TransportState;
  private listeners = new Set<() => void>();
  private raf: number | null = null;
  private lastTs = 0;
  private fpsMs = 0;
  private fpsCount = 0;
  private lastSources: SourceFile[] | null = null;

  constructor(private coreRef: () => PpuCore = () => ppuCore) {
    this.frame = powerOnFrame(); // floor for safeFrame's catch on the very first call
    this.frame = this.safeFrame(0, 0);
    this.snapshot = this.build();
  }

  /** Run the core's frame() under guard: never throws, keeps last good frame,
   *  records/clears runtimeError with identity-stable references. */
  private safeFrame(t: number, f: number): FrameResult {
    try {
      const fr = this.coreRef().frame(t, f);
      this.setRuntimeError(undefined);
      return fr;
    } catch (e) {
      this.setRuntimeError(toLuaError(e));
      return this.frame; // keep last good frame; loop stays alive
    }
  }

  private setRuntimeError(next: LuaError | undefined) {
    if (!luaErrorEq(this.runtimeError, next)) this.runtimeError = next;
  }

  private build(): TransportState {
    return {
      t: this.clock.t,
      f: this.clock.f,
      playing: this.playing,
      fps: this.fps,
      frame: this.frame,
      runtimeError: this.runtimeError,
    };
  }

  private emit() {
    this.snapshot = this.build();
    for (const l of this.listeners) l();
  }

  /** Recompute the frame at the current clock and notify (paused-safe). */
  private renderOnce() {
    this.frame = this.safeFrame(this.clock.t, this.clock.f);
    this.emit();
  }

  /** Advance one animation step. rAF-free so it is unit-testable. */
  step(dtMs: number) {
    this.clock = advanceClock(this.clock, dtMs);
    this.fpsMs += Math.min(Math.max(dtMs, 0), 100);
    this.fpsCount += 1;
    if (this.fpsMs >= 250) {
      this.fps = Math.round((this.fpsCount * 1000) / this.fpsMs);
      this.fpsMs = 0;
      this.fpsCount = 0;
    }
    this.frame = this.safeFrame(this.clock.t, this.clock.f);
    this.emit();
  }

  private loop = (now: number) => {
    this.step(now - this.lastTs);
    this.lastTs = now;
    this.raf = requestAnimationFrame(this.loop);
  };

  private startLoop() {
    if (this.raf !== null || !this.playing || this.listeners.size === 0) return;
    this.lastTs = performance.now();
    this.raf = requestAnimationFrame(this.loop);
  }

  private stopLoop() {
    if (this.raf !== null) {
      cancelAnimationFrame(this.raf);
      this.raf = null;
    }
  }

  // external-store contract (stable bound refs for useSyncExternalStore)
  subscribe = (cb: () => void) => {
    this.listeners.add(cb);
    this.startLoop();
    return () => {
      this.listeners.delete(cb);
      if (this.listeners.size === 0) this.stopLoop();
    };
  };
  getSnapshot = () => this.snapshot;

  // actions
  setPlaying(p: boolean) {
    if (this.playing === p) return;
    this.playing = p;
    if (p) {
      this.startLoop();
    } else {
      this.stopLoop();
      this.fps = 0;
    }
    this.emit();
  }
  toggle = () => this.setPlaying(!this.playing);

  scrub(fraction: number) {
    this.clock = scrubToClock(fraction);
    this.renderOnce();
  }

  /** ▶ Run: deterministic restart — re-push the last sources so the core
   *  builds a fresh program, rewind the clock to t=0/f=0, resume playback. */
  restart = () => {
    if (this.lastSources !== null) this.coreRef().setSources(this.lastSources);
    this.clock = { t: 0, f: 0 };
    this.setPlaying(true);
    this.renderOnce();
  };

  setSources = (files: SourceFile[]): { ok: boolean; error?: LuaError } => {
    this.lastSources = files;
    const res = this.coreRef().setSources(files);
    this.renderOnce(); // re-render at the CURRENT clock — recompile never resets t/f
    return res;
  };

  setLayerVisible = (id: string, visible: boolean) => {
    this.coreRef().setLayerVisible(id, visible);
    this.renderOnce();
  };

  /** Pure quantize+pack (no engine mutation) — the Add Source dialog and
   *  the drop path use this to produce a payload. */
  convertSource = (kind: SourceKind, options: ConvertSourceOptions, image: ImageData): ConvertSourceResult => {
    return this.coreRef().convertSource(kind, options, image);
  };

  /** Register a source payload for rendering under `name`, then refresh the frame. */
  addSource = (name: string, payload: Uint8Array): { ok: boolean; error?: string } => {
    const res = this.coreRef().addSource(name, payload);
    this.renderOnce(); // registered source shows on the next rendered frame
    return res;
  };
}

export const transport = new Transport();

/** Subscribe a component to the shared transport snapshot. */
export function useTransport(): TransportState {
  return useSyncExternalStore(transport.subscribe, transport.getSnapshot);
}

/** Subscribe only to the transport's runtime error. Re-renders the consumer
 *  only when the error identity changes (the snapshot churns every frame, but
 *  runtimeError keeps a stable reference while unchanged). */
export function useTransportRuntimeError(): LuaError | undefined {
  return useSyncExternalStore(transport.subscribe, () => transport.getSnapshot().runtimeError);
}
