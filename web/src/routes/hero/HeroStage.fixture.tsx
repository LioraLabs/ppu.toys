/** Stories the presentational stage with a synthetic animated frame — no wasm,
 *  no transport. The plasma exercises the living-phosphor look: bright moving
 *  blobs leave green-tinted decay trails and fat scanlines. */
import { useMemo } from "react";
import { useFixtureInput } from "react-cosmos/client";
import { HeroStage } from "./HeroStage";
import { WIDTH, HEIGHT } from "../../ppu/core";

function usePlasmaFrame(): () => Uint8ClampedArray {
  return useMemo(() => {
    const fb = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
    return () => {
      const t = performance.now() / 1000;
      for (let y = 0; y < HEIGHT; y++) {
        for (let x = 0; x < WIDTH; x++) {
          const i = (y * WIDTH + x) * 4;
          const v =
            Math.sin(x / 18 + t * 2) + Math.sin(y / 14 - t * 1.4) + Math.sin((x + y) / 30 + t);
          fb[i] = 40 + 60 * (v + 3) * 0.5;
          fb[i + 1] = 20 + 40 * (Math.sin(v + t) + 1);
          fb[i + 2] = 80 + 80 * (Math.cos(v - t) + 1) * 0.5;
          fb[i + 3] = 255;
        }
      }
      // A bright moving sprite to show off trails + beam bloom.
      const sx = Math.floor(128 + 90 * Math.sin(t * 2.2));
      const sy = Math.floor(112 + 70 * Math.cos(t * 1.7));
      for (let dy = -6; dy <= 6; dy++) {
        for (let dx = -6; dx <= 6; dx++) {
          const x = sx + dx;
          const y = sy + dy;
          if (x < 0 || x >= WIDTH || y < 0 || y >= HEIGHT || dx * dx + dy * dy > 36) continue;
          const i = (y * WIDTH + x) * 4;
          fb[i] = fb[i + 1] = fb[i + 2] = 255;
        }
      }
      return fb;
    };
  }, []);
}

/** "tune" (control panel toggle) overlays a lil-gui panel on the live scene;
 *  its "dump" button logs a LAYOUTS-shaped block to paste into HeroStage. */
/** Synthetic clip-thumb for the cart label: a gradient data URL, no network. */
function fakeThumb(): string {
  const c = document.createElement("canvas");
  c.width = 256;
  c.height = 224;
  const ctx = c.getContext("2d")!;
  const grad = ctx.createLinearGradient(0, 0, 256, 224);
  grad.addColorStop(0, "#7b3fe4");
  grad.addColorStop(0.5, "#e04a7a");
  grad.addColorStop(1, "#e8a33c");
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, 256, 224);
  return c.toDataURL();
}

const Fixture = ({ width, height }: { width: number; height: number }) => {
  const getFrame = usePlasmaFrame();
  const [tune] = useFixtureInput("tune", false);
  const cart = useMemo(() => ({ thumbUrl: fakeThumb(), avatarUrl: null, handle: "ada" }), []);
  return (
    <div style={{ width, height }}>
      <HeroStage getFrame={getFrame} onFail={() => {}} cart={cart} tune={tune} />
    </div>
  );
};

export default {
  desktop: <Fixture width={640} height={512} />,
  mobile: <Fixture width={320} height={600} />,
};
