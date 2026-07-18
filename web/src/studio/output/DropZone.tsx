import { useCallback, useRef, useState } from "react";
import type { ChangeEvent, DragEvent } from "react";

export interface DropZoneProps {
  /** Convert/register error surfaced under the prompt (red), or null when clean. */
  error: string | null;
  /** Called with the picked/dropped files (PNG filtering happens downstream). */
  onFiles: (files: FileList | File[]) => void;
}

/** LIVE OUTPUT drop zone (presentational): a PNG picker / drag target that
 *  hands its files up via `onFiles`. The drag-over highlight is local UI
 *  state driven by native DOM events; the convert/register pipeline lives in
 *  the wired container (DropZoneWired → useAssets), so this renders wasm-free. */
export function DropZone({ error, onFiles }: DropZoneProps) {
  const [over, setOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const onDrop = useCallback(
    (e: DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setOver(false);
      if (e.dataTransfer.files.length) onFiles(e.dataTransfer.files);
    },
    [onFiles],
  );

  const onPick = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => {
      if (e.target.files) onFiles(e.target.files);
      e.target.value = "";
    },
    [onFiles],
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
