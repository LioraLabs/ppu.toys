import { AssetsPanel } from "./assets/AssetsPanel";
import { LayersPanel } from "./dock/LayersPanel";
import { CgramPalette } from "./dock/CgramPalette";

export function LeftDock() {
  return (
    <aside className="dock">
      <LayersPanel />
      <CgramPalette />
      <AssetsPanel />
    </aside>
  );
}
