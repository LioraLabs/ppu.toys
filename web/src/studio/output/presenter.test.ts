import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { Presenter } from "./presenter";

/** Minimal ImageData stand-in — the node test env has no DOM ImageData, which the
 *  Canvas2D fallback constructs. */
class FakeImageData {
  data: Uint8ClampedArray;
  constructor(public width: number, public height: number) {
    this.data = new Uint8ClampedArray(width * height * 4);
  }
}

/** A WebGL context that reports EVERY shader compile as failed with a null info
 *  log — the exact signature of a degraded/lost context (browser WebGL disabled
 *  or GPU blocklisted) seen on real hardware. The shaders are valid GLSL ES; the
 *  context itself is what's broken. */
function compileFailGl(): WebGLRenderingContext {
  return {
    VERTEX_SHADER: 0,
    FRAGMENT_SHADER: 1,
    COMPILE_STATUS: 2,
    LINK_STATUS: 3,
    createProgram: () => ({}),
    createShader: () => ({}),
    shaderSource: () => {},
    compileShader: () => {},
    getShaderParameter: () => false, // compile "fails"
    getShaderInfoLog: () => null, // ...with no log, as observed in the wild
    deleteShader: () => {},
    deleteProgram: () => {},
    attachShader: () => {},
    linkProgram: () => {},
    getProgramParameter: () => true,
    useProgram: () => {},
    getExtension: () => ({ loseContext: () => {} }),
  } as unknown as WebGLRenderingContext;
}

function fakeCanvas(gl: WebGLRenderingContext): HTMLCanvasElement {
  const ctx2d = { putImageData: () => {} };
  return {
    getContext: (kind: string) =>
      kind === "webgl" || kind === "experimental-webgl"
        ? gl
        : kind === "2d"
          ? ctx2d
          : null,
  } as unknown as HTMLCanvasElement;
}

describe("Presenter — a WebGL failure is non-fatal", () => {
  beforeEach(() => {
    (globalThis as unknown as { ImageData: unknown }).ImageData = FakeImageData;
  });
  afterEach(() => {
    delete (globalThis as unknown as { ImageData?: unknown }).ImageData;
  });

  it("returns false (does not throw) when shader compilation fails, signalling a remount", () => {
    const p = new Presenter();
    let result: boolean | undefined;
    expect(() => {
      result = p.init(fakeCanvas(compileFailGl()));
    }).not.toThrow();
    // false tells OutputCanvas the canvas is tainted — remount fresh for 2D.
    expect(result).toBe(false);
  });

  it("forceCanvas2d skips WebGL entirely and uses the pristine canvas's 2D context", () => {
    let got2d = false;
    let askedWebgl = false;
    const freshCanvas = {
      getContext: (kind: string) => {
        if (kind === "2d") {
          got2d = true;
          return { putImageData: () => {} };
        }
        askedWebgl = true;
        return null;
      },
    } as unknown as HTMLCanvasElement;
    const p = new Presenter();
    let result: boolean | undefined;
    expect(() => {
      result = p.init(freshCanvas, true);
    }).not.toThrow();
    expect(askedWebgl).toBe(false); // never tainted the fresh canvas with webgl
    expect(got2d).toBe(true); // drew straight to its 2D context
    expect(result).toBe(false);
  });

  it("renders without throwing in the 2D fallback", () => {
    const p = new Presenter();
    p.init(
      {
        getContext: (k: string) => (k === "2d" ? { putImageData: () => {} } : null),
      } as unknown as HTMLCanvasElement,
      true,
    );
    const fb = new Uint8ClampedArray(256 * 224 * 4);
    expect(() =>
      p.render(fb, { crt: false, scanline: false, pixelGrid: false }),
    ).not.toThrow();
  });
});
