import { formatAddr, cgram15ToCss } from "../format";
import { cgLabel } from "./trace";
import type { CgramOwner, VramRegion } from "./regions";

const pct = (words: number) => `${(words / 0x8000) * 100}%`;

/** Full-width VRAM address-space bar: char regions on the top lane, map regions
 *  on the bottom (live-derived regions may overlap — two lanes keep both
 *  readable). Hover outlines; click copies the region's base address. */
export function VramBar({ regions, onCopy }: { regions: VramRegion[]; onCopy: (label: string) => void }) {
  return (
    <div className="tm-vrambar">
      {regions.map((r) => (
        <button
          key={r.id}
          type="button"
          className={
            "tm-vramregion" +
            (r.id === "m7" ? " tm-vramregion--full" : r.kind === "map" ? " tm-vramregion--map" : "")
          }
          style={{ left: pct(r.start), width: pct(Math.max(r.end - r.start, 64)), background: r.color }}
          title={`${r.label} ${formatAddr(r.start)}–${formatAddr(Math.max(r.end - 1, r.start))} · ${r.usage}`}
          onClick={() => onCopy(formatAddr(r.start))}
        />
      ))}
    </div>
  );
}

/** Wrapping legend; each row copies its region's base address. */
export function VramLegend({ regions, onCopy }: { regions: VramRegion[]; onCopy: (label: string) => void }) {
  return (
    <div className="tm-legend">
      {regions.map((r) => (
        <button key={r.id} type="button" onClick={() => onCopy(formatAddr(r.start))}>
          <i style={{ background: r.color }} />
          <span className="tm-lname">{r.label}</span>
          <span className="tm-lrange">
            {formatAddr(r.start)}–{formatAddr(Math.max(r.end - 1, r.start))}
          </span>
          <span className="tm-lusage">{r.usage}</span>
        </button>
      ))}
    </div>
  );
}

/** CGRAM ownership: 16 palettes x 16 entries; transparent index-0 column gets a
 *  diagonal hairline; owner legend on the left; click copies "CG $xx". */
export function CgramGrid({
  cgram,
  owners,
  onCopy,
}: {
  cgram: Uint16Array;
  owners: CgramOwner[];
  onCopy: (label: string) => void;
}) {
  return (
    <div className="tm-cgrid">
      {owners.map((owner, row) => (
        <CgramRow key={row} row={row} owner={owner} cgram={cgram} onCopy={onCopy} />
      ))}
    </div>
  );
}

function CgramRow({ row, owner, cgram, onCopy }: { row: number; owner: CgramOwner; cgram: Uint16Array; onCopy: (label: string) => void }) {
  return (
    <>
      <span className={"tm-cgowner" + (owner.used ? "" : " tm-cgowner--unused")} title={owner.label}>
        {owner.label}
      </span>
      {Array.from({ length: 16 }, (_, col) => {
        const i = row * 16 + col;
        return (
          <button
            key={col}
            type="button"
            className={"tm-cgcell" + (col === 0 ? " tm-cgcell--zero" : "")}
            style={{ backgroundColor: cgram15ToCss(cgram[i] ?? 0) }}
            title={cgLabel(i)}
            onClick={() => onCopy(cgLabel(i))}
          />
        );
      })}
    </>
  );
}
