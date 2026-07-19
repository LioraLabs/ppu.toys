import { BlitCanvas } from "./BlitCanvas";

// BlitCanvas is a pure presentational canvas blit — it draws an RGBA byte
// buffer 1:1 into a <canvas> and never touches the core. All pixel CONTENT
// comes from a caller-supplied buffer; here it's a synthetic gradient, no
// rasterizer involved.
const W = 64;
const H = 64;
const px = new Uint8ClampedArray(W * H * 4);
for (let y = 0; y < H; y++) {
  for (let x = 0; x < W; x++) {
    const i = (y * W + x) * 4;
    px[i] = Math.floor((x / (W - 1)) * 255); // r by x
    px[i + 1] = Math.floor((y / (H - 1)) * 255); // g by y
    px[i + 2] = 128; // b constant
    px[i + 3] = 255; // a
  }
}

// Scales the native 64x64 buffer up for visibility; BlitCanvas itself has no
// sizing opinion (that's the caller's CSS, same as the app's compose/tracemem
// panels).
const scaleStyle = `.blitcanvas-story { width: 256px; height: 256px; image-rendering: pixelated; border: 1px solid #333; display: block; }`;

const Gradient = () => (
  <>
    <style>{scaleStyle}</style>
    <BlitCanvas className="blitcanvas-story" pixels={px} width={W} height={H} title="synthetic gradient" />
  </>
);

const WithOverlay = () => (
  <>
    <style>{scaleStyle}</style>
    <BlitCanvas
      className="blitcanvas-story"
      pixels={px}
      width={W}
      height={H}
      title="synthetic gradient with overlay edge lines"
      overlay={(ctx) => {
        ctx.fillStyle = "#ff9540";
        ctx.fillRect(0, 0, 1, H);
        ctx.fillRect(W - 1, 0, 1, H);
      }}
    />
  </>
);

export default {
  Gradient,
  WithOverlay,
};
