/** Procedural low-poly models for the landing hero: a 90s CRT TV and an
 *  SNES-flavored 16-bit console + pad. Built from three.js primitives — no
 *  external assets, no logos or exact trade dress. Dimensions are eyeballed
 *  world units; the stage camera and lights are tuned to them. */
import * as THREE from "three";
import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";
import { hueOf } from "../../components/Avatar";

const std = (color: number, roughness = 0.85, metalness = 0.05) =>
  new THREE.MeshStandardMaterial({ color, roughness, metalness });

function box(
  parent: THREE.Group,
  geo: THREE.BufferGeometry,
  mat: THREE.Material,
  x: number,
  y: number,
  z: number,
): THREE.Mesh {
  const m = new THREE.Mesh(geo, mat);
  m.position.set(x, y, z);
  parent.add(m);
  return m;
}

/** Screen glass: 8:7 plane bulged toward the viewer, z = bulge·(1−r²). */
function curvedScreen(w: number, h: number, bulge: number): THREE.BufferGeometry {
  const geo = new THREE.PlaneGeometry(w, h, 32, 28);
  const pos = geo.attributes.position;
  for (let i = 0; i < pos.count; i++) {
    const nx = pos.getX(i) / (w / 2);
    const ny = pos.getY(i) / (h / 2);
    pos.setZ(i, bulge * (1 - Math.min(1, (nx * nx + ny * ny) * 0.5)));
  }
  return geo;
}

/** Stadium (fully-rounded-end rectangle) outline, for extruded shapes like
 *  the controller body. Built in XY; callers rotate flat as needed. */
function stadium(w: number, h: number, r: number): THREE.Shape {
  const s = new THREE.Shape();
  const hw = w / 2 - r;
  const hh = h / 2 - r;
  s.absarc(-hw, hh, r, Math.PI / 2, Math.PI, false);
  s.absarc(-hw, -hh, r, Math.PI, Math.PI * 1.5, false);
  s.absarc(hw, -hh, r, Math.PI * 1.5, 0, false);
  s.absarc(hw, hh, r, 0, Math.PI / 2, false);
  return s;
}

export interface TvBuild {
  group: THREE.Group;
  screen: THREE.Mesh;
}

/** 90s charcoal CRT TV, screen-dominant: thin bezel around a big centered
 *  curved screen, a low control strip under it (LED, two buttons, speaker
 *  slits), feet, rabbit ears. The toy on screen is the star; the set frames. */
export function buildTv(screenMat: THREE.Material): TvBuild {
  const g = new THREE.Group();
  const shell = std(0x3a3e48, 0.85);
  const face = std(0x181b21, 0.7);
  const trim = std(0x43474f, 0.6);

  // Shell + slightly narrower tapering back half for the CRT-tube silhouette.
  box(g, new RoundedBoxGeometry(3.6, 3.2, 1.6, 4, 0.14), shell, 0, 1.74, 0.45);
  box(g, new RoundedBoxGeometry(3.0, 2.7, 1.6, 4, 0.14), shell, 0, 1.8, -0.5);

  box(g, new RoundedBoxGeometry(3.4, 3.0, 0.2, 3, 0.08), face, 0, 1.74, 1.22);
  const screen = box(g, curvedScreen(2.95, 2.58, 0.14), screenMat, 0, 1.86, 1.33);

  // Control strip under the screen: power LED, two buttons, speaker slits.
  box(
    g,
    new THREE.BoxGeometry(0.07, 0.07, 0.03),
    new THREE.MeshStandardMaterial({ color: 0xff5d6a, emissive: 0xff2233, emissiveIntensity: 1.4 }),
    -1.45,
    0.42,
    1.34,
  );
  const btn = new THREE.CylinderGeometry(0.05, 0.05, 0.05, 16);
  btn.rotateX(Math.PI / 2);
  box(g, btn, trim, -1.2, 0.42, 1.34);
  box(g, btn, trim, -1.02, 0.42, 1.34);
  const slit = new THREE.BoxGeometry(0.035, 0.16, 0.03);
  for (let i = 0; i < 9; i++) box(g, slit, std(0x0e1116, 0.95), 0.85 + i * 0.08, 0.42, 1.34);

  const footGeo = new THREE.BoxGeometry(0.5, 0.14, 0.5);
  box(g, footGeo, std(0x14110e, 0.95), -1.25, 0.07, 0.55);
  box(g, footGeo, std(0x14110e, 0.95), 1.25, 0.07, 0.55);

  // Rabbit ears.
  box(g, new THREE.SphereGeometry(0.1, 12, 10), trim, 0.2, 3.36, -0.5);
  const earGeo = new THREE.CylinderGeometry(0.012, 0.02, 1.5, 6);
  const earL = box(g, earGeo, trim, -0.14, 3.98, -0.5);
  earL.rotation.z = 0.5;
  const earR = box(g, earGeo, trim, 0.54, 3.98, -0.5);
  earR.rotation.z = -0.5;

  return { group: g, screen };
}

export interface CartBuild {
  group: THREE.Group;
  /** Cart face: assign a makeCartLabel() texture to its material's map (and
   *  set the color white) to brand the cartridge. Untextured it's a dark
   *  sticker. */
  label: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshStandardMaterial>;
}

/** The labeled cartridge, a standalone object so layouts can seat it in the
 *  console slot (desktop) or lean it against the TV (mobile). Origin at the
 *  bottom-center of the shell; the label faces +z. */
export function buildCart(): CartBuild {
  const g = new THREE.Group();
  box(g, new RoundedBoxGeometry(1.0, 0.46, 0.42, 2, 0.04), std(0x9c9a96, 0.85), 0, 0.23, 0);
  box(g, new THREE.BoxGeometry(1.02, 0.1, 0.26), std(0xb8b6b0, 0.85), 0, 0.41, 0);
  const label = box(
    g,
    new THREE.PlaneGeometry(0.82, 0.28),
    new THREE.MeshStandardMaterial({ color: 0x3a3f49, roughness: 0.9 }),
    0,
    0.23,
    0.215,
  ) as CartBuild["label"];
  return { group: g, label };
}

/** SFC-silhouette 16-bit console (no logos or text): glossy black slab, gray
 *  deck plate with the curved "smile" groove, a dark cart slot behind it,
 *  gray eject + dark reset + power slide switch, a colored dot row, front
 *  ports + LED, and a dogbone pad (d-pad, diamond of four buttons,
 *  start/select) on a cable. */
export function buildConsole(): THREE.Group {
  const g = new THREE.Group();
  const black = std(0x1d1f24, 0.45, 0.1);
  const plate = std(0x9aa0a8, 0.7);
  const dark = std(0x2e3138, 0.7);
  const gray = std(0xb8b6b0, 0.85);

  // Single black slab, gray deck plate inset on top.
  box(g, new RoundedBoxGeometry(2.5, 0.5, 1.75, 3, 0.1), black, 0, 0.25, 0);
  box(g, new RoundedBoxGeometry(2.26, 0.08, 1.38, 2, 0.03), plate, 0, 0.52, -0.06);

  // The "smile": a shallow dark arc grooved across the plate, bowing toward
  // the front. Torus arc centered at 6 o'clock, laid flat, squashed.
  const smile = new THREE.TorusGeometry(1.6, 0.05, 8, 40, 1.2);
  smile.rotateZ(-Math.PI / 2 - 0.6);
  smile.rotateX(-Math.PI / 2);
  smile.scale(1, 0.35, 1);
  box(g, smile, dark, 0, 0.56, -1.75);

  // Dark cart slot behind the smile; the cart itself is a buildCart() the
  // stage seats here (or elsewhere) per layout.
  box(g, new THREE.BoxGeometry(1.1, 0.04, 0.48), dark, 0, 0.55, -0.58);

  // Deck controls: power slide switch, big gray eject, dark reset.
  box(g, new RoundedBoxGeometry(0.3, 0.05, 0.42, 2, 0.02), dark, -0.72, 0.55, 0.28);
  box(g, new RoundedBoxGeometry(0.2, 0.1, 0.18, 2, 0.03), black, -0.72, 0.56, 0.2);
  box(g, new RoundedBoxGeometry(0.52, 0.07, 0.34, 2, 0.03), std(0xa8adb4, 0.65), 0, 0.55, 0.28);
  box(g, new RoundedBoxGeometry(0.26, 0.08, 0.3, 2, 0.04), dark, 0.72, 0.55, 0.28);

  // Colored dot row on the plate's back-right corner (homage, not the mark).
  const dotGeo = new THREE.CylinderGeometry(0.033, 0.033, 0.015, 12);
  [0xe04a4a, 0xe8c33c, 0x4fae62, 0x4a6fe0].forEach((c, i) =>
    box(g, dotGeo, std(c, 0.5), 0.62 + i * 0.1, 0.565, -0.62),
  );

  // Front: two controller ports + power LED.
  const portGeo = new RoundedBoxGeometry(0.34, 0.16, 0.06, 2, 0.05);
  box(g, portGeo, dark, -0.55, 0.16, 0.86);
  box(g, portGeo, dark, -0.05, 0.16, 0.86);
  box(
    g,
    new THREE.BoxGeometry(0.06, 0.06, 0.03),
    new THREE.MeshStandardMaterial({ color: 0xff5d6a, emissive: 0xff2233, emissiveIntensity: 1.2 }),
    0.85,
    0.2,
    0.88,
  );

  // Dogbone pad: extruded stadium body, d-pad, button diamond, start/select.
  const pad = new THREE.Group();
  pad.position.set(-0.95, 0, 1.55);
  pad.rotation.y = 0.45;
  pad.scale.setScalar(1.3);
  g.add(pad);
  const bodyGeo = new THREE.ExtrudeGeometry(stadium(1.05, 0.46, 0.22), {
    depth: 0.07,
    bevelEnabled: true,
    bevelThickness: 0.03,
    bevelSize: 0.03,
    bevelSegments: 3,
    curveSegments: 24,
  });
  bodyGeo.rotateX(-Math.PI / 2);
  // Body top sits at y≈0.15 (0.05 + depth 0.07 + top bevel 0.03); features at
  // 0.17 poke ~half-proud of it.
  box(pad, bodyGeo, gray, 0, 0.05, 0);
  // D-pad cross on the left disc.
  box(pad, new THREE.BoxGeometry(0.22, 0.05, 0.07), dark, -0.34, 0.17, 0);
  box(pad, new THREE.BoxGeometry(0.07, 0.05, 0.22), dark, -0.34, 0.17, 0);
  // Four buttons in a diamond on the right disc (SFC colors, no markings).
  const btnGeo = new THREE.CylinderGeometry(0.045, 0.045, 0.05, 14);
  const diamond: [number, number, number][] = [
    [0.34, -0.08, 0x4a6fe0], // top: X blue
    [0.42, 0, 0xe04a4a], // right: A red
    [0.34, 0.08, 0xe8c33c], // bottom: B yellow
    [0.26, 0, 0x4fae62], // left: Y green
  ];
  for (const [x, z, c] of diamond) box(pad, btnGeo, std(c, 0.6), x, 0.17, z);
  // Start/select: two angled slats in the middle.
  for (const dx of [-0.05, 0.07]) {
    const s = box(pad, new THREE.BoxGeometry(0.11, 0.04, 0.045), dark, dx, 0.17, 0.06);
    s.rotation.y = -0.6;
  }
  const cable = new THREE.CatmullRomCurve3([
    new THREE.Vector3(-0.95, 0.05, 1.28),
    new THREE.Vector3(-1.05, 0.02, 1.08),
    new THREE.Vector3(-0.72, 0.02, 1.02),
    new THREE.Vector3(-0.55, 0.12, 0.89),
  ]);
  g.add(new THREE.Mesh(new THREE.TubeGeometry(cable, 24, 0.018, 6), std(0x3a3f49, 0.9)));

  return g;
}

export interface CartIdentity {
  /** First frame of the toy's recorded clip (the published thumb). */
  thumbUrl: string;
  /** Author's Discord avatar URL, or null for the letter-tile fallback. */
  avatarUrl: string | null;
  handle: string;
}

/** Cart label texture: the clip's first frame cover-cropped, the author's
 *  avatar in a ring in front. Starts as a dark sticker and refreshes once the
 *  images land; a failed load just leaves that layer out. */
export function makeCartLabel(cart: CartIdentity): THREE.CanvasTexture {
  const W = 512;
  const H = 176;
  const cx = W / 2;
  const cy = H / 2;
  const r = 62;
  const c = document.createElement("canvas");
  c.width = W;
  c.height = H;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = "#23252b";
  ctx.fillRect(0, 0, W, H);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;

  const load = (url: string) =>
    new Promise<HTMLImageElement | null>((res) => {
      const i = new Image();
      i.crossOrigin = "anonymous";
      i.onload = () => res(i);
      i.onerror = () => res(null);
      i.src = url;
    });

  Promise.all([load(cart.thumbUrl), cart.avatarUrl ? load(cart.avatarUrl) : null]).then(
    ([thumb, avatar]) => {
      if (thumb) {
        const s = Math.max(W / thumb.width, H / thumb.height);
        const w = thumb.width * s;
        const h = thumb.height * s;
        ctx.drawImage(thumb, cx - w / 2, cy - h / 2, w, h);
      }
      ctx.save();
      ctx.beginPath();
      ctx.arc(cx, cy, r, 0, Math.PI * 2);
      ctx.clip();
      if (avatar) {
        ctx.drawImage(avatar, cx - r, cy - r, r * 2, r * 2);
      } else {
        ctx.fillStyle = `hsl(${hueOf(cart.handle)} 42% 38%)`;
        ctx.fillRect(cx - r, cy - r, r * 2, r * 2);
        ctx.fillStyle = "#fff";
        ctx.font = `700 ${Math.round(r * 0.84)}px system-ui, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(cart.handle.slice(0, 1).toUpperCase(), cx, cy);
      }
      ctx.restore();
      ctx.lineWidth = 7;
      ctx.strokeStyle = "#101318";
      ctx.beginPath();
      ctx.arc(cx, cy, r + 3, 0, Math.PI * 2);
      ctx.stroke();
      tex.needsUpdate = true;
    },
  );
  return tex;
}

/** Soft blob shadow: radial-gradient canvas texture on a floor-flat plane —
 *  reads as contact shadow with no shadow maps. */
export function buildBlobShadow(w: number, d: number): THREE.Mesh {
  const c = document.createElement("canvas");
  c.width = c.height = 128;
  const ctx = c.getContext("2d")!;
  const grad = ctx.createRadialGradient(64, 64, 8, 64, 64, 64);
  grad.addColorStop(0, "rgba(0,0,0,0.6)");
  grad.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, 128, 128);
  const mesh = new THREE.Mesh(
    new THREE.PlaneGeometry(w, d),
    new THREE.MeshBasicMaterial({
      map: new THREE.CanvasTexture(c),
      transparent: true,
      depthWrite: false,
    }),
  );
  mesh.rotation.x = -Math.PI / 2;
  mesh.position.y = 0.004;
  return mesh;
}
