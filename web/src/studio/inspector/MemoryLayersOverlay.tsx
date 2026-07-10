import { HEIGHT, WIDTH, type ImportReport, type PlaneId } from "../../ppu/core";
import { ppuCore } from "../../ppu/instance";
import { bgMode, formatAddr, formatValue } from "./format";
import { useInspectorFrame } from "./useInspectorFrame";
import { Copyable, useCopyToast } from "./copyToast";
import { BlitCanvas } from "./BlitCanvas";
import { ModeBadge, PlaneSeg, TraceCaption, TraceChain } from "./tracemem/TraceChain";
import { CgramGrid, VramBar, VramLegend } from "./tracemem/MemoryPanels";
import { MODE_BPP, REGION_COLORS, cgramOwners, vramRegions } from "./tracemem/regions";
import { setLayerVisible, useLayerVis } from "./tracemem/stores";
import "./tracemem/tracemem.css";

const LAYERS: { id: PlaneId; name: string; color: string }[] = [
  { id: "bg1", name: "BG1", color: REGION_COLORS["bg1-char"] },
  { id: "bg2", name: "BG2", color: REGION_COLORS["bg2-char"] },
  { id: "bg3", name: "BG3", color: REGION_COLORS["bg3-char"] },
  { id: "bg4", name: "BG4", color: REGION_COLORS["bg4-char"] },
  { id: "obj", name: "OBJ", color: REGION_COLORS["obj-a"] },
];

function layerTag(id: PlaneId, mode: number): { tag: string; absent: boolean } {
  if (id === "obj") return { tag: "4bpp sprites", absent: false };
  const bpp = (MODE_BPP[mode] ?? MODE_BPP[1])[Number(id[2]) - 1];
  return bpp ? { tag: `${bpp}bpp tiles`, absent: false } : { tag: `absent in mode ${mode}`, absent: true };
}

function healthLine(r: ImportReport): { name: string; stats: string[]; warns: string[] } {
  if (r.mode === "m7") {
    return {
      name: `M7 BG${r.layer + 1}`,
      stats: [`${r.report.colors} col`, `${r.report.unique_tiles}/${r.report.tile_capacity} tiles`, `${r.report.map_tiles_w}×${r.report.map_tiles_h} map`],
      warns: r.report.overflow_tiles > 0 ? [`${r.report.overflow_tiles} tiles over capacity`] : [],
    };
  }
  const name = r.mode === "obj" ? "OBJ" : `BG${r.layer + 1}`;
  return {
    name,
    stats: [`${r.report.colors_used} col`, `${r.report.palettes_used} pal`, `${r.report.unique_tiles} tiles`, `${r.report.vram_words} words`],
    warns: r.report.overflows.map((o) => o.kind),
  };
}

/** Full-screen "Memory & Layers" overlay (Expand from Trace/Memory).
 *  Shares traceSelection + the memory panels with the docked tabs; the layer
 *  visibility toggles relocated here from the old LeftDock (M9 deviation). */
export function MemoryLayersOverlay({ onCollapse }: { onCollapse: () => void }) {
  const frame = useInspectorFrame();
  const { toast, copy } = useCopyToast();
  const vis = useLayerVis();
  const mode = bgMode(frame.registers);
  const vram = ppuCore.vram();
  const regions = vramRegions(frame.registers, vram);
  const owners = cgramOwners(frame.registers, vram, frame.oam);
  const reports = ppuCore.importReports();
  return (
    <div className="insp-overlay">
      <div className="insp-overlay-bar">
        <span className="insp-overlay-title">Memory &amp; Layers</span>
        <span className="insp-overlay-sub">VRAM · CGRAM · layer visibility</span>
        <div className="tb-spacer" />
        <button type="button" className="btn-ghost" onClick={onCollapse}>
          ↩ Collapse
        </button>
      </div>
      <div className="insp-overlay-body">
        <div className="tm-ov">
          <aside className="tm-ov-left">
            <div className="tm-ov-head">Priority stack</div>
            <div className="tm-layers">
              {LAYERS.map((l) => {
                const { tag, absent } = layerTag(l.id, mode);
                return (
                  <div key={l.id} className={"tm-layerrow" + (absent ? " tm-layerrow--absent" : "")}>
                    <i style={{ background: l.color }} />
                    <div className="tm-layername">
                      <div>{l.name}</div>
                      <div>{tag}</div>
                    </div>
                    <button
                      type="button"
                      className={"tm-pill " + (vis[l.id] ? "tm-pill--on" : "tm-pill--off")}
                      onClick={() => setLayerVisible(l.id, !vis[l.id])}
                    >
                      {vis[l.id] ? "on" : "off"}
                    </button>
                  </div>
                );
              })}
            </div>
            <div className="tm-ov-head">Import health</div>
            {reports.length === 0 && <div className="tm-health tm-faint">no imports this frame</div>}
            {reports.map((r, i) => {
              const h = healthLine(r);
              return (
                <div key={i} className={"tm-health" + (h.warns.length ? " tm-health--warn" : "")}>
                  <div className="tm-health-title">
                    <span className="tm-health-dot" />
                    {h.name}
                  </div>
                  <div className="tm-health-stats">
                    {h.stats.map((s) => (
                      <span key={s}>{s}</span>
                    ))}
                  </div>
                  {h.warns.map((w) => (
                    <div key={w} className="tm-health-warnline">
                      {w}
                    </div>
                  ))}
                </div>
              );
            })}
          </aside>
          <main className="tm-ov-center">
            <div className="tm-controls">
              <PlaneSeg />
              <ModeBadge frame={frame} />
            </div>
            <TraceCaption frame={frame} />
            <div className="tm-ov-head">Resolution chain</div>
            <TraceChain frame={frame} copy={copy} variant="overlay" />
            <div className="tm-ov-head">VRAM address space · 32,768 words</div>
            <VramBar regions={regions} onCopy={copy} />
            <VramLegend regions={regions} onCopy={copy} />
            <div className="tm-ov-head">CGRAM ownership</div>
            <CgramGrid cgram={frame.cgram} owners={owners} onCopy={copy} />
          </main>
          <aside className="tm-ov-right">
            <div className="tm-ov-head">Live output</div>
            <BlitCanvas pixels={frame.framebuffer} width={WIDTH} height={HEIGHT} className="tm-outcanvas" />
            <div className="tm-ov-head">Registers</div>
            <div className="tm-regrows">
              {frame.registers.map((r) => (
                <div key={r.addr} className="tm-regrow">
                  <Copyable label={formatAddr(r.addr)} onCopy={copy} />
                  <span className="tm-regname">{r.name}</span>
                  <span className="tm-regval">{formatValue(r.value)}</span>
                </div>
              ))}
            </div>
          </aside>
        </div>
        {toast}
      </div>
    </div>
  );
}
