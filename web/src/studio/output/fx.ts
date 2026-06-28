/** Toggleable present-pass effects, layered over the integer upscale. */
export interface PresentFx {
  crt: boolean;
  scanline: boolean;
  pixelGrid: boolean;
}

/** Effects off = a crisp, integer-upscaled native-res image (no blur). */
export const DEFAULT_FX: PresentFx = { crt: false, scanline: false, pixelGrid: false };

export const FX_STORAGE_KEY = "ppu.toys:present-fx";

const bool = (v: unknown): boolean => v === true;

/** Parse persisted JSON into a PresentFx, tolerating null/garbage/extra keys. */
export function parseFx(raw: string | null): PresentFx {
  if (!raw) return DEFAULT_FX;
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    return { crt: bool(o.crt), scanline: bool(o.scanline), pixelGrid: bool(o.pixelGrid) };
  } catch {
    return DEFAULT_FX;
  }
}

/** Load persisted effects (SSR/no-storage safe). */
export function loadFx(): PresentFx {
  try {
    return parseFx(localStorage.getItem(FX_STORAGE_KEY));
  } catch {
    return DEFAULT_FX;
  }
}

/** Persist effects (best-effort; storage may be unavailable). */
export function saveFx(fx: PresentFx): void {
  try {
    localStorage.setItem(FX_STORAGE_KEY, JSON.stringify(fx));
  } catch {
    /* ignore */
  }
}

/** Shader uniform values (0/1 floats) for a present-FX state. */
export function fxUniforms(fx: PresentFx): { uCrt: number; uScanline: number; uGrid: number } {
  return { uCrt: fx.crt ? 1 : 0, uScanline: fx.scanline ? 1 : 0, uGrid: fx.pixelGrid ? 1 : 0 };
}
