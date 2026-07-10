import { useState } from "react";
import { formatAddr, formatValue } from "../format";
import { releaseAllPins, releasePin } from "./pinStore";
import type { Compositor } from "./useCompositor";

/** Marker a pinned control wears; click = unpin that register. Renders
 *  nothing while the register is script-driven. */
export function PinDot({ c, addr }: { c: Compositor; addr: number }) {
  if (!c.isPinned(addr)) return null;
  return (
    <button
      type="button"
      className="cmp-pin"
      title={`${formatAddr(addr)} pinned — script value overridden. Click to unpin.`}
      onClick={(e) => {
        e.stopPropagation();
        releasePin(addr);
      }}
    >
      ◉
    </button>
  );
}

/** Pinned-override summary: one chip per pin (click = unpin) + clear-all.
 *  Rendered by both docked tabs and the overlay; hidden while nothing is
 *  pinned. ▶ Run also clears every pin (transport.restart). */
export function PinBar({ c }: { c: Compositor }) {
  if (c.pins.length === 0) return null;
  return (
    <div className="cmp-pinbar">
      <span className="cmp-pinbar-label">◉ {c.pins.length} pinned</span>
      {c.pins.map((p) => (
        <button
          key={p.addr}
          type="button"
          className="cmp-pinchip"
          title="pinned override — click to unpin"
          onClick={() => releasePin(p.addr)}
        >
          {formatAddr(p.addr)}={formatValue(p.value)} ✕
        </button>
      ))}
      <button type="button" className="cmp-pinchip cmp-clearpins" onClick={releaseAllPins}>
        clear all
      </button>
    </div>
  );
}

/** One copyable register readout row: effective (live-or-pinned) value, note,
 *  optional color swatch, pin marker with individual unpin. */
export function RegRow({
  c,
  addr,
  name,
  note,
  swatch,
}: {
  c: Compositor;
  addr: number;
  name: string;
  note?: string;
  swatch?: string;
}) {
  const [copied, setCopied] = useState(false);
  const value = c.read(addr);
  const copy = () => {
    void navigator.clipboard?.writeText(`${formatAddr(addr)}=${formatValue(value)}`).catch(() => {});
    setCopied(true);
    window.setTimeout(() => setCopied(false), 900);
  };
  return (
    <div
      className="cmp-reg"
      role="button"
      tabIndex={0}
      title="click to copy"
      onClick={copy}
      onKeyDown={(e) => e.key === "Enter" && copy()}
    >
      <span className="cmp-reg-addr">{formatAddr(addr)}</span>
      <span className="cmp-reg-name">{name}</span>
      {swatch !== undefined && <span className="cmp-reg-swatch" style={{ background: swatch }} />}
      <span className="cmp-reg-val">{formatValue(value)}</span>
      <span className="cmp-reg-note">{copied ? "copied" : (note ?? "")}</span>
      <PinDot c={c} addr={addr} />
    </div>
  );
}
