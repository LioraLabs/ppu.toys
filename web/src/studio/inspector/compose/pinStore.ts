import { useSyncExternalStore } from "react";
import type { CompositorScreens, FrameResult, PinnedRegister } from "../../../ppu/core";
import { ppuCore } from "../../../ppu/instance";
import { transport } from "../../transport/transport";
import type { RegWrite } from "./model";

/** The pinned-override glue shared by the Compose/Windows tabs AND the
 *  Compositor overlay. State lives in the core (the pin set) — this module
 *  only reads it store-shaped and nudges the shared transport so a write
 *  re-renders immediately even while paused (transport.step(0) re-runs the
 *  frame at the current clock). ▶ Run clears pins via transport.restart(). */

let cached: PinnedRegister[] = [];

/** Content-cached view of ppuCore.listPins(): keeps a stable array reference
 *  while the pin set is unchanged (useSyncExternalStore contract), swaps it
 *  when any pin differs — including pins cleared behind our back by restart. */
export function pinsSnapshot(): PinnedRegister[] {
  const fresh = ppuCore.listPins();
  const same =
    fresh.length === cached.length &&
    fresh.every((p, i) => p.addr === cached[i].addr && p.value === cached[i].value);
  if (!same) cached = fresh;
  return cached;
}

/** Subscribe a component to the pin set (re-evaluated on every transport tick). */
export function usePins(): PinnedRegister[] {
  return useSyncExternalStore(transport.subscribe, pinsSnapshot);
}

/** Pin one register override and re-render the frame (paused-safe). */
export function writePin(addr: number, value: number): void {
  ppuCore.pin(addr, value);
  transport.step(0);
}

/** Pin a batch (multi-register encodes like combine/area) in one re-render. */
export function writePins(writes: RegWrite[]): void {
  for (const w of writes) ppuCore.pin(w.addr, w.value);
  transport.step(0);
}

/** Drop one pin — the control falls back to the script-driven value. */
export function releasePin(addr: number): void {
  ppuCore.unpin(addr);
  transport.step(0);
}

/** The clear-all affordance. */
export function releaseAllPins(): void {
  ppuCore.clearPins();
  transport.step(0);
}

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
