import { useMemo, useState } from "react";
import type { FrameResult, ImportReport } from "../../ppu/core";
import { cgram15ToCss, formatValue } from "./format";
import { MODE_BPP } from "./tracemem/regions";
import { pokeMany } from "../pokes/pokeStore";
import type { Poke } from "../pokes/pokes";

type BgId = 0 | 1 | 2 | 3;
const TILE_PAGE = 64;
const MAP_PAGE = 256;

export function decodeTile2bpp(vram: Uint16Array, base: number, tile: number): number[] {
  const out = new Array<number>(64).fill(0);
  const off = base + tile * 8;
  for (let y = 0; y < 8; y++) {
    const w = vram[(off + y) & 0x7fff] ?? 0;
    for (let x = 0; x < 8; x++) {
      const bit = 7 - x;
      out[y * 8 + x] = ((w >> bit) & 1) | (((w >> (bit + 8)) & 1) << 1);
    }
  }
  return out;
}

export function decodeTile4bpp(vram: Uint16Array, base: number, tile: number): number[] {
  const out = new Array<number>(64).fill(0);
  const off = base + tile * 16;
  for (let y = 0; y < 8; y++) {
    const lo = vram[(off + y) & 0x7fff] ?? 0;
    const hi = vram[(off + 8 + y) & 0x7fff] ?? 0;
    for (let x = 0; x < 8; x++) {
      const bit = 7 - x;
      out[y * 8 + x] =
        ((lo >> bit) & 1) |
        (((lo >> (bit + 8)) & 1) << 1) |
        (((hi >> bit) & 1) << 2) |
        (((hi >> (bit + 8)) & 1) << 3);
    }
  }
  return out;
}

export function decodeTile8bpp(vram: Uint16Array, base: number, tile: number): number[] {
  const out = new Array<number>(64).fill(0);
  const off = base + tile * 32;
  for (let y = 0; y < 8; y++) {
    for (let pair = 0; pair < 4; pair++) {
      const word = vram[(off + pair * 8 + y) & 0x7fff] ?? 0;
      for (let x = 0; x < 8; x++) {
        const bit = 7 - x;
        out[y * 8 + x] |= ((word >> bit) & 1) << (pair * 2);
        out[y * 8 + x] |= ((word >> (bit + 8)) & 1) << (pair * 2 + 1);
      }
    }
  }
  return out;
}

export function characterPokes(
  vram: Uint16Array,
  base: number,
  tile: number,
  bpp: 2 | 4 | 8,
  pixels: number[],
  mode7 = false,
): Poke[] {
  if (mode7) {
    return pixels.map((pixel, i) => {
      const addr = (tile * 64 + i) & 0x7fff;
      const word = (vram[addr] & 0x00ff) | ((pixel & 0xff) << 8);
      return { lvalue: `vram[0x${addr.toString(16)}]`, expr: `0x${word.toString(16)}`, note: `tile ${tile}` };
    });
  }
  const words: Poke[] = [];
  const off = base + tile * bpp * 4;
  for (let pair = 0; pair < bpp / 2; pair++) {
    for (let y = 0; y < 8; y++) {
      let word = 0;
      for (let x = 0; x < 8; x++) {
        const pixel = pixels[y * 8 + x] ?? 0;
        word |= ((pixel >> (pair * 2)) & 1) << (7 - x);
        word |= ((pixel >> (pair * 2 + 1)) & 1) << (15 - x);
      }
      const addr = (off + pair * 8 + y) & 0x7fff;
      words.push({ lvalue: `vram[0x${addr.toString(16)}]`, expr: `0x${word.toString(16)}`, note: `tile ${tile}` });
    }
  }
  return words;
}

export function tilemapEntry(word: number) {
  return {
    tile: word & 0x03ff,
    pal: (word >> 10) & 0x07,
    prio: ((word >> 13) & 1) === 1,
    flipX: ((word >> 14) & 1) === 1,
    flipY: ((word >> 15) & 1) === 1,
  };
}

function reg(frame: FrameResult, name: string): number {
  return frame.registers.find((r) => r.name === name)?.value ?? 0;
}

function bgBases(frame: FrameResult, bg: BgId) {
  const sc = reg(frame, `BG${bg + 1}SC`);
  const mapBase = ((sc >> 2) & 0x3f) << 10;
  const nba = reg(frame, bg < 2 ? "BG12NBA" : "BG34NBA");
  const nibble = bg === 0 || bg === 2 ? nba & 0x0f : (nba >> 4) & 0x0f;
  return { mapBase, charBase: nibble << 12, screenSize: sc & 0x03 };
}

function bgBpp(mode: number, bg: BgId): 2 | 4 | 8 | 0 {
  return (MODE_BPP[mode]?.[bg] ?? 0) as 2 | 4 | 8 | 0;
}

function tileColor(
  frame: FrameResult,
  bpp: 2 | 4 | 8,
  pal: number,
  idx: number,
  paletteBase: number,
): string {
  const base = paletteBase + (bpp === 2 ? pal * 4 : bpp === 4 ? pal * 16 : 0);
  return idx === 0 ? "transparent" : cgram15ToCss(frame.cgram[base + idx] ?? 0);
}

function TilePreview({
  frame,
  pixels,
  bpp,
  pal,
  paletteBase = 0,
}: {
  frame: FrameResult;
  pixels: number[];
  bpp: 2 | 4 | 8;
  pal: number;
  paletteBase?: number;
}) {
  return (
    <div className="vram-tile-px" aria-hidden="true">
      {pixels.map((p, i) => (
        <span key={i} style={{ background: tileColor(frame, bpp, pal, p, paletteBase) }} />
      ))}
    </div>
  );
}

function reportLine(r: ImportReport): string {
  const who = r.layer === undefined ? "OBJ" : `BG${r.layer + 1}`;
  return `${who}: "${r.slot}" not placed — needs ${r.expected}, found ${r.found}`;
}

function CharacterEditor({
  frame,
  vram,
  base,
  tile,
  bpp,
  pixels: initial,
  mode7,
  paletteBase,
  onClose,
}: {
  frame: FrameResult;
  vram: Uint16Array;
  base: number;
  tile: number;
  bpp: 2 | 4 | 8;
  pixels: number[];
  mode7: boolean;
  paletteBase: number;
  onClose: () => void;
}) {
  const [pixels, setPixels] = useState(initial);
  const [ink, setInk] = useState(1);
  const [history, setHistory] = useState<number[][]>([]);
  const write = (next: number[]) => {
    setHistory((old) => [...old.slice(-31), pixels]);
    setPixels(next);
    pokeMany(characterPokes(vram, base, tile, bpp, next, mode7));
  };
  const paint = (i: number) => {
    if (pixels[i] === ink) return;
    const next = [...pixels];
    next[i] = ink;
    write(next);
  };
  return (
    <div className="vram-editor" aria-label={`Edit character ${tile}`}>
      <div className="vram-editor-head">
        <strong>CHAR ${tile.toString(16).toUpperCase().padStart(3, "0")}</strong>
        <span>{bpp}bpp · index {ink}</span>
        <button type="button" className="btn-ghost" onClick={onClose}>close</button>
      </div>
      <div className="vram-editor-body">
        <div className="vram-pixel-editor">
          {pixels.map((pixel, i) => (
            <button
              type="button"
              key={i}
              aria-label={`pixel ${i % 8},${Math.floor(i / 8)} index ${pixel}`}
              style={{ background: tileColor(frame, bpp, 0, pixel, paletteBase) }}
              onPointerDown={(e) => {
                e.preventDefault();
                paint(i);
              }}
              onPointerEnter={(e) => e.buttons === 1 && paint(i)}
            />
          ))}
        </div>
        <div className="vram-editor-tools">
          <label>
            color index
            <input
              type="number"
              min={0}
              max={(1 << bpp) - 1}
              value={ink}
              onChange={(e) => {
                const value = e.currentTarget.valueAsNumber;
                if (Number.isFinite(value)) setInk(Math.max(0, Math.min((1 << bpp) - 1, value)));
              }}
            />
          </label>
          <div className="vram-editor-actions">
            <button
              type="button"
              className="btn-ghost"
              disabled={history.length === 0}
              onClick={() => {
                const prior = history[history.length - 1];
                if (!prior) return;
                setHistory((old) => old.slice(0, -1));
                setPixels(prior);
                pokeMany(characterPokes(vram, base, tile, bpp, prior, mode7));
              }}
            >undo</button>
            <button type="button" className="btn-ghost" onClick={() => write(new Array(64).fill(0))}>clear</button>
          </div>
          <span>Writes generated VRAM pokes. Your code can still overwrite them.</span>
        </div>
      </div>
    </div>
  );
}

export function VramTab({
  frame,
  vram,
  reports,
}: {
  frame: FrameResult | null;
  vram: Uint16Array;
  reports: ImportReport[];
}) {
  const [bg, setBg] = useState<BgId>(0);
  const [tilePage, setTilePage] = useState(0);
  const [mapPage, setMapPage] = useState(0);
  const [editing, setEditing] = useState<{ tile: number; pixels: number[] } | null>(null);

  const mode = frame ? reg(frame, "BGMODE") & 0x07 : 1;
  const activeBg = bgBpp(mode, bg)
    ? bg
    : (MODE_BPP[mode]?.findIndex(Boolean) as BgId) || 0;
  const bpp = bgBpp(mode, activeBg);
  const bases = frame ? bgBases(frame, activeBg) : { mapBase: 0, charBase: 0, screenSize: 0 };
  const tileCount = mode === 7 ? 256 : 1024;
  const tileStart = Math.min(tilePage * TILE_PAGE, tileCount - TILE_PAGE);
  const tiles = useMemo(() => {
    if (mode === 7) {
      return Array.from({ length: TILE_PAGE }, (_, i) => {
        const tile = tileStart + i;
        const pixels = new Array<number>(64).fill(0);
        for (let i = 0; i < 64; i++) pixels[i] = (vram[(tile * 64 + i) & 0x7fff] ?? 0) >> 8;
        return pixels;
      });
    }
    const decode = bpp === 2 ? decodeTile2bpp : bpp === 8 ? decodeTile8bpp : decodeTile4bpp;
    return Array.from({ length: TILE_PAGE }, (_, i) =>
      decode(vram, bases.charBase, tileStart + i),
    );
  }, [bases.charBase, bpp, mode, tileStart, vram]);

  if (!frame) return <div className="insp-empty">waiting for frame…</div>;

  const [mapWidth, mapHeight] =
    mode === 7
      ? [128, 128]
      : bases.screenSize === 1
        ? [64, 32]
        : bases.screenSize === 2
          ? [32, 64]
          : bases.screenSize === 3
            ? [64, 64]
            : [32, 32];
  const mapCount = mapWidth * mapHeight;
  const mapStart = Math.min(mapPage * MAP_PAGE, mapCount - MAP_PAGE);
  const map = Array.from({ length: Math.min(MAP_PAGE, mapCount - mapStart) }, (_, i) => {
    const cell = mapStart + i;
    if (mode === 7)
      return { tile: vram[cell] & 0xff, pal: 0, prio: false, flipX: false, flipY: false };
    const x = cell % mapWidth;
    const y = Math.floor(cell / mapWidth);
    const screen =
      bases.screenSize === 1
        ? Math.floor(x / 32)
        : bases.screenSize === 2
          ? Math.floor(y / 32)
          : bases.screenSize === 3
            ? Math.floor(y / 32) * 2 + Math.floor(x / 32)
            : 0;
    const off = screen * 0x400 + (y % 32) * 32 + (x % 32);
    return tilemapEntry(vram[(bases.mapBase + off) & 0x7fff] ?? 0);
  });

  const pager = (count: number, page: number, setPage: (page: number) => void, size: number) => (
    <select
      aria-label="Page"
      value={Math.min(page, Math.ceil(count / size) - 1)}
      onChange={(e) => setPage(Number(e.target.value))}
    >
      {Array.from({ length: Math.ceil(count / size) }, (_, p) => (
        <option value={p} key={p}>
          {p * size}–{Math.min((p + 1) * size - 1, count - 1)}
        </option>
      ))}
    </select>
  );

  return (
    <div className="insp-scroll">
      <div className="vram-toolbar">
        <div className="insp-subhead">VRAM</div>
        <select
          value={activeBg}
          onChange={(e) => {
            setBg(Number(e.target.value) as BgId);
            setTilePage(0);
            setMapPage(0);
          }}
          disabled={mode === 7}
        >
          {(MODE_BPP[mode] ?? []).map((bits, i) =>
            bits ? <option value={i} key={i}>BG{i + 1}</option> : null,
          )}
        </select>
      </div>

      <div className="vram-budget">
        {reports.length === 0 ? (
          <span>no import reports this frame</span>
        ) : (
          reports.map((r, i) => <span key={i}>{reportLine(r)}</span>)
        )}
      </div>

      <div className="vram-meta">
        <span>mode {mode}</span>
        <span>{mode === 7 ? "Mode 7 interleaved" : `${bpp}bpp`}</span>
        <span>map ${formatValue(bases.mapBase)}</span>
        <span>char ${formatValue(bases.charBase)}</span>
        <span>screen {bases.screenSize}</span>
      </div>

      <div className="vram-section-head">
        <div className="insp-subhead">TILES · {tileCount}</div>
        {pager(tileCount, tilePage, setTilePage, TILE_PAGE)}
      </div>
      {editing && (
        <CharacterEditor
          key={`${mode}:${activeBg}:${editing.tile}`}
          frame={frame}
          vram={vram}
          base={bases.charBase}
          tile={editing.tile}
          bpp={mode === 7 ? 8 : bpp || 4}
          pixels={editing.pixels}
          mode7={mode === 7}
          paletteBase={mode === 0 ? activeBg * 32 : 0}
          onClose={() => setEditing(null)}
        />
      )}
      <div className="vram-grid vram-grid--tiles">
        {tiles.map((pixels, i) => {
          const tile = tileStart + i;
          return <button
            type="button"
            className="vram-tile"
            key={tile}
            title={`edit tile ${tile}`}
            onClick={() => setEditing({ tile, pixels: [...pixels] })}
          >
            <TilePreview
              frame={frame}
              pixels={pixels}
              bpp={mode === 7 ? 8 : bpp || 4}
              pal={0}
              paletteBase={mode === 0 ? activeBg * 32 : 0}
            />
            <span>{tile.toString(16).padStart(3, "0")}</span>
          </button>
        })}
      </div>

      <div className="vram-section-head">
        <div className="insp-subhead">TILEMAP · {mapWidth}×{mapHeight}</div>
        {pager(mapCount, mapPage, setMapPage, MAP_PAGE)}
      </div>
      <div className="vram-grid vram-grid--map">
        {map.map((m, i) => (
          <div
            className={"vram-map-cell" + (m.prio ? " vram-map-cell--prio" : "")}
            key={i}
            title={`cell ${mapStart + i}`}
          >
            <span>t{m.tile}</span>
            <span>p{m.pal}</span>
            <span>
              {m.flipX ? "H" : ""}
              {m.flipY ? "V" : ""}
            </span>
          </div>
        ))}
      </div>

      <div className="insp-subhead">CGRAM BANKS</div>
      <div className="palette-banks">
        {Array.from({ length: 16 }, (_, bank) => (
          <div className="palette-bank" key={bank}>
            <span>{bank.toString(16).toUpperCase()}</span>
            {Array.from({ length: 16 }, (_, i) => (
              <i
                key={i}
                style={{ background: cgram15ToCss(frame.cgram[bank * 16 + i] ?? 0) }}
                title={`$${(bank * 16 + i).toString(16).padStart(2, "0")}`}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
