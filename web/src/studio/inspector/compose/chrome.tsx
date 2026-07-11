import { formatAddr, formatValue } from "../format";
import { useCopyToast } from "../copyToast";
import { clearPokes, unpoke, unpokeMany } from "../../pokes/pokeStore";
import { HexPoke } from "../../pokes/HexPoke";
import { pokeMatchesLive } from "./model";
import type { Compositor } from "./useCompositor";

/** Marker a poked control wears; click = unpoke everything it covers. SOLID
 *  while every covered poke still matches the live registers; HOLLOW when a
 *  later script write overrode any of them (apply_pokes() runs first in
 *  frame(), so the script wins). Renders nothing while unpoked. `fields`
 *  scopes the marker to a control's friendly fields; without it the marker
 *  is register-centric (any poke living in `addr`). */
export function PokeDot({ c, addr, fields }: { c: Compositor; addr: number; fields?: readonly string[] }) {
  const ps = c.pokedAt(addr, fields);
  if (ps.length === 0) return null;
  const matches = ps.map((p) => pokeMatchesLive(p, c.frame.registers));
  const match = matches.some((m) => m === false) ? false : matches.every((m) => m === true) ? true : null;
  const state =
    match === null
      ? "poked"
      : match
        ? "poked · live matches"
        : "poked · live value differs (script write or quantization)";
  const what = ps.map((p) => `${p.lvalue} = ${p.expr}`).join(", ");
  return (
    <button
      type="button"
      className={"cmp-poke" + (match === false ? " cmp-poke--overridden" : "")}
      title={`${formatAddr(addr)} ${what} — ${state}. Click to unpoke.`}
      onClick={(e) => {
        e.stopPropagation();
        unpokeMany(ps.map((p) => p.lvalue));
      }}
    />
  );
}

/** Poke summary bar: one chip per poke (click = unpoke), copy the generated
 *  apply_pokes() source, clear-all, and a warning chip when pokes exist but
 *  nothing calls apply_pokes(). Rendered by both docked tabs and the overlay;
 *  hidden while nothing is poked. ▶ Run does NOT clear pokes. */
export function PokeBar({ c }: { c: Compositor }) {
  if (c.pokes.length === 0) return null;
  const copyFn = () => {
    try {
      // the FILE is the source of truth — copy its bytes, never a re-generation
      void navigator.clipboard?.writeText(c.pokesSource).catch(() => {});
    } catch {
      /* clipboard unavailable (permissions/tests) */
    }
  };
  return (
    <div className="cmp-pokebar">
      <span className="cmp-pokebar-label">◉ {c.pokes.length} poked</span>
      {c.pokes.map((p) => (
        <button
          key={p.lvalue}
          type="button"
          className="cmp-pokechip"
          title={`${p.lvalue} = ${p.expr} — click to unpoke`}
          onClick={() => unpoke(p.lvalue)}
        >
          {p.lvalue}={p.expr} ✕
        </button>
      ))}
      {!c.pokesApplied && (
        <span
          className="cmp-pokewarn"
          title="pokes.lua is generated, but no file calls apply_pokes() — the pokes never run"
        >
          ⚠ pokes not applied — call apply_pokes() in frame()
        </span>
      )}
      <button
        type="button"
        className="cmp-pokechip cmp-copypokes"
        title="copy the generated pokes.lua source"
        onClick={copyFn}
      >
        copy fn
      </button>
      <button type="button" className="cmp-pokechip cmp-clearpokes" onClick={clearPokes}>
        clear all
      </button>
    </div>
  );
}

/** One copyable register readout row: live value, note, optional color
 *  swatch, poke marker with individual unpoke. */
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
      <span className="cmp-reg-val">
        <HexPoke addr={addr} value={value}>
          {formatValue(value)}
        </HexPoke>
      </span>
      <span className="cmp-reg-note">{note ?? ""}</span>
      <PokeDot c={c} addr={addr} />
      {toast}
    </div>
  );
}
