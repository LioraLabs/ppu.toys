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
import {
  buildTv,
  buildConsole,
  buildCart,
  buildBlobShadow,
  makeCartLabel,
  type CartIdentity,
  type CartBuild,
} from "./heroModels";
// This component creates the .hero3d-mount/.hero3d-canvas elements, so it owns
// the CSS import: without it a standalone mount (e.g. the cosmos fixture) has
// an unstyled canvas whose attribute size (dpr > 1) inflates its container,
// re-triggering the ResizeObserver in a growth loop.
import type { padKeyHandlers } from "../../studio/transport/pad";
import "./hero.css";

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

export interface HeroLayout {
  fov: number;
  cam: [number, number, number];
  lookAt: [number, number, number];
  tvPos: [number, number, number];
  tvRotY: number;
  consolePos: [number, number, number];
  consoleRotY: number;
  /** Mobile hides the console: the TV alone, front and center. */
  console: boolean;
}

/** Composition presets, picked by container aspect. Tune via the cosmos
 *  fixture's "tune" toggle (lil-gui) and paste the dumped values here. */
export const LAYOUTS: { desktop: HeroLayout; mobile: HeroLayout } = {
  desktop: {
    fov: 27,
    cam: [0.25, 2.1, 10.35],
    lookAt: [0, 1.25, -0.3],
    tvPos: [-0.8, 0, 0],
    tvRotY: 0.09,
    consolePos: [0.95, 0, 1.95],
    consoleRotY: -0.35,
    console: true,
  },
  mobile: {
    fov: 30,
    cam: [0, 2.0, 13.5],
    lookAt: [0, 1.75, 0],
    tvPos: [0, 0, 0],
    tvRotY: 0,
    consolePos: [1.0, 0, 1.35],
    consoleRotY: -0.45,
    console: false,
  },
};

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
  cart = null,
  tune = false,
  pad,
}: {
  /** Latest native framebuffer (RGBA, 256x224) or null while loading. */
  getFrame: () => Uint8ClampedArray | null;
  /** Called once if WebGL is unavailable — parent swaps in a 2D fallback. */
  onFail: () => void;
  /** Toy identity for the cartridge label (clip thumb + author avatar);
   *  null leaves a blank cart. */
  cart?: CartIdentity | null;
  /** Dev-only: lil-gui panel over the live scene + a dump button that logs
   *  values in LAYOUTS shape. Set from the cosmos fixture, never in the app. */
  tune?: boolean;
  /** Controller key handlers (pad.ts): the TV becomes focusable and playable. */
  pad?: ReturnType<typeof padKeyHandlers>;
}) {
  const mountRef = useRef<HTMLDivElement>(null);
  const getFrameRef = useRef(getFrame);
  getFrameRef.current = getFrame;
  const onFailRef = useRef(onFail);
  onFailRef.current = onFail;
  const cartLabelRef = useRef<CartBuild["label"] | null>(null);

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
    const camera = new THREE.PerspectiveCamera(LAYOUTS.desktop.fov, 1, 0.1, 40);
    const camBase = new THREE.Vector3();
    const lookAt = new THREE.Vector3();

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
    scene.add(tv.group);

    const console3d = buildConsole();
    scene.add(console3d);
    const cart3d = buildCart();
    cartLabelRef.current = cart3d.label;

    const applyLayout = (l: HeroLayout) => {
      camera.fov = l.fov;
      camBase.set(...l.cam);
      lookAt.set(...l.lookAt);
      tv.group.position.set(...l.tvPos);
      tv.group.rotation.y = l.tvRotY;
      console3d.position.set(...l.consolePos);
      console3d.rotation.y = l.consoleRotY;
      console3d.visible = l.console;
      // The cart rides in the console slot, or — console hidden — leans
      // against the TV's face. add() reparents, removing from the old parent.
      if (l.console) {
        console3d.add(cart3d.group);
        cart3d.group.position.set(0, 0.49, -0.58);
        cart3d.group.rotation.set(0, 0, 0);
        cart3d.group.scale.setScalar(1);
      } else {
        tv.group.add(cart3d.group);
        cart3d.group.position.set(0.78, 0.02, 1.62);
        cart3d.group.rotation.set(-0.34, -0.08, 0);
        cart3d.group.scale.setScalar(1.35);
      }
    };
    applyLayout(LAYOUTS.desktop);

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
      // Portrait-ish containers get the mobile composition. While tuning,
      // leave whatever the gui has set alone.
      if (!tune) applyLayout(w / h < 0.9 ? LAYOUTS.mobile : LAYOUTS.desktop);
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
      camera.lookAt(lookAt);
      renderer.render(scene, camera);
    };
    tick();

    // --- dev tuning panel ----------------------------------------------------
    let gui: { destroy(): void } | undefined;
    if (tune) {
      import("three/examples/jsm/libs/lil-gui.module.min.js").then(({ GUI }) => {
        const g = new GUI({ title: "hero layout" });
        gui = g;
        const onFov = () => camera.updateProjectionMatrix();
        const vec = (folder: string, v: THREE.Vector3, range = 6) => {
          const f = g.addFolder(folder);
          f.add(v, "x", -range, range, 0.05);
          f.add(v, "y", -range, range, 0.05);
          f.add(v, "z", -range, 3 * range, 0.05);
          return f;
        };
        g.add(camera, "fov", 10, 60, 1).onChange(onFov);
        vec("camera", camBase);
        vec("lookAt", lookAt, 3);
        vec("tv", tv.group.position, 4).add(tv.group.rotation, "y", -1, 1, 0.01).name("rotY");
        vec("console", console3d.position, 4)
          .add(console3d.rotation, "y", -1, 1, 0.01)
          .name("rotY");
        g.add(
          {
            dump: () =>
              console.log(
                JSON.stringify(
                  {
                    fov: camera.fov,
                    cam: camBase.toArray(),
                    lookAt: lookAt.toArray(),
                    tvPos: tv.group.position.toArray(),
                    tvRotY: tv.group.rotation.y,
                    consolePos: console3d.position.toArray(),
                    consoleRotY: console3d.rotation.y,
                    console: console3d.visible,
                  } satisfies HeroLayout,
                  null,
                  2,
                ),
              ),
          },
          "dump",
        );
      });
    }

    return () => {
      gui?.destroy();
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
  }, [tune]);

  // The cart identity lands after mount (featured toy is fetched); texture the
  // label whenever it changes. `tune` is a dep because toggling it rebuilds
  // the scene — and with it a fresh, unlabeled cart.
  useEffect(() => {
    const label = cartLabelRef.current;
    if (!cart || !label) return;
    const tex = makeCartLabel(cart);
    label.material.map = tex;
    label.material.color.set(0xffffff);
    label.material.needsUpdate = true;
    return () => {
      label.material.map = null;
      tex.dispose();
    };
  }, [cart?.thumbUrl, cart?.avatarUrl, cart?.handle, tune]);

  return <div ref={mountRef} className="hero3d-mount" {...pad} />;
}
