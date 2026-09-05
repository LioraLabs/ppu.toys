import { useState } from "react";
import { TouchController } from "./TouchController";
import { WIDTH, HEIGHT } from "../ppu/core";
import { PlayerControls, PlayerFrame } from "./ReadOnlyPlayer";

// The wired ReadOnlyPlayer drives the shared transport/core, so it can't be
// storied without booting wasm. We story its presentational half, PlayerFrame
// (pure markup: the letterbox + native-res pixelated canvas). The Default variant
// paints a static gradient through the canvas callback ref to prove the frame
// renders and stays pixelated on upscale — no transport, no core.
function paintGradient(canvas: HTMLCanvasElement | null) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const img = ctx.createImageData(WIDTH, HEIGHT);
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4;
      img.data[i] = (x / WIDTH) * 255; // R ramps across
      img.data[i + 1] = (y / HEIGHT) * 255; // G ramps down
      img.data[i + 2] = 128;
      img.data[i + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
}

const Default = () => {
  const [crt, setCrt] = useState(true);
  return <PlayerFrame canvasRef={paintGradient} crt={crt} onCrtToggle={() => setCrt((v) => !v)} />;
};

const Empty = () => <PlayerFrame />;

export function MobilePermalink() {
  const [playing, setPlaying] = useState(true);
  const [crt, setCrt] = useState(true);
  const controls = {
    playing,
    crt,
    onToggle: () => setPlaying((v) => !v),
    onCrtToggle: () => setCrt((v) => !v),
  };
  return (
    <div style={{ width: "100%", maxWidth: 393 }}>
      <div className="player-console">
        <PlayerFrame canvasRef={paintGradient} />
        <TouchController onChange={() => {}}>
          <PlayerControls {...controls} />
        </TouchController>
      </div>
    </div>
  );
}

export default {
  MobilePermalink,
  Default,
  Empty,
};
