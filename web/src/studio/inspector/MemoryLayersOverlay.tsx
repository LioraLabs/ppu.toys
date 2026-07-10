/** Full-screen "Memory & Layers" overlay (⤢ Expand from Trace/Memory).
 *  Chrome only — the body (VRAM map, CGRAM grid, layer visibility) lands in
 *  the Trace/Memory inspector ticket. */
export function MemoryLayersOverlay({ onCollapse }: { onCollapse: () => void }) {
  return (
    <div className="insp-overlay">
      <div className="insp-overlay-bar">
        <span className="insp-overlay-title">Memory &amp; Layers</span>
        <span className="insp-overlay-sub">VRAM · CGRAM · layer visibility</span>
        <div className="tb-spacer" />
        <button type="button" className="btn-ghost" onClick={onCollapse}>
          ↩ Collapse
        </button>
      </div>
      <div className="insp-overlay-body" />
    </div>
  );
}
