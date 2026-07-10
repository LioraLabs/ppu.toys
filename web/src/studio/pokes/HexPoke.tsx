import { useEffect, useRef, useState, type ReactNode } from "react";
import { REG_LVALUES, regPoke } from "../inspector/compose/model";
import { formatValue } from "../inspector/format";
import { poke } from "./pokeStore";
import "./pokes.css";

/** Parse user hex input for a register poke. Returns null when invalid.
 *  Accepts "1f", "0x1f", "$1F". Range: 0..0xff for one-byte registers,
 *  0..0x7fff for COLDATA ($2132, 15-bit fixed color). */
export function parseHexPoke(addr: number, raw: string): number | null {
  const cleaned = raw.trim().replace(/^\$|^0x/i, "");
  if (!/^[0-9a-f]+$/i.test(cleaned)) return null;
  const v = parseInt(cleaned, 16);
  const max = addr === 0x2132 ? 0x7fff : 0xff;
  return v >= 0 && v <= max ? v : null;
}

/** Click-to-edit hex value cell for a register readout row. Not editable
 *  when `addr` has no `REG_LVALUES` mapping — status registers (e.g. $213E)
 *  render the plain value with NO affordance, exactly as before. Editable
 *  registers (the 16 REG_LVALUES entries) swap to a small inline input on
 *  click, pre-filled with the current hex (no 0x prefix), autofocused and
 *  fully selected. Enter parses + pokes a whole-register write and closes;
 *  an invalid value keeps editing with a `.pk-hex--bad` style instead of
 *  closing; Escape or blur cancels without poking. Script-wins semantics
 *  hold: a running script may immediately overwrite the poked value on the
 *  next frame (surfaced elsewhere by the poke marker, not by this control). */
export function HexPoke({
  addr,
  value,
  children,
}: {
  addr: number;
  value: number;
  children?: ReactNode;
}) {
  const editable = REG_LVALUES[addr] !== undefined;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [bad, setBad] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editing]);

  const text = children ?? formatValue(value);

  if (!editable) {
    return <span className="pk-hex">{text}</span>;
  }

  const open = () => {
    setDraft(value.toString(16));
    setBad(false);
    setEditing(true);
  };

  if (!editing) {
    return (
      <span
        className="pk-hex pk-hex--editable"
        role="button"
        tabIndex={0}
        title={`click to edit $${addr.toString(16).toUpperCase()}`}
        onClick={(e) => {
          e.stopPropagation();
          open();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.stopPropagation();
            e.preventDefault();
            open();
          }
        }}
      >
        {text}
      </span>
    );
  }

  const commit = () => {
    const v = parseHexPoke(addr, draft);
    if (v === null) {
      setBad(true);
      return;
    }
    poke(regPoke(addr, v));
    setEditing(false);
  };

  return (
    <input
      ref={inputRef}
      className={"pk-hex-input" + (bad ? " pk-hex--bad" : "")}
      value={draft}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => {
        setDraft(e.currentTarget.value);
        setBad(false);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.stopPropagation();
          commit();
        } else if (e.key === "Escape") {
          e.stopPropagation();
          setEditing(false);
        }
      }}
      onBlur={() => setEditing(false)}
    />
  );
}
