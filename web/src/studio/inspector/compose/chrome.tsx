import { formatAddr, formatValue } from "../format";
import { useCopyToast } from "../copyToast";
import type { Compositor } from "./useCompositor";

/** Marker a control wears when its register is overridden. Currently always
 *  hidden — the pin seam is gone (isPinned is always false); stubbed until
 *  Task 7 turns this into a PokeDot over the generated pokes. */
/* ppu-61: replaced in Task 7 */
export function PinDot({ c, addr }: { c: Compositor; addr: number }) {
  if (!c.isPinned(addr)) return null;
  return null;
}

/** Override summary bar. Currently always hidden — the pin seam is gone
 *  (c.pins is always empty); stubbed until Task 7 turns this into a
 *  PokeBar over the generated pokes. */
/* ppu-61: replaced in Task 7 */
export function PinBar({ c }: { c: Compositor }) {
  if (c.pins.length === 0) return null;
  return null;
}

/** One copyable register readout row: effective (live-or-overridden) value,
 *  note, optional color swatch, override marker. */
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
  const { toast, copy } = useCopyToast();
  const value = c.read(addr);
  const doCopy = () => copy(`${formatAddr(addr)}=${formatValue(value)}`);
  return (
    <div
      className="cmp-reg"
      role="button"
      tabIndex={0}
      title="click to copy"
      onClick={doCopy}
      onKeyDown={(e) => e.key === "Enter" && doCopy()}
    >
      <span className="cmp-reg-addr">{formatAddr(addr)}</span>
      <span className="cmp-reg-name">{name}</span>
      {swatch !== undefined && <span className="cmp-reg-swatch" style={{ background: swatch }} />}
      <span className="cmp-reg-val">{formatValue(value)}</span>
      <span className="cmp-reg-note">{note ?? ""}</span>
      <PinDot c={c} addr={addr} />
      {toast}
    </div>
  );
}
