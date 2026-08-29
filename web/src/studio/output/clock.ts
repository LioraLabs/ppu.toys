import { WIDTH, HEIGHT } from "../../ppu/core";

/** Frames per second the timeline is quantized to (SNES NTSC ~= 60). */
export const FPS = 60;
/** Largest single-tick advance, ms — absorbs tab-refocus / breakpoint gaps. */
const MAX_DT_MS = 100;

/** Playback position. `t` seconds (float), `f` frame index, f = floor(t*FPS). */
export interface Clock {
  t: number;
  f: number;
}

function clock(t: number): Clock {
  return { t, f: Math.floor(t * FPS) };
}

/** Advance the clock by real elapsed wall-clock time (stable 60fps regardless
 *  of monitor refresh). */
export function advanceClock(c: Clock, dtMs: number): Clock {
  const dt = Math.min(Math.max(dtMs, 0), MAX_DT_MS) / 1000;
  return clock(c.t + dt);
}

/** Jump to an absolute time, clamped at the start of the toy. */
export function seekClock(seconds: number): Clock {
  return clock(Math.max(0, seconds));
}

/** Largest integer upscale of the native framebuffer that fits the container. */
export function integerScale(
  containerW: number,
  containerH: number,
  nativeW = WIDTH,
  nativeH = HEIGHT,
): number {
  const k = Math.floor(Math.min(containerW / nativeW, containerH / nativeH));
  return Math.max(1, k);
}
