/** Presentational 3D stage: CRT TV + console, screen fed by a framebuffer
 *  callback — no transport/core coupling, so it can be storied and
 *  screenshotted with a synthetic frame (the PlayerFrame pattern in 3D).
 *
 *  The screen material runs the shared "living phosphor" CRT (crt.glsl.ts):
 *  the same feedback decay pass as the Presenter, here as a ping-pong between
 *  two native-res render targets, then crtImage() on the curved glass. The
 *  mesh is already curved in 3D, so the in-shader warp is only a touch. */
import { useEffect, useRef } from "react";
import * as THREE from "three";
import { WIDTH, HEIGHT } from "../../ppu/core";
import { CRT_LIB, DECAY_FRAG_BODY, CRT_DECAY } from "../../studio/output/crt.glsl";
import { buildTv, buildConsole, buildBlobShadow } from "./heroModels";

const SCREEN_VERT = `\
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;

const SCREEN_FRAG = `\
${CRT_LIB}
varying vec2 vUv;
uniform sampler2D uTex;
uniform vec2 uNative;
void main() {
  gl_FragColor = vec4(crtImage(uTex, vUv, uNative, 0.05), 1.0);
}`;

const PASSTHROUGH_VERT = `\
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0);
}`;

/** Average framebuffer color (sparse stride), for the screen-glow light. */
function avgColor(fb: Uint8ClampedArray, out: THREE.Color): number {
  let r = 0,
    g = 0,
    b = 0,
    n = 0;
  for (let i = 0; i < fb.length; i += 4 * 997) {
    r += fb[i];
    g += fb[i + 1];
    b += fb[i + 2];
    n++;
  }
  out.setRGB(r / n / 255, g / n / 255, b / n / 255);
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / n / 255;
  return lum;
}

export function HeroStage({
  getFrame,
  onFail,
}: {
  /** Latest native framebuffer (RGBA, 256x224) or null while loading. */
  getFrame: () => Uint8ClampedArray | null;
  /** Called once if WebGL is unavailable — parent swaps in a 2D fallback. */
  onFail: () => void;
}) {
  const mountRef = useRef<HTMLDivElement>(null);
  const getFrameRef = useRef(getFrame);
  getFrameRef.current = getFrame;
  const onFailRef = useRef(onFail);
  onFailRef.current = onFail;

  useEffect(() => {
    const container = mountRef.current;
    if (!container) return;

    // The renderer creates its own canvas: a StrictMode remount then gets a
    // FRESH context instead of re-initialising three on a canvas whose GL
    // state the disposed renderer already mutated (which renders the scene
    // but leaves the render-target feedback chain black).
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    } catch {
      onFailRef.current();
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.domElement.className = "hero3d-canvas";
    container.appendChild(renderer.domElement);

    // --- scene -------------------------------------------------------------
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 40);
    const camBase = new THREE.Vector3(0.7, 2.1, 10.6);
    camera.position.copy(camBase);

    const screenMat = new THREE.ShaderMaterial({
      vertexShader: SCREEN_VERT,
      fragmentShader: SCREEN_FRAG,
      uniforms: {
        // Starts on the (black) feedback target: a warming-up TV until the
        // first real frame lands, and never a null sampler.
        uTex: { value: null as THREE.Texture | null },
        uNative: { value: new THREE.Vector2(WIDTH, HEIGHT) },
      },
    });
    const tv = buildTv(screenMat);
    tv.group.position.set(-0.95, 0, 0);
    tv.group.rotation.y = 0.12;
    scene.add(tv.group);

    const console3d = buildConsole();
    console3d.position.set(1.75, 0, 1.1);
    console3d.rotation.y = -0.35;
    scene.add(console3d);

    scene.add(buildBlobShadow(7, 3.4));

    scene.add(new THREE.AmbientLight(0xffffff, 0.55));
    const key = new THREE.DirectionalLight(0xfff0dd, 1.6);
    key.position.set(3, 5, 4);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0x5fc9e8, 0.5);
    rim.position.set(-4, 2, -3);
    scene.add(rim);
    // Tinted per-frame from the framebuffer: the toy's palette spills onto
    // the console and floor.
    const glow = new THREE.PointLight(0xffffff, 0, 6, 2);
    glow.position.set(-0.6, 1.6, 2.0);
    scene.add(glow);

    // --- phosphor feedback (shared decay shader, three-flavoured) ----------
    const frameData = new Uint8Array(WIDTH * HEIGHT * 4);
    const frameTex = new THREE.DataTexture(frameData, WIDTH, HEIGHT);
    frameTex.minFilter = frameTex.magFilter = THREE.LinearFilter;
    const rtOpts = {
      depthBuffer: false,
      minFilter: THREE.LinearFilter,
      magFilter: THREE.LinearFilter,
    };
    const rts = [
      new THREE.WebGLRenderTarget(WIDTH, HEIGHT, rtOpts),
      new THREE.WebGLRenderTarget(WIDTH, HEIGHT, rtOpts),
    ];
    let ping = 0;
    const decayMat = new THREE.ShaderMaterial({
      vertexShader: PASSTHROUGH_VERT,
      fragmentShader: DECAY_FRAG_BODY,
      uniforms: {
        uCur: { value: frameTex },
        uPrev: { value: rts[0].texture },
        uDecay: { value: new THREE.Vector3(...CRT_DECAY) },
      },
    });
    const decayScene = new THREE.Scene();
    decayScene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), decayMat));
    const decayCam = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
    screenMat.uniforms.uTex.value = rts[0].texture;

    // --- sizing + parallax -------------------------------------------------
    const resize = () => {
      const w = container.clientWidth;
      const h = container.clientHeight;
      if (!w || !h) return;
      renderer.setSize(w, h, false);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);

    const pointer = new THREE.Vector2(0, 0);
    const onPointer = (e: PointerEvent) => {
      const r = container.getBoundingClientRect();
      pointer.set(
        ((e.clientX - r.left) / r.width) * 2 - 1,
        ((e.clientY - r.top) / r.height) * 2 - 1,
      );
    };
    const onLeave = () => pointer.set(0, 0);
    container.addEventListener("pointermove", onPointer);
    container.addEventListener("pointerleave", onLeave);

    // --- frame loop --------------------------------------------------------
    const glowCol = new THREE.Color();
    let raf = 0;
    const t0 = performance.now();
    const tick = () => {
      raf = requestAnimationFrame(tick);
      const t = (performance.now() - t0) / 1000;

      const fb = getFrameRef.current();
      if (fb) {
        frameData.set(fb);
        frameTex.needsUpdate = true;
        const write = 1 - ping;
        decayMat.uniforms.uPrev.value = rts[ping].texture;
        renderer.setRenderTarget(rts[write]);
        renderer.render(decayScene, decayCam);
        renderer.setRenderTarget(null);
        screenMat.uniforms.uTex.value = rts[write].texture;
        ping = write;
        glow.intensity = 1.2 + 2.4 * avgColor(fb, glowCol);
        glow.color.copy(glowCol);
      }

      camera.position.set(
        camBase.x + Math.sin(t * 0.4) * 0.08 + pointer.x * 0.35,
        camBase.y + Math.cos(t * 0.3) * 0.05 - pointer.y * 0.25,
        camBase.z,
      );
      camera.lookAt(0.1, 1.25, 0);
      renderer.render(scene, camera);
    };
    tick();

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      container.removeEventListener("pointermove", onPointer);
      container.removeEventListener("pointerleave", onLeave);
      rts[0].dispose();
      rts[1].dispose();
      frameTex.dispose();
      scene.traverse((o) => {
        if (o instanceof THREE.Mesh) {
          o.geometry.dispose();
          const mats = Array.isArray(o.material) ? o.material : [o.material];
          for (const m of mats) m.dispose();
        }
      });
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, []);

  return <div ref={mountRef} className="hero3d-mount" />;
}
