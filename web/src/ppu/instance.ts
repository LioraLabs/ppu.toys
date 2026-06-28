import { MockPpuCore } from "./mock";
import type { PpuCore } from "./core";

/** Single shared PpuCore the Studio talks to. Swapped for the real WASM core at
 *  integration. NOTE: U3/U4 also consume a core instance; the milestone
 *  orchestrator reconciles ownership — kept to one line for trivial merge. */
export const ppuCore: PpuCore = new MockPpuCore();
