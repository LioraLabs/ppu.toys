import type { PlaneId } from "../../../ppu/core";
import { MODE_BPP, REGION_COLORS } from "./regions";
import { setLayerVisible, useLayerVis } from "./stores";
import "./tracemem.css";

const LAYERS: { id: PlaneId; name: string; color: string }[] = [
  { id: "bg1", name: "BG1", color: REGION_COLORS["bg1-char"] },
  { id: "bg2", name: "BG2", color: REGION_COLORS["bg2-char"] },
  { id: "bg3", name: "BG3", color: REGION_COLORS["bg3-char"] },
  { id: "bg4", name: "BG4", color: REGION_COLORS["bg4-char"] },
  { id: "obj", name: "OBJ", color: REGION_COLORS["obj-a"] },
];

function layerTag(id: PlaneId, mode: number): { tag: string; absent: boolean } {
  if (id === "obj") return { tag: "4bpp sprites", absent: false };
  const bpp = (MODE_BPP[mode] ?? MODE_BPP[1])[Number(id[2]) - 1];
  return bpp
    ? { tag: `${bpp}bpp tiles`, absent: false }
    : { tag: `absent in mode ${mode}`, absent: true };
}

/** Priority stack + per-layer visibility toggles. Lived in the Memory & Layers
 *  overlay until the Expand overlays were retired; now a Memory-tab panel.
 *  Reads/writes the shared layer-visibility store (wasm-free). */
export function LayersPanel({ mode }: { mode: number }) {
  const vis = useLayerVis();
  return (
    <div className="tm-layers">
      {LAYERS.map((l) => {
        const { tag, absent } = layerTag(l.id, mode);
        return (
          <div key={l.id} className={"tm-layerrow" + (absent ? " tm-layerrow--absent" : "")}>
            <i style={{ background: l.color }} />
            <div className="tm-layername">
              <div>{l.name}</div>
              <div>{tag}</div>
            </div>
            <button
              type="button"
              className={"tm-pill " + (vis[l.id] ? "tm-pill--on" : "tm-pill--off")}
              onClick={() => setLayerVisible(l.id, !vis[l.id])}
            >
              {vis[l.id] ? "on" : "off"}
            </button>
          </div>
        );
      })}
    </div>
  );
}
