/** VRAM tab. The uploaded-asset list lives in AssetsPanel's local useAssets
 *  state (LeftDock), and the PpuCore seam exposes no VRAM read-back. So there is
 *  no data path to enumerate uploaded sources / obj.sheet from here yet.
 *
 *  NOTE (U8): wire this to a shared asset store (or a seam VRAM-read method) so
 *  the inspector can show source names + previews. Placeholder until then. */
export function VramTab() {
  return (
    <div className="insp-scroll">
      <div className="insp-note">
        VRAM sources are not yet readable from the inspector. Uploaded images are
        tracked in the left dock's Assets panel; a shared asset store (or a seam
        VRAM-read method) lands at U8 to surface names + previews here.
      </div>
      <div className="insp-empty">no VRAM sources</div>
    </div>
  );
}
