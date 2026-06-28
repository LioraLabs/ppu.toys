const LAYERS = [
  { name: "BG1", tag: "sky · 4bpp · scroll", visible: true, grad: "linear-gradient(180deg,#241640,#ff7a4d)" },
  { name: "BG2", tag: "hills · 4bpp · scroll", visible: true, grad: "linear-gradient(180deg,#23142e,#6a2a58)" },
  { name: "BG3", tag: "clouds · 2bpp", visible: true, grad: "linear-gradient(180deg,#46224e,#ffa766)" },
  { name: "BG4", tag: "— · disabled", visible: false, grad: "repeating-linear-gradient(45deg,#16181f,#16181f 4px,#1d2129 4px,#1d2129 8px)" },
  { name: "OBJ", tag: "sprites · 128 · 16×16", visible: true, grad: "linear-gradient(135deg,#5ec97a,#5fc9e8)" },
];

function hslHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(255 * c).toString(16).padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

const SWATCHES: string[] = [];
for (let p = 0; p < 16; p++) {
  for (let i = 0; i < 16; i++) {
    if (i === 0) {
      SWATCHES.push("#05060a");
      continue;
    }
    const hue = (p * 22 + 330 + i * 3) % 360;
    const sat = p % 5 === 0 ? 0.1 : 0.5;
    const light = 0.1 + i * 0.052;
    SWATCHES.push(hslHex(hue, sat, light));
  }
}

export function LeftDock() {
  return (
    <aside className="dock">
      <div className="section-header">LAYERS</div>
      {LAYERS.map((layer) => (
        <div
          className={"layer-row" + (layer.visible ? "" : " layer-row--hidden")}
          key={layer.name}
        >
          <span className="vis-dot" />
          <span className="layer-thumb" style={{ background: layer.grad }} />
          <div className="layer-meta">
            <span className="layer-name">{layer.name}</span>
            <span className="layer-tag">{layer.tag}</span>
          </div>
        </div>
      ))}
      <div className="cgram-section">
        <div className="section-header">CGRAM</div>
        <div className="palette-grid">
          {SWATCHES.map((color, idx) => (
            <div className="swatch" key={idx} style={{ background: color }} />
          ))}
        </div>
        <div className="palette-footer">
          <span>
            pal <span className="pal-num">0</span> · 16×16
          </span>
          <span className="bits">15-bit</span>
        </div>
      </div>
    </aside>
  );
}
