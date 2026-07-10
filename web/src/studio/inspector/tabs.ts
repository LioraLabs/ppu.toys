/** Inspector tab model. The four Workspace tabs come first (their bodies land
 *  in the Trace/Memory and Compose/Windows inspector tickets); the legacy tabs
 *  stay functional through the transition — the final set is a done-gate call. */
export type TabId =
  | "trace"
  | "memory"
  | "compose"
  | "windows"
  | "registers"
  | "sprites"
  | "vram";

export type OverlayId = "memory-layers" | "compositor";

export const INSPECTOR_TABS: { id: TabId; label: string; legacy?: boolean }[] = [
  { id: "trace", label: "Trace" },
  { id: "memory", label: "Memory" },
  { id: "compose", label: "Compose" },
  { id: "windows", label: "Windows" },
  { id: "registers", label: "Registers", legacy: true },
  { id: "sprites", label: "Sprites", legacy: true },
  { id: "vram", label: "VRAM", legacy: true },
];

/** Which full-screen overlay ⤢ Expand opens for a tab (legacy tabs: none). */
export function overlayForTab(id: TabId): OverlayId | null {
  switch (id) {
    case "trace":
    case "memory":
      return "memory-layers";
    case "compose":
    case "windows":
      return "compositor";
    default:
      return null;
  }
}
