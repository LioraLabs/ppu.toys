import { WIDTH, HEIGHT } from "../../ppu/core";

/** Frames per second the timeline is quantized to (SNES NTSC ~= 60). */
export const FPS = 60;
/** Length of the looping timeline, in seconds — bounds the scrubber domain. */
export const LOOP_SECONDS = 10;
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
 *  of monitor refresh), wrapping around the loop. */
export function advanceClock(c: Clock, dtMs: number): Clock {
  const dt = Math.min(Math.max(dtMs, 0), MAX_DT_MS) / 1000;
  return clock((c.t + dt) % LOOP_SECONDS);
}

/** Map a 0..1 scrubber fraction to a clock position on the loop. */
export function scrubToClock(fraction: number): Clock {
  const p = Math.min(1, Math.max(0, fraction));
  return clock(p * LOOP_SECONDS);
}

/** Inverse of scrubToClock: a clock's position as a 0..1 fraction. advanceClock
 *  already keeps t in [0, LOOP_SECONDS]; clamp (don't wrap) so the right edge
 *  maps to 1 rather than snapping the handle back to 0. */
export function clockToScrub(c: Clock): number {
  return Math.min(1, Math.max(0, c.t / LOOP_SECONDS));
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
