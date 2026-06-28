import { useState } from "react";
import { ppuCore } from "../../ppu/instance";

interface Layer {
  id: string;
  name: string;
  tag: string;
  grad: string;
}

const LAYERS: Layer[] = [
  { id: "bg1", name: "BG1", tag: "sky · 4bpp · scroll", grad: "linear-gradient(180deg,#241640,#ff7a4d)" },
  { id: "bg2", name: "BG2", tag: "hills · 4bpp · scroll", grad: "linear-gradient(180deg,#23142e,#6a2a58)" },
  { id: "bg3", name: "BG3", tag: "clouds · 2bpp", grad: "linear-gradient(180deg,#46224e,#ffa766)" },
  { id: "bg4", name: "BG4", tag: "— · disabled", grad: "repeating-linear-gradient(45deg,#16181f,#16181f 4px,#1d2129 4px,#1d2129 8px)" },
  { id: "obj", name: "OBJ", tag: "sprites · 128 · 16×16", grad: "linear-gradient(135deg,#5ec97a,#5fc9e8)" },
];

/** LAYERS dock section: per-layer visibility toggles wired to the shared core.
 *  BG4 starts hidden to match the handoff's disabled-layer state. */
export function LayersPanel() {
  const [visible, setVisible] = useState<Record<string, boolean>>({
    bg1: true, bg2: true, bg3: true, bg4: false, obj: true,
  });

  const toggle = (id: string) => {
    setVisible((prev) => {
      const next = !prev[id];
      ppuCore.setLayerVisible(id, next);
      return { ...prev, [id]: next };
    });
  };

  return (
    <>
      <div className="section-header">LAYERS</div>
      {LAYERS.map((layer) => {
        const on = visible[layer.id];
        return (
          <div
            className={"layer-row" + (on ? "" : " layer-row--hidden")}
            key={layer.id}
            role="button"
            tabIndex={0}
            aria-pressed={on}
            aria-label={`Toggle ${layer.name} visibility`}
            onClick={() => toggle(layer.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                toggle(layer.id);
              }
            }}
          >
            <span className="vis-dot" />
            <span className="layer-thumb" style={{ background: layer.grad }} />
            <div className="layer-meta">
              <span className="layer-name">{layer.name}</span>
              <span className="layer-tag">{layer.tag}</span>
            </div>
          </div>
        );
      })}
    </>
  );
}
