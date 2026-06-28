import { useSyncExternalStore } from "react";
import { ppuCore } from "../../ppu/instance";
import type { FrameResult, LuaError } from "../../ppu/core";
import { advanceClock, scrubToClock, type Clock } from "../output/clock";

export interface TransportState {
  t: number;
  f: number;
  playing: boolean;
  fps: number;
  frame: FrameResult;
}

/** ONE shared transport: owns the single rAF clock and drives the single shared
 *  ppuCore. Canvas, inspector, dock and status bar all read this via
 *  useTransport(); transport actions are the only writers to the core. */
class Transport {
  private clock: Clock = { t: 0, f: 0 };
  private playing = true;
  private fps = 0;
  private frame: FrameResult = ppuCore.frame(0, 0);
  private snapshot: TransportState = this.build();
  private listeners = new Set<() => void>();
  private raf: number | null = null;
  private lastTs = 0;
  private fpsMs = 0;
  private fpsCount = 0;

  private build(): TransportState {
    return {
      t: this.clock.t,
      f: this.clock.f,
      playing: this.playing,
      fps: this.fps,
      frame: this.frame,
    };
  }

  private emit() {
    this.snapshot = this.build();
    for (const l of this.listeners) l();
  }

  /** Recompute the frame at the current clock and notify (paused-safe). */
  private renderOnce() {
    this.frame = ppuCore.frame(this.clock.t, this.clock.f);
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
    this.frame = ppuCore.frame(this.clock.t, this.clock.f);
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
    const res = ppuCore.setSource(src);
    this.renderOnce();
    return res;
  };

  setLayerVisible = (id: string, visible: boolean) => {
    ppuCore.setLayerVisible(id, visible);
    this.renderOnce();
  };

  uploadTexture = (slot: string, image: ImageData) => {
    ppuCore.uploadTexture(slot, image);
    this.renderOnce();
  };
}

export const transport = new Transport();

/** Subscribe a component to the shared transport snapshot. */
export function useTransport(): TransportState {
  return useSyncExternalStore(transport.subscribe, transport.getSnapshot);
}
