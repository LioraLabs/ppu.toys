import { useEffect, useRef, type PointerEvent } from "react";
import { HEIGHT, WIDTH } from "../../../ppu/core";

interface Props {
  /** WIDTH*HEIGHT*4 RGBA, JS-owned (core buffers are copies already). */
  pixels: Uint8ClampedArray;
  /** Chrome drawn after the blit (window edge lines). */
  overlay?: (ctx: CanvasRenderingContext2D) => void;
  className?: string;
  title?: string;
  onPixelDown?: (x: number, y: number) => void;
  onPixelDrag?: (x: number, y: number) => void;
  onPixelUp?: () => void;
}

/** Dumb 256x224 preview: blits an RGBA buffer every render and reports pointer
 *  events in framebuffer coordinates. All pixel CONTENT comes from the core —
 *  this component never composites. */
export function PreviewCanvas({
  pixels,
  overlay,
  className,
  title,
  onPixelDown,
  onPixelDrag,
  onPixelUp,
}: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  // One reused ImageData per canvas, filled via data.set() — same pattern as
  // the output presenter's Canvas2D path (and it sidesteps the lib.dom
  // ImageData(buffer) ArrayBufferLike incompatibility).
  const image = useRef<ImageData | null>(null);
  useEffect(() => {
    const ctx = ref.current?.getContext("2d");
    if (!ctx) return;
    const img = (image.current ??= new ImageData(WIDTH, HEIGHT));
    img.data.set(pixels);
    ctx.putImageData(img, 0, 0);
    overlay?.(ctx);
  });
  const toPixel = (e: PointerEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const clamp = (v: number, hi: number) => Math.min(hi, Math.max(0, Math.floor(v)));
    return {
      x: clamp(((e.clientX - r.left) / r.width) * WIDTH, WIDTH - 1),
      y: clamp(((e.clientY - r.top) / r.height) * HEIGHT, HEIGHT - 1),
    };
  };
  return (
    <canvas
      ref={ref}
      width={WIDTH}
      height={HEIGHT}
      className={className}
      title={title}
      onPointerDown={
        onPixelDown
          ? (e) => {
              try {
                e.currentTarget.setPointerCapture(e.pointerId);
              } catch {
                // Capture is a nicety (keeps drags alive off-canvas); an
                // invalid pointer id must not swallow the click itself.
              }
              const { x, y } = toPixel(e);
              onPixelDown(x, y);
            }
          : undefined
      }
      onPointerMove={
        onPixelDrag
          ? (e) => {
              if (e.buttons & 1) {
                const { x, y } = toPixel(e);
                onPixelDrag(x, y);
              }
            }
          : undefined
      }
      onPointerUp={onPixelUp ? () => onPixelUp() : undefined}
    />
  );
}
