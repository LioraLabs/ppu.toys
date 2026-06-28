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

/** A working WebGL context whose shader compiles succeed UNLESS the context has
 *  been lost — mirroring real behaviour where compiling on a lost context fails
 *  with a null log. loseContext() flips _lost. */
function workingGl(): WebGLRenderingContext & { _lost: boolean } {
  const gl = {
    _lost: false,
    VERTEX_SHADER: 0,
    FRAGMENT_SHADER: 1,
    COMPILE_STATUS: 2,
    LINK_STATUS: 3,
    createProgram: () => ({}),
    createShader: () => ({}),
    shaderSource: () => {},
    compileShader: () => {},
    getShaderParameter: () => !gl._lost,
    getShaderInfoLog: () => (gl._lost ? null : ""),
    deleteShader: () => {},
    deleteProgram: () => {},
    attachShader: () => {},
    linkProgram: () => {},
    getProgramParameter: () => !gl._lost,
    getProgramInfoLog: () => "",
    useProgram: () => {},
    createBuffer: () => ({}),
    bindBuffer: () => {},
    bufferData: () => {},
    getAttribLocation: () => 0,
    enableVertexAttribArray: () => {},
    vertexAttribPointer: () => {},
    createTexture: () => ({}),
    bindTexture: () => {},
    texParameteri: () => {},
    getUniformLocation: () => ({}),
    uniform1i: () => {},
    uniform2f: () => {},
    uniform1f: () => {},
    deleteBuffer: () => {},
    deleteTexture: () => {},
    getExtension: (name: string) =>
      name === "WEBGL_lose_context" ? { loseContext: () => { gl._lost = true; } } : null,
  };
  return gl as unknown as WebGLRenderingContext & { _lost: boolean };
}

/** A canvas that returns the SAME context object on every getContext call, as a
 *  real canvas does (its context is created once and reused). */
function reusableGlCanvas(gl: WebGLRenderingContext): HTMLCanvasElement {
  return {
    getContext: (kind: string) =>
      kind === "webgl" || kind === "experimental-webgl" ? gl : null,
  } as unknown as HTMLCanvasElement;
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

  it("keeps WebGL across a StrictMode-style remount — dispose must not poison the context", () => {
    const gl = workingGl();
    const canvas = reusableGlCanvas(gl);

    // Effect pass #1: WebGL initialises fine.
    const p1 = new Presenter();
    expect(p1.init(canvas)).toBe(true);

    // StrictMode cleanup between the two passes.
    p1.dispose();
    expect(gl._lost).toBe(false); // dispose must NOT have lost the shared context

    // Effect pass #2 on the SAME canvas (same reused context) must still work —
    // a poisoned (lost) context here is what made shader compiles fail with a
    // null log and forced the Canvas2D fallback even on healthy GPUs.
    const p2 = new Presenter();
    expect(p2.init(canvas)).toBe(true);
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
