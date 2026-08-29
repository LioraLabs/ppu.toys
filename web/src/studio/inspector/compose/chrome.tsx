import { formatAddr, formatValue } from "../format";
import { useCopyToast } from "../copyToast";
import {
  clearPokes,
  setDialect,
  unpoke,
  unpokeMany,
  usePokes,
  usePokesApplied,
  usePokesSource,
} from "../../pokes/pokeStore";
import { HexPoke } from "../../pokes/HexPoke";
import { pokeMatchesLive, type PokeDialect } from "./model";
import type { Compositor } from "./useCompositor";
import { usePokeDialect } from "./dialect";

/** Marker a poked control wears; click = unpoke everything it covers. SOLID
 *  while every covered poke still matches the live registers; HOLLOW when a
 *  later script write overrode any of them (apply_pokes() runs first in
 *  frame(), so the script wins). Renders nothing while unpoked. `fields`
 *  scopes the marker to a control's friendly fields; without it the marker
 *  is register-centric (any poke living in `addr`). */
export function PokeDot({
  c,
  addr,
  fields,
}: {
  c: Compositor;
  addr: number;
  fields?: readonly string[];
}) {
  const ps = c.pokedAt(addr, fields);
  if (ps.length === 0) return null;
  const matches = ps.map((p) => pokeMatchesLive(p, c.frame.registers));
  const match = matches.some((m) => m === false)
    ? false
    : matches.every((m) => m === true)
      ? true
      : null;
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

/** Segmented selector for the dialect NEW pokes emit: friendly field lines or
 *  raw whole-register mnemonics. Persisted studio preference; existing pokes
 *  are untouched (a re-poke of the same control migrates its line — the write
 *  evicts the other dialect's poke on that register). Always visible, unlike
 *  PokeBar: the choice matters before the first poke exists. */
const DIALECTS: readonly { id: PokeDialect; label: string; title: string }[] = [
  {
    id: "friendly",
    label: "friendly",
    title: 'new pokes emit friendly fields — color.op = "sub"',
  },
  {
    id: "raw",
    label: "raw",
    title: "new pokes emit whole-register mnemonics — CGADSUB = 0x41",
  },
  {
    id: "scanline",
    label: "scanline",
    title:
      "new pokes emit per-scanline keyframes inside a generated hdma() hook — needs a panel with a selected scanline (Windows); elsewhere it falls back to friendly",
  },
];

export function DialectToggle() {
  const d = usePokeDialect();
  return (
    <div className="cmp-dialect">
      <span className="cmp-dialect-label">POKE AS</span>
      <div className="cmp-seg cmp-dialect-seg" role="group" aria-label="poke dialect">
        {DIALECTS.map((o) => (
          <button
            key={o.id}
            type="button"
            className={d === o.id ? "cmp-seg--on" : ""}
            aria-pressed={d === o.id}
            title={o.title}
            onClick={() => setDialect(o.id)}
          >
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Poke summary bar: one chip per poke (click = unpoke), copy the generated
 *  apply_pokes() source, clear-all, and a warning chip when pokes exist but
 *  nothing calls apply_pokes(). Rendered by both docked tabs and the overlay;
 *  hidden while nothing is poked. Stopping playback does NOT clear pokes. */
export function PokeBar() {
  const pokes = usePokes();
  const source = usePokesSource();
  const applied = usePokesApplied();
  if (pokes.length === 0) return null;
  const copyFn = () => {
    try {
      // the FILE is the source of truth — copy its bytes, never a re-generation
      void navigator.clipboard?.writeText(source).catch(() => {});
    } catch {
      /* clipboard unavailable (permissions/tests) */
    }
  };
  return (
    <div className="cmp-pokebar">
      <span className="cmp-pokebar-label">◉ {pokes.length} poked</span>
      {pokes.map((p) => (
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
      {!applied && (
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
