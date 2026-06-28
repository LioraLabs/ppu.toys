import { MockPpuCore } from "./mock";
import type { PpuCore } from "./core";

/** Single shared PpuCore the Studio talks to. The transport is the sole writer;
 *  canvas / inspector / dock read it via the transport snapshot, and VramTab
 *  reads it directly. Starts as the mock so module init stays synchronous;
 *  `bootstrapCore()` swaps in the real WASM core before first render when
 *  VITE_USE_WASM is set. This is a live `let`, so consumers that read
 *  `ppuCore.<method>()` at call time observe the swap. */
export let ppuCore: PpuCore = new MockPpuCore();

export type CoreKind = "mock" | "wasm";

/** Which core is live. Set once by bootstrapCore before first render, so reading
 *  it at render time (not import time) reflects the actual selection. */
let kind: CoreKind = "mock";
export function coreKind(): CoreKind {
  return kind;
}

/** Select the real WASM core when VITE_USE_WASM is set (see Cookfile
 *  `dev-wasm`, which builds the wasm pkg and runs Vite with the flag). No-op —
 *  stays on the mock — otherwise. Call once before rendering the app. The wasm
 *  adapter is dynamically imported so mock-mode builds never load the wasm glue. */
export async function bootstrapCore(): Promise<void> {
  if (import.meta.env.VITE_USE_WASM) {
    const { createWasmPpuCore } = await import("./wasm");
    ppuCore = await createWasmPpuCore();
    kind = "wasm";
  }
}
