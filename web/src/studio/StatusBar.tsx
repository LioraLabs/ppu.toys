import { useTransport } from "./transport/transport";
import { useSharedAssets } from "./assets/sharedAssets";
import { coreKind } from "../ppu/instance";

/** Live status bar: play state, transport clock, fps, asset count. */
export function StatusBar() {
  const { t, f, playing, fps, runtimeError } = useTransport();
  const assets = useSharedAssets();
  return (
    <footer className="statusbar">
      <span
        className={`sb-item${runtimeError ? " sb-item--error" : ""}`}
        title={runtimeError?.message}
      >
        <span className={`sb-dot${runtimeError ? " sb-dot--error" : ""}`} />
        {runtimeError ? `lua error${runtimeError.line ? `: line ${runtimeError.line}` : ""}` : "lua"}
      </span>
      <span className="sb-item">{playing ? "▶ playing" : "⏸ paused"}</span>
      <span className="sb-item">t={t.toFixed(2)}s · f={f}</span>
      <span className="sb-item">assets: {assets.length}</span>
      <span className="tb-spacer" />
      <span className="sb-item">{fps} fps</span>
      <span className="sb-item">256×224</span>
      <span className="sb-item sb-item--dim">{coreKind() === "wasm" ? "wasm-ppu" : "mock-ppu"}</span>
    </footer>
  );
}
