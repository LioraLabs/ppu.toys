import { useMemo } from "react";
import { ppuCore } from "../../ppu/instance";
import { useSharedAssets } from "../assets/sharedAssets";

/** VRAM tab: enumerates sources resident in the core (PpuCore.listAssets) and
 *  pairs each with the uploaded preview/name from the shared asset store. */
export function VramTab() {
  const uiAssets = useSharedAssets(); // re-renders on upload
  const byId = useMemo(
    () => new Map(uiAssets.map((a) => [a.id, a])),
    [uiAssets],
  );
  const vram = ppuCore.listAssets();

  if (vram.length === 0) {
    return (
      <div className="insp-scroll">
        <div className="insp-note">
          No VRAM sources yet. Drop a PNG in the left dock's Assets panel; it
          appears here and is referenceable from Lua as bg[n].source / obj.sheet.
        </div>
        <div className="insp-empty">no VRAM sources</div>
      </div>
    );
  }

  return (
    <div className="insp-scroll">
      <div className="insp-subhead">VRAM · {vram.length} source(s)</div>
      <div className="asset-list">
        {vram.map((v) => {
          const ui = byId.get(v.id);
          return (
            <div className="asset-tile" key={v.id} title={`${v.id} · ${v.width}×${v.height}`}>
              {ui && <img className="asset-thumb" src={ui.preview} alt={v.id} />}
              <span className="asset-id">{v.id}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
