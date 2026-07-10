/** Full-screen "Compositor" overlay (⤢ Expand from Compose/Windows).
 *  Chrome only — the body (screen composite, color math, windows) lands in
 *  the Compose/Windows inspector ticket. */
export function CompositorOverlay({ onCollapse }: { onCollapse: () => void }) {
  return (
    <div className="insp-overlay">
      <div className="insp-overlay-bar">
        <span className="insp-overlay-title">Compositor</span>
        <span className="insp-overlay-sub">main/sub screens · color math · windows</span>
        <div className="tb-spacer" />
        <button type="button" className="btn-ghost" onClick={onCollapse}>
          ↩ Collapse
        </button>
      </div>
      <div className="insp-overlay-body" />
    </div>
  );
}
