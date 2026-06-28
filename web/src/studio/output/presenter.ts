import { WIDTH, HEIGHT } from "../../ppu/core";
import type { PresentFx } from "./fx";
import { fxUniforms } from "./fx";

const VERT_SRC = `\
attribute vec2 aPos;
varying vec2 vUv;
void main() {
  vUv = aPos * 0.5 + 0.5;
  gl_Position = vec4(aPos, 0.0, 1.0);
}`;

const FRAG_SRC = `\
#extension GL_OES_standard_derivatives : enable
precision mediump float;
varying vec2 vUv;
uniform sampler2D uTex;
uniform vec2 uNative;
uniform float uCrt;
uniform float uScanline;
uniform float uGrid;

vec2 barrel(vec2 uv, float amt) {
  vec2 cc = uv - 0.5;
  float d = dot(cc, cc);
  return uv + cc * d * amt;
}

void main() {
  vec2 uv = vUv;
  if (uCrt > 0.5) uv = barrel(uv, 0.18);

  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
    gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
    return;
  }

  vec3 col = texture2D(uTex, uv).rgb;

  float scan = max(uScanline, uCrt);
  if (scan > 0.5) {
    float s = sin(uv.y * uNative.y * 3.14159265);
    col *= 1.0 - 0.35 * scan * (s * s);
  }

  if (uGrid > 0.5) {
    vec2 g = abs(fract(uv * uNative) - 0.5);
    float line = step(0.5 - fwidth(uv.x * uNative.x), max(g.x, g.y));
    col *= 1.0 - 0.5 * line;
  }

  if (uCrt > 0.5) {
    vec2 cc = uv - 0.5;
    col *= 1.0 - 0.6 * dot(cc, cc);
  }

  gl_FragColor = vec4(col, 1.0);
}`;

function compile(gl: WebGLRenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(sh) ?? "shader compile failed");
  }
  return sh;
}

/** Owns the present pipeline: framebuffer texture -> integer upscale -> FX -> canvas.
 *  Falls back to a Canvas2D blit if WebGL is unavailable. */
export class Presenter {
  private canvas!: HTMLCanvasElement;
  private gl: WebGLRenderingContext | null = null;
  private ctx2d: CanvasRenderingContext2D | null = null;
  private image: ImageData | null = null;
  private program: WebGLProgram | null = null;
  private tex: WebGLTexture | null = null;
  private u: Record<string, WebGLUniformLocation | null> = {};
  private k = 1;

  /** @returns true if WebGL succeeded, false if it fell back to Canvas2D. */
  init(canvas: HTMLCanvasElement): boolean {
    this.canvas = canvas;
    const gl = (canvas.getContext("webgl", { antialias: false, alpha: false })
      ?? canvas.getContext("experimental-webgl")) as WebGLRenderingContext | null;
    if (!gl) return this.initFallback(canvas);
    gl.getExtension("OES_standard_derivatives");
    const prog = gl.createProgram()!;
    gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VERT_SRC));
    gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FRAG_SRC));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return this.initFallback(canvas);
    gl.useProgram(prog);

    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const aPos = gl.getAttribLocation(prog, "aPos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

    this.tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);

    this.u = {
      uTex: gl.getUniformLocation(prog, "uTex"),
      uNative: gl.getUniformLocation(prog, "uNative"),
      uCrt: gl.getUniformLocation(prog, "uCrt"),
      uScanline: gl.getUniformLocation(prog, "uScanline"),
      uGrid: gl.getUniformLocation(prog, "uGrid"),
    };
    gl.uniform1i(this.u.uTex, 0);
    gl.uniform2f(this.u.uNative, WIDTH, HEIGHT);
    this.program = prog;
    this.gl = gl;
    return true;
  }

  private initFallback(canvas: HTMLCanvasElement): boolean {
    this.ctx2d = canvas.getContext("2d");
    this.image = new ImageData(WIDTH, HEIGHT);
    return false;
  }

  /** Size the drawing buffer to an exact integer multiple of native (crisp). */
  resize(k: number): void {
    this.k = Math.max(1, k);
    if (this.gl) {
      this.canvas.width = WIDTH * this.k;
      this.canvas.height = HEIGHT * this.k;
      this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    } else {
      this.canvas.width = WIDTH;
      this.canvas.height = HEIGHT;
    }
    this.canvas.style.width = `${WIDTH * this.k}px`;
    this.canvas.style.height = `${HEIGHT * this.k}px`;
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
    gl.bindTexture(gl.TEXTURE_2D, this.tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, WIDTH, HEIGHT, 0, gl.RGBA,
      gl.UNSIGNED_BYTE, new Uint8Array(framebuffer.buffer, framebuffer.byteOffset, framebuffer.length));
    const un = fxUniforms(fx);
    gl.uniform1f(this.u.uCrt, un.uCrt);
    gl.uniform1f(this.u.uScanline, un.uScanline);
    gl.uniform1f(this.u.uGrid, un.uGrid);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  dispose(): void {
    const gl = this.gl;
    if (!gl) return;
    if (this.program) gl.deleteProgram(this.program);
    if (this.tex) gl.deleteTexture(this.tex);
    const ext = gl.getExtension("WEBGL_lose_context");
    ext?.loseContext();
    this.gl = null;
  }
}
