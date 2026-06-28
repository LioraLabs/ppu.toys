import { useCallback, useRef, useState } from "react";
import type { ChangeEvent, DragEvent } from "react";
import "./assets.css";
import { ppuCore } from "../../ppu/instance";
import { useAssets } from "./useAssets";

/** ASSETS dock section: a PNG drop zone + a list of uploaded assets (preview +
 *  the id users reference from Lua as bg[n].source / obj.sheet). */
export function AssetsPanel() {
  const { assets, error, addFiles } = useAssets(ppuCore);
  const [over, setOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const onDrop = useCallback(
    (e: DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setOver(false);
      if (e.dataTransfer.files.length) void addFiles(e.dataTransfer.files);
    },
    [addFiles],
  );

  const onPick = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => {
      if (e.target.files) void addFiles(e.target.files);
      e.target.value = "";
    },
    [addFiles],
  );

  return (
    <div className="assets-section">
      <div className="section-header">ASSETS</div>
      <div
        className={"asset-drop" + (over ? " asset-drop--over" : "")}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={onDrop}
        onClick={() => inputRef.current?.click()}
        role="button"
        tabIndex={0}
      >
        <span className="asset-drop-main">drop image &rarr; sprite / bg</span>
        <span className="asset-drop-sub">png &middot; 4bpp / 2bpp &middot; auto-tile</span>
        <input
          ref={inputRef}
          type="file"
          accept="image/png"
          multiple
          hidden
          onChange={onPick}
        />
      </div>
      {error && <div className="asset-error">{error}</div>}
      {assets.length > 0 && (
        <div className="asset-list">
          {assets.map((a) => (
            <div className="asset-tile" key={a.id} title={`${a.name} · ${a.width}×${a.height}`}>
              <img className="asset-thumb" src={a.preview} alt={a.name} />
              <span className="asset-id">{a.id}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
