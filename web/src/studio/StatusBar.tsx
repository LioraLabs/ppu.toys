import { useTransport } from "./transport/transport";
import { useSharedAssets } from "./assets/sharedAssets";

/** Live status bar: play state, transport clock, fps, asset count. */
export function StatusBar() {
  const { t, f, playing, fps } = useTransport();
  const assets = useSharedAssets();
  return (
    <footer className="statusbar">
      <span className="sb-item">
        <span className="sb-dot" />
        lua
      </span>
      <span className="sb-item">{playing ? "▶ playing" : "⏸ paused"}</span>
      <span className="sb-item">t={t.toFixed(2)}s · f={f}</span>
      <span className="sb-item">assets: {assets.length}</span>
      <span className="tb-spacer" />
      <span className="sb-item">{fps} fps</span>
      <span className="sb-item">256×224</span>
      <span className="sb-item sb-item--dim">mock-ppu</span>
    </footer>
  );
}
