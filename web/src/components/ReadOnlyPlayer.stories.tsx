import type { Story, StoryDefault } from "@ladle/react";
import { WIDTH, HEIGHT } from "../ppu/core";
import { PlayerFrame } from "./ReadOnlyPlayer";

// The wired ReadOnlyPlayer drives the shared transport/core, so it can't be
// storied without booting wasm. We story its presentational half, PlayerFrame
// (pure markup: the letterbox + native-res pixelated canvas). The Default variant
// paints a static gradient through the canvas callback ref to prove the frame
// renders and stays pixelated on upscale — no transport, no core.
export default {
  title: "Components/ReadOnlyPlayer",
} satisfies StoryDefault;

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

export const Default: Story = () => <PlayerFrame canvasRef={paintGradient} />;

export const Empty: Story = () => <PlayerFrame />;
