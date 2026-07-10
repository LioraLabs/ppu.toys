import { useMemo, useState, type ReactNode } from "react";
import { HEIGHT, WIDTH, type FrameResult } from "../../../ppu/core";
import { ppuCore } from "../../../ppu/instance";
import { bgMode, formatAddr } from "../format";
import { BlitCanvas } from "../BlitCanvas";
import {
  TRACE_PLANES,
  bgr555Label,
  bgr555ToHex,
  cgLabel,
  resolvePaletteEntry,
  spriteAt,
  tileToRgba,
  tileWords,
  traceCaption,
} from "./trace";
import { Copyable } from "../copyToast";
import { CgramPoke } from "../../pokes/CgramPoke";
import { pickPaletteIdx, selectObj, selectPixel, selectPlane, useTraceSelection } from "./stores";

/** Plane segmented control (user-selected; shared store). */
export function PlaneSeg() {
  const sel = useTraceSelection();
  return (
    <div className="tm-seg" role="tablist">
      {TRACE_PLANES.map((p) => (
        <button
          key={p}
          type="button"
          className={sel.plane === p ? "tm-seg--on" : ""}
          onClick={() => selectPlane(p)}
        >
          {p.toUpperCase()}
        </button>
      ))}
    </div>
  );
}

/** Mode badge — REPORTED from the live frame (M9 deviation), not a selector. */
export function ModeBadge({ frame }: { frame: FrameResult }) {
  return <span className="tm-mode-badge">MODE {bgMode(frame.registers)}</span>;
}

export function TraceCaption({ frame }: { frame: FrameResult }) {
  const sel = useTraceSelection();
  return <div className="tm-caption">{traceCaption(sel.plane, bgMode(frame.registers))}</div>;
}

function Stage({ n, title, cls, children, overlay }: { n: number; title: string; cls: string; children: ReactNode; overlay: boolean }) {
  return (
    <div className={`tm-stage ${cls}`}>
      <div className="tm-stage-title">{overlay ? `${n} · ${title}` : title}</div>
      <div className="tm-card">{children}</div>
    </div>
  );
}

const Arrow = () => <div className="tm-arrow">→</div>;

export function TraceChain({ frame, copy, variant }: { frame: FrameResult; copy: (label: string) => void; variant: "tab" | "overlay" }) {
  const sel = useTraceSelection();
  const overlay = variant === "overlay";
  const isObj = sel.plane === "obj";
  const layer = isObj ? 0 : Number(sel.plane[2]);
  const [hover, setHover] = useState<{ x: number; y: number } | null>(null);
  const [swatchPokeOpen, setSwatchPokeOpen] = useState(false);

  // Per-frame seam queries — the frame object identity changes every frame,
  // so `frame` in the deps re-queries the core exactly once per rendered frame.
  const minimap = useMemo(() => ppuCore.layerView(sel.plane), [frame, sel.plane]);
  const bg = useMemo(
    () => (isObj ? null : ppuCore.traceBgPixel(layer, sel.x, sel.y)),
    [frame, isObj, layer, sel.x, sel.y],
  );
  const obj = useMemo(() => (isObj ? ppuCore.traceObj(sel.objIndex) : null), [frame, isObj, sel.objIndex]);

  const mode = bgMode(frame.registers);
  if (!isObj && !bg) {
    return <div className="tm-note">BG{layer} does not exist in mode {mode} — pick another plane.</div>;
  }
  if (isObj && !obj) return <div className="tm-note">no sprite at OAM #{sel.objIndex}</div>;

  // Unified stage-2/3 inputs. BgTrace.pixels is tileSize x tileSize indices
  // (mode 7: always 8x8 — core.ts contract); ObjTrace.pixels is width x height.
  const pixels = isObj ? obj!.pixels : bg!.tile.pixels;
  const tileW = isObj ? obj!.width : bg!.regs.mode === 7 ? 8 : bg!.regs.tileSize;
  const tileH = isObj ? obj!.height : bg!.regs.mode === 7 ? 8 : bg!.regs.tileSize;
  const paletteBase = isObj ? obj!.paletteBase : bg!.tile.paletteBase;
  const bpp = isObj ? 4 : bg!.regs.bpp;
  const directColor = !isObj && bg!.regs.directColor && bpp === 8;
  const charAddr = isObj ? obj!.charAddr : bg!.tile.charAddr;
  const usedIdx = new Set(pixels);
  const tileRgba = tileToRgba(pixels, tileW, tileH, frame.cgram, paletteBase, directColor);

  // Effective palette pick: explicit strip pick, else the traced pixel; the
  // core's exact resolution (bgr555/cgramIndex) wins when it applies.
  const exact = !isObj && sel.pickedIdx === null ? bg!.pixel : null;
  const pickIdx = sel.pickedIdx ?? (isObj ? 0 : bg!.pixel?.index ?? 0);
  const resolved = resolvePaletteEntry(pickIdx, paletteBase, frame.cgram, directColor);
  const bgr = exact ? exact.bgr555 : resolved.bgr555;
  const cgAddr = exact ? exact.cgramIndex ?? null : directColor ? null : resolved.cgAddr;

  // Source rows
  const srcRows: [string, ReactNode][] = isObj
    ? [
        ["OBSEL char", <Copyable label={formatAddr(obj!.charBase)} onCopy={copy} />],
        ["sprite", `#${obj!.index}`],
        ["pos", `${obj!.oam.x},${obj!.oam.y}`],
        ["size", `${obj!.width}×${obj!.height}`],
        ["pal / prio", `${obj!.oam.pal} / ${obj!.oam.prio}`],
        ["flip", `${obj!.oam.flipX ? "H" : "–"}${obj!.oam.flipY ? "V" : "–"}`],
      ]
    : [
        ["map", <Copyable label={formatAddr(bg!.regs.mapBase)} onCopy={copy} />],
        ["char", <Copyable label={formatAddr(bg!.regs.charBase)} onCopy={copy} />],
        ["entry @", <Copyable label={formatAddr(bg!.tile.mapAddr)} onCopy={copy} />],
        ["scroll", `${bg!.regs.scrollX},${bg!.regs.scrollY}`],
        ["tile size", `${bg!.regs.tileSize}px`],
        ["mosaic", bg!.regs.mosaic > 1 ? `${bg!.regs.mosaic}px` : "off"],
      ];

  // Minimap selection/hover boxes (percent-positioned divs over the canvas).
  const ts = isObj ? 8 : bg!.regs.tileSize;
  const selX = isObj ? obj!.oam.x : sel.x - (sel.x % ts);
  const selY = isObj ? obj!.oam.y : sel.y - (sel.y % ts);
  const selW = isObj ? obj!.width : ts;
  const selH = isObj ? obj!.height : ts;
  const boxStyle = (x: number, y: number, w: number, h: number) => ({
    left: `${(x / WIDTH) * 100}%`,
    top: `${(y / HEIGHT) * 100}%`,
    width: `${(w / WIDTH) * 100}%`,
    height: `${(h / HEIGHT) * 100}%`,
  });

  /** Swatch + idx/CG/BGR555/rgb copyables for the effective pick (inside the
   *  palette stage in the tab; its own "CGRAM COLOR" stage in the overlay). */
  const palettePick = (
    <div className="tm-palpick">
      <span className="tm-swatch-wrap">
        <button
          type="button"
          className="tm-swatch tm-swatch--btn"
          style={{ backgroundColor: bgr555ToHex(bgr) }}
          disabled={cgAddr === null}
          title={cgAddr === null ? "direct color — CGRAM bypassed, nothing to poke" : `poke ${cgLabel(cgAddr)}`}
          onClick={() => setSwatchPokeOpen(true)}
        />
        {swatchPokeOpen && cgAddr !== null && (
          <CgramPoke index={cgAddr} current={bgr} onClose={() => setSwatchPokeOpen(false)} />
        )}
      </span>
      <div className="tm-meta">
        <div>
          idx <span className="tm-strong">{pickIdx}</span>
          {cgAddr !== null && (
            <>
              {" @ "}
              <Copyable label={cgLabel(cgAddr)} onCopy={copy} />
            </>
          )}
        </div>
        <div>
          BGR555 <Copyable label={bgr555Label(bgr)} onCopy={copy} cyan />
        </div>
        <div>
          rgb <Copyable label={bgr555ToHex(bgr)} onCopy={copy} />
        </div>
        {pickIdx === 0 && <div className="tm-faint">index 0 = transparent</div>}
      </div>
    </div>
  );

  return (
    <>
      <div className="tm-chain">
        <Stage n={1} title={isObj ? "SOURCE (OAM)" : "SOURCE (TILEMAP)"} cls="tm-stage--source" overlay={overlay}>
          <div className="tm-minimap-wrap">
            <BlitCanvas
              pixels={minimap}
              width={WIDTH}
              height={HEIGHT}
              className="tm-minimap"
              title={isObj ? "click a sprite" : "click to trace a pixel"}
              onDown={(x, y) => {
                if (isObj) {
                  const hit = spriteAt(ppuCore, x, y);
                  if (hit) selectObj(hit.index);
                } else {
                  selectPixel(x, y);
                }
              }}
              onHover={setHover}
            />
            {hover && !isObj && (
              <div className="tm-hoverbox" style={boxStyle(hover.x - (hover.x % ts), hover.y - (hover.y % ts), ts, ts)} />
            )}
            <div className="tm-selbox" style={boxStyle(selX, selY, selW, selH)} />
          </div>
          <div className="tm-rows">
            {srcRows.map(([l, v]) => (
              <div className="tm-row" key={l}>
                <span className="tm-row-l">{l}</span>
                <span className="tm-row-v">{v}</span>
              </div>
            ))}
          </div>
        </Stage>
        <Arrow />
        <Stage n={2} title="CHAR (VRAM)" cls="tm-stage--char" overlay={overlay}>
          <div className="tm-tilewrap">
            <BlitCanvas pixels={tileRgba} width={tileW} height={tileH} className="tm-tile" />
            {!isObj && bg!.pixel && (
              <div
                className="tm-pxbox"
                style={{
                  left: `${(bg!.pixel.fx / tileW) * 100}%`,
                  top: `${(bg!.pixel.fy / tileH) * 100}%`,
                  width: `${100 / tileW}%`,
                  height: `${100 / tileH}%`,
                }}
              />
            )}
          </div>
          <div className="tm-meta">
            {!isObj && (
              <div className="tm-strong">
                tile {bg!.tile.tile} · pal {bg!.tile.pal}
                {bg!.tile.prio ? " · prio" : ""}
                {bg!.tile.flipX ? " · H" : ""}
                {bg!.tile.flipY ? " · V" : ""}
              </div>
            )}
            {isObj && <div className="tm-strong">OAM tile {obj!.oam.tile}</div>}
            <div>
              char @ <Copyable label={formatAddr(charAddr)} onCopy={copy} />
            </div>
            <div className="tm-faint">
              {bpp}bpp · {tileWords(bpp, tileW, tileH)} words
            </div>
          </div>
        </Stage>
        <Arrow />
        <Stage n={3} title="SUB-PALETTE" cls="tm-stage--pal" overlay={overlay}>
          <div className="tm-palstrip">
            {Array.from({ length: 16 }, (_, i) => {
              const e = resolvePaletteEntry(i, paletteBase, frame.cgram, directColor);
              return (
                <button
                  key={i}
                  type="button"
                  className={(usedIdx.has(i) ? "tm-pal--used " : "") + (i === pickIdx ? "tm-pal--sel" : "")}
                  style={{ backgroundColor: bgr555ToHex(e.bgr555) }}
                  title={directColor ? `direct ${i}` : cgLabel(paletteBase + i)}
                  onClick={() => pickPaletteIdx(i)}
                />
              );
            })}
          </div>
          <div className="tm-meta">
            <div>
              base <Copyable label={cgLabel(paletteBase)} onCopy={copy} />
            </div>
            <div className="tm-faint">bordered = used by this {isObj ? "sprite" : "tile"}</div>
          </div>
          {!overlay && palettePick}
        </Stage>
        {overlay && (
          <>
            <Arrow />
            <Stage n={4} title="CGRAM COLOR" cls="tm-stage--char" overlay>
              {palettePick}
            </Stage>
            <Arrow />
            <Stage n={5} title="OUTPUT" cls="tm-stage--out" overlay>
              <BlitCanvas pixels={frame.framebuffer} width={WIDTH} height={HEIGHT} className="tm-outcanvas" />
              <div className="tm-meta">
                lands at screen{" "}
                <span className="tm-strong">
                  {isObj ? `${obj!.oam.x},${obj!.oam.y}` : `${sel.x},${sel.y}`}
                </span>
              </div>
            </Stage>
          </>
        )}
      </div>
      {directColor && <div className="tm-note">direct color — CGRAM bypassed; the 8-bit index maps straight to BGR555</div>}
      {!isObj && !bg!.regs.visible && <div className="tm-note">layer hidden (visibility toggle in Memory &amp; Layers)</div>}
    </>
  );
}
