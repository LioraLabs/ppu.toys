import { useCallback, useRef, useState } from "react";
import type { ChangeEvent, DragEvent } from "react";
import { useAssets } from "../assets/useAssets";

/** LIVE OUTPUT drop zone: PNG → quantized into a format-committed graphics
 *  source (useAssets → convertSource → addSource) + persisted on the sketch. */
export function DropZone() {
  const { error, addFiles } = useAssets();
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
    <div
      className={"drop-zone" + (over ? " drop-zone--over" : "")}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={(e) => {
        // ignore dragleave fired when crossing into a child element
        if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(false);
      }}
      onDrop={onDrop}
      onClick={() => inputRef.current?.click()}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          inputRef.current?.click();
        }
      }}
      role="button"
      tabIndex={0}
    >
      <span className="drop-main">drop image &rarr; sprite / bg</span>
      <span className={"drop-sub" + (error ? " drop-sub--error" : "")}>
        {error ?? "png · quantized to tiles+palette"}
      </span>
      <input ref={inputRef} type="file" accept="image/png" multiple hidden onChange={onPick} />
    </div>
  );
}
