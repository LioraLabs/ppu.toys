import { useMemo, useState } from "react";
import type { FrameResult, ImportReport } from "../../ppu/core";
import { ppuCore } from "../../ppu/instance";
import { cgram15ToCss, formatValue } from "./format";

type BgId = 0 | 1 | 2;

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

function bgBpp(mode: number, bg: BgId): 2 | 4 {
  return mode === 1 && bg === 2 ? 2 : 4;
}

function tileColor(frame: FrameResult, bpp: 2 | 4, pal: number, idx: number): string {
  const base = bpp === 2 ? pal * 4 : pal * 16;
  return idx === 0 ? "transparent" : cgram15ToCss(frame.cgram[base + idx] ?? 0);
}

function TilePreview({ frame, pixels, bpp, pal }: { frame: FrameResult; pixels: number[]; bpp: 2 | 4; pal: number }) {
  return (
    <div className="vram-tile-px" aria-hidden="true">
      {pixels.map((p, i) => (
        <span key={i} style={{ background: tileColor(frame, bpp, pal, p) }} />
      ))}
    </div>
  );
}

function reportLine(r: ImportReport): string {
  if (r.mode === "m7") {
    const overflow = r.report.overflow_tiles > 0 ? ` · overflow ${r.report.overflow_tiles}` : "";
    return `M7 BG${r.layer + 1}: ${r.report.colors} colors · ${r.report.unique_tiles}/${r.report.tile_capacity} tiles · ${r.report.map_tiles_w}x${r.report.map_tiles_h} map${overflow}`;
  }
  const who = r.mode === "obj" ? "OBJ" : `BG${r.layer + 1}`;
  const overflow = r.report.overflows.length > 0 ? ` · ${r.report.overflows.length} overflow` : "";
  return `${who}: ${r.report.colors_used} colors · ${r.report.palettes_used} palettes · ${r.report.unique_tiles} tiles · ${r.report.vram_words} words${overflow}`;
}

export function VramTab({ frame }: { frame: FrameResult | null }) {
  const [bg, setBg] = useState<BgId>(0);
  const vram = ppuCore.vram();
  const reports = ppuCore.importReports();

  const mode = frame ? reg(frame, "BGMODE") & 0x07 : 1;
  const bpp = bgBpp(mode, bg);
  const bases = frame ? bgBases(frame, bg) : { mapBase: 0, charBase: 0, screenSize: 0 };
  const tiles = useMemo(() => {
    if (mode === 7) {
      return Array.from({ length: 32 }, (_, tile) => {
        const pixels = new Array<number>(64).fill(0);
        for (let i = 0; i < 64; i++) pixels[i] = (vram[(tile * 64 + i) & 0x7fff] ?? 0) >> 8;
        return pixels;
      });
    }
    const decode = bpp === 2 ? decodeTile2bpp : decodeTile4bpp;
    return Array.from({ length: 32 }, (_, tile) => decode(vram, bases.charBase, tile));
  }, [bases.charBase, bpp, mode, vram]);

  if (!frame) return <div className="insp-empty">waiting for frame…</div>;

  const map = Array.from({ length: 64 }, (_, i) =>
    mode === 7
      ? { tile: vram[i] & 0xff, pal: 0, prio: false, flipX: false, flipY: false }
      : tilemapEntry(vram[(bases.mapBase + i) & 0x7fff] ?? 0),
  );

  return (
    <div className="insp-scroll">
      <div className="vram-toolbar">
        <div className="insp-subhead">VRAM</div>
        <select value={bg} onChange={(e) => setBg(Number(e.target.value) as BgId)} disabled={mode === 7}>
          <option value={0}>BG1</option>
          <option value={1}>BG2</option>
          <option value={2}>BG3</option>
        </select>
      </div>

      <div className="vram-budget">
        {reports.length === 0 ? <span>no import reports this frame</span> : reports.map((r, i) => <span key={i}>{reportLine(r)}</span>)}
      </div>

      <div className="vram-meta">
        <span>mode {mode}</span>
        <span>{mode === 7 ? "Mode 7 interleaved" : `${bpp}bpp`}</span>
        <span>map ${formatValue(bases.mapBase)}</span>
        <span>char ${formatValue(bases.charBase)}</span>
        <span>screen {bases.screenSize}</span>
      </div>

      <div className="insp-subhead">TILES</div>
      <div className="vram-grid vram-grid--tiles">
        {tiles.map((pixels, tile) => (
          <div className="vram-tile" key={tile} title={`tile ${tile}`}>
            <TilePreview frame={frame} pixels={pixels} bpp={mode === 7 ? 4 : bpp} pal={0} />
            <span>{tile.toString(16).padStart(2, "0")}</span>
          </div>
        ))}
      </div>

      <div className="insp-subhead">TILEMAP</div>
      <div className="vram-grid vram-grid--map">
        {map.map((m, i) => (
          <div className={"vram-map-cell" + (m.prio ? " vram-map-cell--prio" : "")} key={i} title={`cell ${i}`}>
            <span>t{m.tile}</span>
            <span>p{m.pal}</span>
            <span>{m.flipX ? "H" : ""}{m.flipY ? "V" : ""}</span>
          </div>
        ))}
      </div>

      <div className="insp-subhead">CGRAM BANKS</div>
      <div className="palette-banks">
        {Array.from({ length: 16 }, (_, bank) => (
          <div className="palette-bank" key={bank}>
            <span>{bank.toString(16).toUpperCase()}</span>
            {Array.from({ length: 16 }, (_, i) => (
              <i key={i} style={{ background: cgram15ToCss(frame.cgram[bank * 16 + i] ?? 0) }} title={`$${(bank * 16 + i).toString(16).padStart(2, "0")}`} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
