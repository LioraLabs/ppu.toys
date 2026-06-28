import { useEffect, useRef, useState } from "react";
import { PpuCore, WIDTH, HEIGHT } from "./ppu/core";
import { MockPpuCore } from "./ppu/mock";
import { createWasmPpuCore } from "./ppu/wasm";

const USE_WASM = import.meta.env.VITE_USE_WASM === "1";

export default function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [core, setCore] = useState<PpuCore | null>(null);

  useEffect(() => {
    let alive = true;
    if (USE_WASM) {
      createWasmPpuCore().then((c) => alive && setCore(c));
    } else {
      setCore(new MockPpuCore());
    }
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!core || !canvasRef.current) return;
    const ctx = canvasRef.current.getContext("2d")!;
    const start = performance.now();
    let raf = 0;
    let frame = 0;
    const loop = (now: number) => {
      const t = (now - start) / 1000;
      const { framebuffer } = core.frame(t, frame++);
      ctx.putImageData(new ImageData(framebuffer as Uint8ClampedArray<ArrayBuffer>, WIDTH, HEIGHT), 0, 0);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [core]);

  return (
    <canvas
      ref={canvasRef}
      width={WIDTH}
      height={HEIGHT}
      style={{
        imageRendering: "pixelated",
        width: WIDTH * 2,
        height: HEIGHT * 2,
        background: "#000",
      }}
    />
  );
}
