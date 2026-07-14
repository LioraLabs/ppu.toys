import type { PpuCore } from "./core";

/** The single shared PpuCore the Studio talks to. The transport is the sole
 *  writer; canvas / inspector / dock read it via the transport snapshot, and
 *  the memory tabs read it directly. This is a live binding: `initCore()` assigns
 *  it once before the first render, and consumers read `ppuCore.<method>()` at
 *  call time (after init), so they observe the assignment.
 *
 *  There is no software fallback. The tool IS the SNES PPU — without the WASM
 *  core there is nothing to render, so a "mock PPU" would only ever be a
 *  convincing lie. If the core fails to load, `main.tsx` shows a hard error
 *  instead of a degraded app. */
export let ppuCore: PpuCore;

/** Assignment seam for `ppuCore`. `initCore()` uses it in the app; the vitest
 *  setup installs a lightweight stub (real wasm can't init under node/jsdom).
 *  The core is set exactly once before first render — not for runtime swapping. */
export function setPpuCore(core: PpuCore): void {
  ppuCore = core;
}

/** Load the real WASM PPU core and make it live. Rejects if the module fails to
 *  load — the caller surfaces that as a hard error, because the app cannot run
 *  without it. Call once before rendering. The wasm adapter is dynamically
 *  imported so its glue stays in a separate chunk. */
export async function initCore(): Promise<void> {
  const { createWasmPpuCore } = await import("./wasm");
  setPpuCore(await createWasmPpuCore());
}
