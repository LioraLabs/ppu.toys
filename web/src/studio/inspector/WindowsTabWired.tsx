import { useMemo } from "react";
import { ppuCore } from "../../ppu/instance";
import { useTransport } from "../transport/transport";
import { WindowsTab } from "./WindowsTab";

/** Wired container: feeds the Windows editor its per-scanline window registers
 *  from the shared core, refreshed every transport frame. Cheap by the same
 *  measure as the M7 fan — 224 rows x 11 bytes — so unlike that panel's plane
 *  image there is nothing here worth caching across frames. */
export function WindowsTabWired() {
  const { f } = useTransport();
  // f advances every transport frame — exactly the refresh signal we want
  const rows = useMemo(() => ppuCore.winScanlines(), [f]);
  return <WindowsTab rows={rows} />;
}
