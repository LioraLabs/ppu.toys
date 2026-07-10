export type TabId =
  | "trace"
  | "memory"
  | "compose"
  | "windows"
  | "registers"
  | "sprites"
  | "vram";

export type OverlayId = "memory-layers" | "compositor";

/** M9 done-gate decision: the full set is permanent.
 *  Trace/Memory/Compose/Windows are the workspace tabs; Registers/Sprites/VRAM
 *  stay as `aux` detail tabs — VRAM's decoded tile + tilemap previews are not
 *  replicated by Memory (which shows address-space regions + CGRAM ownership),
 *  Sprites carries the load-bearing M7 RANGE/TIME-OVER badges, and Registers is
 *  the raw register truth. `aux` is informational — overlay routing keys on the
 *  tab id (aux tabs map to none) and no distinct styling exists today. */
export const INSPECTOR_TABS: { id: TabId; label: string; aux?: boolean }[] = [
  { id: "trace", label: "Trace" },
  { id: "memory", label: "Memory" },
  { id: "compose", label: "Compose" },
  { id: "windows", label: "Windows" },
  { id: "registers", label: "Registers", aux: true },
  { id: "sprites", label: "Sprites", aux: true },
  { id: "vram", label: "VRAM", aux: true },
];

/** Which full-screen overlay ⤢ Expand opens for a tab (aux tabs: none). */
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
