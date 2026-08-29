/** The ppu.toys "living phosphor" CRT — shared GLSL ES 1.0 chunks.
 *
 *  Consumed by two renderers: the Presenter's WebGL1 present pass (Studio,
 *  Permalink player) and the landing hero's three.js screen material. Chunks
 *  carry NO `precision` header — the presenter prepends its own and three.js
 *  injects one — and no `main`, except DECAY_FRAG_BODY which is a full
 *  fragment body for the feedback pass.
 *
 *  The signature look, in signal order:
 *    1. phosphor persistence — a native-res feedback pass keeps
 *       `max(cur, prev * decay)` per channel; green lingers, blue dies fast,
 *       so motion leaves brief green-tinted comet trails (P22-ish).
 *    2. gaussian beam whose width swells with luminance — bright pixels bloom
 *       into fat scanlines, dark rows thin to threads.
 *    3. aperture grille phosphor triads per physical pixel column.
 *    4. chroma fringing, halation, barrel warp, rounded corners, vignette,
 *       a faint static glass glint.
 *
 *  Techniques informed by Kyle Pittman's Super Win the Game CRT writeup and
 *  shadertoy MdyyWt ("1990-esque"); all code original.
 */

/** Per-channel phosphor decay per frame (uDecay). Green outlives red outlives
 *  blue. ponytail: frame-based, tuned for 60fps — scale by dt if the transport
 *  ever runs at other rates. */
export const CRT_DECAY: [number, number, number] = [0.42, 0.72, 0.36];

/** Feedback pass: previous phosphor state fades per channel, new frame
 *  re-excites. Runs at native res (256x224) into a ping-pong FBO. */
export const DECAY_FRAG_BODY = `\
varying vec2 vUv;
uniform sampler2D uCur;
uniform sampler2D uPrev;
uniform vec3 uDecay;
void main() {
  vec3 cur = texture2D(uCur, vUv).rgb;
  vec3 ghost = texture2D(uPrev, vUv).rgb * uDecay;
  gl_FragColor = vec4(max(cur, ghost), 1.0);
}`;

/** Pure functions, no main. Callers build an image with
 *  `crtImage(tex, uv, native, warp)` — `warp` differs per consumer: the flat
 *  presenter quad wants full barrel (0.16), the hero's screen mesh is already
 *  curved in 3D so it only needs a touch (0.05).
 *  ponytail: the consts below are the calibration knobs — retune by eye with
 *  the HeroStage cosmos fixture + `pnpm run shoot`. */
export const CRT_LIB = `\
const float CRT_CORNER    = 0.06;  // corner radius, uv units
const float CRT_BEAM_MIN  = 0.30;  // beam sigma at black, in scanlines
const float CRT_BEAM_MAX  = 0.68;  // beam sigma at white
const float CRT_MASK_DIM  = 0.62;  // off-phosphor leakage in the grille
const float CRT_MASK_GAIN = 1.18;  // grille brightness compensation
const float CRT_FRINGE    = 0.60;  // chroma offset, native px
const float CRT_HALO_R    = 2.5;   // halation tap radius, native px
const float CRT_HALO      = 0.10;  // halation strength
const float CRT_GLINT     = 0.04;  // glass glint strength

vec2 crtWarp(vec2 uv, float amt) {
  vec2 cc = uv - 0.5;
  float d = dot(cc, cc);
  return uv + cc * d * (amt + 2.0 * amt * d);
}

float crtCornerMask(vec2 uv) {
  vec2 q = abs(uv - 0.5) - (vec2(0.5) - CRT_CORNER);
  float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - CRT_CORNER;
  return 1.0 - smoothstep(-0.004, 0.001, d);
}

// Framebuffer row 0 is the top of the image; quad/mesh uv v=0 is the bottom.
// Flip here, once, and offset R/B for NTSC-ish chroma fringing.
vec3 crtFetch(sampler2D tex, vec2 uv, vec2 native) {
  vec2 suv = vec2(uv.x, 1.0 - uv.y);
  vec2 e = vec2(CRT_FRINGE / native.x, 0.0);
  return vec3(
    texture2D(tex, suv - e).r,
    texture2D(tex, suv).g,
    texture2D(tex, suv + e).b);
}

// Gaussian beam: width follows luminance, so brightness modulates scanline
// thickness instead of a fixed sin^2 darkening.
vec3 crtBeam(vec3 col, vec2 uv, vec2 native) {
  float f = fract(uv.y * native.y) - 0.5;
  float lum = dot(col, vec3(0.299, 0.587, 0.114));
  float sigma = mix(CRT_BEAM_MIN, CRT_BEAM_MAX, lum);
  float w = exp(-0.5 * f * f / (sigma * sigma));
  return col * mix(0.55, 1.12, w);
}

// Aperture grille: R/G/B phosphor columns cycling per physical pixel.
vec3 crtGrille(float px) {
  float m = mod(px, 3.0);
  vec3 mask = vec3(CRT_MASK_DIM);
  if (m < 1.0) mask.r = 1.0;
  else if (m < 2.0) mask.g = 1.0;
  else mask.b = 1.0;
  return mask * CRT_MASK_GAIN;
}

vec3 crtHalo(sampler2D tex, vec2 uv, vec2 native) {
  vec2 suv = vec2(uv.x, 1.0 - uv.y);
  vec2 e = vec2(CRT_HALO_R) / native;
  return 0.25 * (
    texture2D(tex, suv + e).rgb +
    texture2D(tex, suv - e).rgb +
    texture2D(tex, suv + vec2(e.x, -e.y)).rgb +
    texture2D(tex, suv - vec2(e.x, -e.y)).rgb);
}

vec3 crtImage(sampler2D tex, vec2 uv0, vec2 native, float warp) {
  vec2 uv = crtWarp(uv0, warp);
  float mask = crtCornerMask(uv);
  if (mask <= 0.0) return vec3(0.0);
  vec2 cl = clamp(uv, 0.0, 1.0);
  vec3 col = crtFetch(tex, cl, native);
  col = crtBeam(col, cl, native);
  col *= crtGrille(gl_FragCoord.x);
  col += CRT_HALO * crtHalo(tex, cl, native);
  vec2 cc = uv - 0.5;
  col *= 1.0 - 0.45 * dot(cc, cc);
  col += CRT_GLINT * pow(max(0.0, 1.0 - length(uv - vec2(0.32, 0.24)) * 1.7), 2.0);
  return col * mask;
}`;
