import { useEffect, useRef, type PointerEvent } from "react";

/** Map a client-space point on a scaled canvas to SOURCE-pixel coords,
 *  clamped into range. Exported for unit tests. */
export function canvasPos(
  rect: { left: number; top: number; width: number; height: number },
  clientX: number,
  clientY: number,
  srcW: number,
  srcH: number,
): { x: number; y: number } {
  const clamp = (v: number, hi: number) => Math.min(Math.max(v, 0), hi);
  return {
    x: clamp(Math.floor(((clientX - rect.left) / rect.width) * srcW), srcW - 1),
    y: clamp(Math.floor(((clientY - rect.top) / rect.height) * srcH), srcH - 1),
  };
}

interface Props {
  /** width*height*4 RGBA, JS-owned (core buffers are copies already). */
  pixels: Uint8ClampedArray;
  width: number;
  height: number;
  /** Chrome drawn after the blit (window edge lines, …). */
  overlay?: (ctx: CanvasRenderingContext2D) => void;
  className?: string;
  title?: string;
  /** Press, in SOURCE-pixel coords. When `onDrag` is also given, the pointer is
   *  captured on press so the drag survives leaving the canvas. */
  onDown?: (x: number, y: number) => void;
  /** Move with the primary button held, in SOURCE-pixel coords. */
  onDrag?: (x: number, y: number) => void;
  onUp?: () => void;
  /** Any move, in SOURCE-pixel coords; null when the pointer leaves. */
  onHover?: (pos: { x: number; y: number } | null) => void;
}

/** The one canvas-blit component: draws an RGBA byte buffer 1:1 into a
 *  <canvas> (CSS scales it, pixelated) and reports pointer events in
 *  source-pixel coordinates. All pixel CONTENT comes from the core — this
 *  component never composites. Serves the Compose previews, the Trace
 *  minimap/tile/output views and the Memory & Layers overlay. */
export function BlitCanvas({
  pixels,
  width,
  height,
  overlay,
  className,
  title,
  onDown,
  onDrag,
  onUp,
  onHover,
}: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  // One reused ImageData per canvas (recreated on a size change), filled via
  // data.set() — same pattern as the output presenter's Canvas2D path (and it
  // sidesteps the lib.dom ImageData(buffer) ArrayBufferLike incompatibility).
  const image = useRef<ImageData | null>(null);
  useEffect(() => {
    const ctx = ref.current?.getContext("2d");
    if (!ctx) return;
    let img = image.current;
    if (!img || img.width !== width || img.height !== height) {
      img = image.current = new ImageData(width, height);
    }
    img.data.set(pixels.subarray(0, width * height * 4));
    ctx.putImageData(img, 0, 0);
    overlay?.(ctx);
  });
  const toPixel = (e: PointerEvent<HTMLCanvasElement>) =>
    canvasPos(e.currentTarget.getBoundingClientRect(), e.clientX, e.clientY, width, height);
  return (
    <canvas
      ref={ref}
      width={width}
      height={height}
      className={className}
      title={title}
      onPointerDown={
        onDown
          ? (e) => {
              if (onDrag) {
                try {
                  e.currentTarget.setPointerCapture(e.pointerId);
                } catch {
                  // Capture is a nicety (keeps drags alive off-canvas); an
                  // invalid pointer id must not swallow the press itself.
                }
              }
              const { x, y } = toPixel(e);
              onDown(x, y);
            }
          : undefined
      }
      onPointerMove={
        onDrag || onHover
          ? (e) => {
              const p = toPixel(e);
              if (onDrag && e.buttons & 1) onDrag(p.x, p.y);
              onHover?.(p);
            }
          : undefined
      }
      onPointerUp={onUp ? () => onUp() : undefined}
      onPointerLeave={onHover ? () => onHover(null) : undefined}
    />
  );
}
