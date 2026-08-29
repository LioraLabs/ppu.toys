/** Stories the presentational stage with a synthetic animated frame — no wasm,
 *  no transport. The plasma exercises the living-phosphor look: bright moving
 *  blobs leave green-tinted decay trails and fat scanlines. */
import { useMemo } from "react";
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

const Fixture = () => {
  const getFrame = usePlasmaFrame();
  return (
    <div style={{ width: 640, height: 512 }}>
      <HeroStage getFrame={getFrame} onFail={() => {}} />
    </div>
  );
};

export default <Fixture />;
