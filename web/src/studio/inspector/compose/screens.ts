import type { CompositorScreens, FrameResult } from "../../../ppu/core";
import { ppuCore } from "../../../ppu/instance";

const screensCache = new WeakMap<FrameResult, CompositorScreens>();

/** The core's compositor intermediates for a frame, fetched once per frame
 *  object no matter how many previews render it (tab + overlay). */
export function screensFor(frame: FrameResult): CompositorScreens {
  let s = screensCache.get(frame);
  if (!s) {
    s = ppuCore.screens();
    screensCache.set(frame, s);
  }
  return s;
}
