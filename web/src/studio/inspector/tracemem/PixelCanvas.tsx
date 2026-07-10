import { useEffect, useRef, type MouseEvent } from "react";
import { canvasPos } from "./trace";

/** Draw an RGBA byte buffer 1:1 into a <canvas>; CSS scales it (pixelated).
 *  onPick/onHover report SOURCE-pixel coords. */
export function PixelCanvas({
  pixels,
  width,
  height,
  className,
  title,
  onPick,
  onHover,
}: {
  pixels: Uint8ClampedArray;
  width: number;
  height: number;
  className?: string;
  title?: string;
  onPick?: (x: number, y: number) => void;
  onHover?: (pos: { x: number; y: number } | null) => void;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const ctx = ref.current?.getContext("2d");
    if (!ctx) return;
    ctx.putImageData(new ImageData(pixels.slice(0, width * height * 4), width, height), 0, 0);
  }, [pixels, width, height]);
  const at = (e: MouseEvent<HTMLCanvasElement>) =>
    canvasPos(e.currentTarget.getBoundingClientRect(), e.clientX, e.clientY, width, height);
  return (
    <canvas
      ref={ref}
      width={width}
      height={height}
      className={className}
      title={title}
      onClick={onPick && ((e) => { const p = at(e); onPick(p.x, p.y); })}
      onMouseMove={onHover && ((e) => onHover(at(e)))}
      onMouseLeave={onHover && (() => onHover(null))}
    />
  );
}
