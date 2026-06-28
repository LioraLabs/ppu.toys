import { useSyncExternalStore } from "react";
import { ppuCore } from "../../ppu/instance";
import type { FrameResult, LuaError, PpuCore } from "../../ppu/core";
import { advanceClock, scrubToClock, type Clock } from "../output/clock";

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
    const o = e as { message: unknown; line?: unknown };
    return { message: String(o.message), line: typeof o.line === "number" ? o.line : undefined };
  }
  return { message: String(e) };
}

function luaErrorEq(a: LuaError | undefined, b: LuaError | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return a.message === b.message && a.line === b.line;
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

  constructor(private coreRef: () => PpuCore = () => ppuCore) {
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

  setSource = (src: string): { ok: boolean; error?: LuaError } => {
    const res = this.coreRef().setSource(src);
    this.renderOnce();
    return res;
  };

  setLayerVisible = (id: string, visible: boolean) => {
    this.coreRef().setLayerVisible(id, visible);
    this.renderOnce();
  };

  uploadTexture = (slot: string, image: ImageData) => {
    this.coreRef().uploadTexture(slot, image);
    this.renderOnce();
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
