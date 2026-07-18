import { useEffect, useRef, useState } from "react";
import type { Poke } from "./pokes";
import { poke } from "./pokeStore";
import { Copyable, useCopyToast } from "../inspector/copyToast";
import { bgr555ToHex, cgLabel } from "../inspector/tracemem/trace";
import { hexToBgr555 } from "../inspector/compose/model";
import "./pokes.css";

export function cgramPoke(index: number, bgr555: number): Poke {
  return {
    lvalue: `cgram[0x${index.toString(16).padStart(2, "0")}]`,
    expr: `0x${bgr555.toString(16).padStart(4, "0")}`,
    note: bgr555ToHex(bgr555),
  };
}

/** Small anchored popover: a native color picker that pokes `cgram[idx]` on
 *  every input event (regeneration is cheap; autosave debounces persistence).
 *  Positioned by the caller — a `position: relative` wrapper around the
 *  trigger element gives this its containing block. Dismisses on Escape and
 *  on an outside click.
 *
 *  `onChange` is the injectable write seam: by default it writes the poke
 *  store, so every existing caller is render-identical. Stories/tests pass a
 *  fixture `onChange` to render with no poke store on the path. */
export function CgramPoke({
  index,
  current,
  onChange,
  onClose,
}: {
  index: number;
  current: number;
  onChange?: (bgr555: number) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const { toast, copy } = useCopyToast();
  // Local value drives the live readout without waiting on the next live
  // frame to round-trip `current` back through the caller.
  const [bgr555, setBgr555] = useState(current);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [onClose]);

  const label = cgLabel(index);
  const hex = bgr555ToHex(bgr555);

  return (
    <div className="pk-popover" ref={ref}>
      <div className="pk-head">
        <span className="pk-title">{label}</span>
        <button type="button" className="pk-close" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <input
        type="color"
        className="pk-color-input"
        defaultValue={hex}
        onInput={(e) => {
          const next = hexToBgr555(e.currentTarget.value);
          setBgr555(next);
          (onChange ?? ((v) => poke(cgramPoke(index, v))))(next);
        }}
      />
      <div className="pk-readout">
        BGR555 ${bgr555.toString(16).padStart(4, "0")} · {hex}
      </div>
      <div className="pk-addr">
        <Copyable label={label} onCopy={copy} />
      </div>
      {toast}
    </div>
  );
}
