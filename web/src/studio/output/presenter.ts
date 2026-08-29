import { WIDTH, HEIGHT } from "../../ppu/core";
import type { PresentFx } from "./fx";
import { fxUniforms } from "./fx";
import { CRT_LIB, DECAY_FRAG_BODY, CRT_DECAY } from "./crt.glsl";

const VERT_SRC = `\
attribute vec2 aPos;
varying vec2 vUv;
void main() {
  vUv = aPos * 0.5 + 0.5;
  gl_Position = vec4(aPos, 0.0, 1.0);
}`;

// CRT path is the shared "living phosphor" look (crt.glsl.ts); uTex is then the
// phosphor feedback texture, not the raw framebuffer. Scanline/grid are debug
// overlays on the plain path only.
const FRAG_SRC = `\
precision mediump float;
${CRT_LIB}
varying vec2 vUv;
uniform sampler2D uTex;
uniform vec2 uNative;
uniform float uCrt;
uniform float uScanline;
uniform float uGrid;
uniform float uGridW;   // grid line half-width in uv*uNative units = 1.0 / scale

void main() {
  if (uCrt > 0.5) {
    gl_FragColor = vec4(crtImage(uTex, vUv, uNative, 0.16), 1.0);
    return;
  }

  vec2 uv = vUv;
  // Flip V: framebuffer row 0 is the top of the image; GL texture v=0 is the
  // bottom of the screen quad. (Done in-shader rather than via
  // UNPACK_FLIP_Y_WEBGL, which is unreliable for ArrayBufferView uploads.)
  vec3 col = texture2D(uTex, vec2(uv.x, 1.0 - uv.y)).rgb;

  if (uScanline > 0.5) {
    float s = sin(uv.y * uNative.y * 3.14159265);
    col *= 1.0 - 0.35 * (s * s);
  }

  if (uGrid > 0.5) {
    vec2 g = abs(fract(uv * uNative) - 0.5);
    float line = step(0.5 - uGridW, max(g.x, g.y));
    col *= 1.0 - 0.5 * line;
  }

  gl_FragColor = vec4(col, 1.0);
}`;

const DECAY_SRC = `\
precision mediump float;
${DECAY_FRAG_BODY}`;

function compile(gl: WebGLRenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(sh) ?? "shader compile failed");
  }
  return sh;
}

/** Compile+link a program with aPos pinned to attrib 0, so the one shared
 *  vertex buffer/attrib setup serves every program. Throws on any failure. */
function buildProgram(gl: WebGLRenderingContext, frag: string): WebGLProgram {
  const prog = gl.createProgram();
  if (!prog) throw new Error("WebGL createProgram returned null");
  const vs = compile(gl, gl.VERTEX_SHADER, VERT_SRC);
  const fs = compile(gl, gl.FRAGMENT_SHADER, frag);
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.bindAttribLocation(prog, 0, "aPos");
  gl.linkProgram(prog);
  // Shaders are owned by the program once linked; free our handles.
  gl.deleteShader(vs);
  gl.deleteShader(fs);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    const log = gl.getProgramInfoLog(prog) ?? "program link failed";
    gl.deleteProgram(prog);
    throw new Error(log);
  }
  return prog;
}

let warnedWebgl = false;
/** A WebGL failure is recoverable: we degrade to the Canvas2D blit and the
 *  framebuffer still shows — only the present-pass effects are lost. Warn once so
 *  it's diagnosable without spamming a frame loop. The shaders are valid GLSL ES;
 *  this fires when the browser's WebGL is disabled or the GPU is blocklisted. */
function warnWebglOnce(err: unknown): void {
  if (warnedWebgl) return;
  warnedWebgl = true;
  console.warn(
    "[ppu.toys] WebGL present pass unavailable — falling back to a plain Canvas2D " +
      "blit (CRT/scanline/pixel-grid effects disabled). The emulator output is " +
      "unaffected. This usually means the browser's WebGL is disabled or the GPU " +
      "is blocklisted; check chrome://gpu and enable hardware acceleration.",
    err,
  );
}

/** Owns the present pipeline: framebuffer texture -> integer upscale -> FX -> canvas.
 *  Falls back to a Canvas2D blit if WebGL is unavailable. */
export class Presenter {
  private canvas!: HTMLCanvasElement;
  private gl: WebGLRenderingContext | null = null;
  private ctx2d: CanvasRenderingContext2D | null = null;
  private image: ImageData | null = null;
  private program: WebGLProgram | null = null;
  private buf: WebGLBuffer | null = null;
  private tex: WebGLTexture | null = null;
  private u: Record<string, WebGLUniformLocation | null> = {};
  private k = 1;
  // Phosphor persistence (CRT feedback): ping-pong native-res targets. All null
  // when the context can't do FBOs — the CRT then renders without trails.
  private decayProg: WebGLProgram | null = null;
  private phosTex: WebGLTexture[] = [];
  private phosFbo: WebGLFramebuffer[] = [];
  private ping = 0;

  /** @returns true if WebGL succeeded, false if it fell back to Canvas2D.
   *  Any GL failure — context unavailable, shader compile, or program link —
   *  degrades to the Canvas2D blit rather than throwing, so a degraded browser
   *  WebGL stack can't take down the React tree (the bug that blanked the app). */
  init(canvas: HTMLCanvasElement, forceCanvas2d = false): boolean {
    this.canvas = canvas;
    if (!forceCanvas2d) {
      const opts: WebGLContextAttributes = { antialias: false, alpha: false };
      const gl = (canvas.getContext("webgl", opts) ??
        canvas.getContext("experimental-webgl", opts)) as WebGLRenderingContext | null;
      if (gl) {
        try {
          this.setupGl(gl);
          this.gl = gl;
          return true;
        } catch (err) {
          warnWebglOnce(err);
          try {
            gl.getExtension("WEBGL_lose_context")?.loseContext();
          } catch {
            /* ignore */
          }
          this.gl = null;
          this.program = null;
          this.buf = null;
          this.tex = null;
          // Obtaining a 'webgl' context locks this canvas to webgl for life, so
          // getContext('2d') on it now returns null and the Canvas2D fallback
          // can't draw. Signal failure: the caller must remount a FRESH canvas
          // and re-init with forceCanvas2d=true (see OutputCanvas).
          return false;
        }
      }
      // No webgl context at all — the canvas is untainted, so the Canvas2D
      // fallback below works directly on it.
    }
    return this.initFallback(canvas);
  }

  /** Build the present pipeline. Throws on any GL failure; init() catches it and
   *  falls back. Does NOT assign this.gl — init() does that only on full success,
   *  so render() never observes a half-built context. */
  private setupGl(gl: WebGLRenderingContext): void {
    const prog = buildProgram(gl, FRAG_SRC);
    gl.useProgram(prog);

    this.buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);

    this.tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    this.u = {
      uTex: gl.getUniformLocation(prog, "uTex"),
      uNative: gl.getUniformLocation(prog, "uNative"),
      uCrt: gl.getUniformLocation(prog, "uCrt"),
      uScanline: gl.getUniformLocation(prog, "uScanline"),
      uGrid: gl.getUniformLocation(prog, "uGrid"),
      uGridW: gl.getUniformLocation(prog, "uGridW"),
    };
    gl.uniform1i(this.u.uTex, 0);
    gl.uniform2f(this.u.uNative, WIDTH, HEIGHT);
    gl.uniform1f(this.u.uGridW, 1 / this.k);
    this.program = prog;

    // Phosphor persistence is best-effort: FBO support missing or broken only
    // costs the trails, never the frame — so its failure must not reach init()'s
    // catch (which would needlessly demote a working context to Canvas2D).
    try {
      this.setupPhosphor(gl);
    } catch {
      this.dropPhosphor(gl);
    }
    gl.useProgram(this.program);
  }

  /** Decay program + two native-res ping-pong render targets. Throws on any
   *  failure; setupGl catches and degrades to persistence-free CRT. */
  private setupPhosphor(gl: WebGLRenderingContext): void {
    const prog = buildProgram(gl, DECAY_SRC);
    gl.useProgram(prog);
    gl.uniform1i(gl.getUniformLocation(prog, "uCur"), 0);
    gl.uniform1i(gl.getUniformLocation(prog, "uPrev"), 1);
    gl.uniform3f(gl.getUniformLocation(prog, "uDecay"), ...CRT_DECAY);
    this.decayProg = prog;
    for (let i = 0; i < 2; i++) {
      const tex = gl.createTexture();
      if (!tex) throw new Error("createTexture returned null");
      this.phosTex.push(tex);
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, WIDTH, HEIGHT, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      // LINEAR: the CRT pass wants a soft base image — beam and grille supply
      // the structure. (The raw framebuffer texture stays NEAREST for the
      // crisp non-CRT path.)
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      const fbo = gl.createFramebuffer();
      if (!fbo) throw new Error("createFramebuffer returned null");
      this.phosFbo.push(fbo);
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
      if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
        throw new Error("phosphor framebuffer incomplete");
      }
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  private dropPhosphor(gl: WebGLRenderingContext): void {
    try {
      gl.bindFramebuffer?.(gl.FRAMEBUFFER, null);
      if (this.decayProg) gl.deleteProgram(this.decayProg);
      for (const t of this.phosTex) gl.deleteTexture(t);
      for (const f of this.phosFbo) gl.deleteFramebuffer(f);
    } catch {
      /* a context broken enough to fail cleanup has nothing worth freeing */
    }
    this.decayProg = null;
    this.phosTex = [];
    this.phosFbo = [];
  }

  private initFallback(canvas: HTMLCanvasElement): boolean {
    this.ctx2d = canvas.getContext("2d");
    this.image = new ImageData(WIDTH, HEIGHT);
    return false;
  }

  /** Size the drawing buffer to an exact integer multiple of native (crisp).
   *  On-screen size is the stylesheet's concern (.display-canvas fills its box
   *  with image-rendering: pixelated, per the Workspace handoff) — the presenter
   *  never sets inline styles, it only picks the buffer resolution. */
  resize(k: number): void {
    this.k = Math.max(1, Math.round(k));
    if (this.gl) {
      this.canvas.width = WIDTH * this.k;
      this.canvas.height = HEIGHT * this.k;
      this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
      this.gl.uniform1f(this.u.uGridW, 1 / this.k);
    } else {
      this.canvas.width = WIDTH;
      this.canvas.height = HEIGHT;
    }
  }

  render(framebuffer: Uint8ClampedArray, fx: PresentFx): void {
    const gl = this.gl;
    if (!gl) {
      if (this.ctx2d && this.image) {
        this.image.data.set(framebuffer);
        this.ctx2d.putImageData(this.image, 0, 0);
      }
      return;
    }
    try {
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.tex);
      gl.texImage2D(
        gl.TEXTURE_2D,
        0,
        gl.RGBA,
        WIDTH,
        HEIGHT,
        0,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        new Uint8Array(framebuffer.buffer, framebuffer.byteOffset, framebuffer.length),
      );
      const un = fxUniforms(fx);
      if (un.uCrt > 0 && this.decayProg) {
        // Pass 1 — phosphor feedback at native res: max(cur, prev * decay)
        // into the write target, then present from it.
        const write = 1 - this.ping;
        gl.useProgram(this.decayProg);
        gl.activeTexture(gl.TEXTURE1);
        gl.bindTexture(gl.TEXTURE_2D, this.phosTex[this.ping]);
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.phosFbo[write]);
        gl.viewport(0, 0, WIDTH, HEIGHT);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.viewport(0, 0, this.canvas.width, this.canvas.height);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, this.phosTex[write]);
        this.ping = write;
      }
      gl.useProgram(this.program);
      gl.uniform1f(this.u.uCrt, un.uCrt);
      gl.uniform1f(this.u.uScanline, un.uScanline);
      gl.uniform1f(this.u.uGrid, un.uGrid);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    } catch (err) {
      // Context lost mid-run — stop driving GL so we don't re-throw every frame.
      warnWebglOnce(err);
      this.gl = null;
    }
  }

  dispose(): void {
    const gl = this.gl;
    if (!gl) return;
    if (this.program) gl.deleteProgram(this.program);
    if (this.buf) gl.deleteBuffer(this.buf);
    if (this.tex) gl.deleteTexture(this.tex);
    this.dropPhosphor(gl);
    // Deliberately do NOT call WEBGL_lose_context.loseContext() here. getContext()
    // returns the SAME context for a given canvas, and React reuses the canvas DOM
    // node across remounts (StrictMode double-invokes effects in dev), so losing
    // the context poisons it: the next init() gets back a dead context whose shader
    // compiles fail with a null log. Just free our resources; the context is
    // released when the canvas element is removed.
    this.gl = null;
  }
}
