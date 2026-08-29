/** Procedural low-poly models for the landing hero: a late-80s CRT TV and a
 *  generic 16-bit console + pad. Built from three.js primitives — no external
 *  assets, no trade dress. Dimensions are eyeballed world units; the stage
 *  camera and lights are tuned to them. */
import * as THREE from "three";
import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";

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

export interface TvBuild {
  group: THREE.Group;
  screen: THREE.Mesh;
}

/** Cream-cased CRT TV: shell, charcoal face, curved screen offset left, a
 *  right-hand control column (knobs + speaker slits), feet, rabbit ears. */
export function buildTv(screenMat: THREE.Material): TvBuild {
  const g = new THREE.Group();
  const shell = std(0xd8cfc0, 0.9);
  const dark = std(0x1a1d24, 0.7);
  const trim = std(0x2c3038, 0.6);

  box(g, new RoundedBoxGeometry(3.5, 2.8, 2.4, 4, 0.14), shell, 0, 1.55, 0);
  box(g, new RoundedBoxGeometry(3.26, 2.56, 0.2, 3, 0.08), dark, 0, 1.55, 1.12);

  const screen = box(g, curvedScreen(2.36, 2.06, 0.12), screenMat, -0.35, 1.55, 1.24);

  // Control column: two knobs, a power LED, speaker slits.
  const knobGeo = new THREE.CylinderGeometry(0.11, 0.11, 0.08, 20);
  knobGeo.rotateX(Math.PI / 2);
  box(g, knobGeo, trim, 1.18, 2.25, 1.26);
  box(g, knobGeo, trim, 1.18, 1.85, 1.26);
  box(
    g,
    new THREE.BoxGeometry(0.07, 0.07, 0.03),
    new THREE.MeshStandardMaterial({ color: 0xff5d6a, emissive: 0xff2233, emissiveIntensity: 1.4 }),
    1.18,
    1.5,
    1.24,
  );
  const slit = new THREE.BoxGeometry(0.5, 0.035, 0.03);
  for (let i = 0; i < 5; i++) box(g, slit, std(0x11141a, 0.95), 1.18, 0.78 + i * 0.11, 1.24);

  const footGeo = new THREE.BoxGeometry(0.5, 0.14, 0.5);
  box(g, footGeo, std(0x171310, 0.95), -1.2, 0.07, 0.55);
  box(g, footGeo, std(0x171310, 0.95), 1.2, 0.07, 0.55);

  // Rabbit ears.
  box(g, new THREE.SphereGeometry(0.1, 12, 10), trim, 0.2, 2.98, -0.3);
  const earGeo = new THREE.CylinderGeometry(0.012, 0.02, 1.5, 6);
  const earL = box(g, earGeo, trim, -0.14, 3.6, -0.3);
  earL.rotation.z = 0.5;
  const earR = box(g, earGeo, trim, 0.54, 3.6, -0.3);
  earR.rotation.z = -0.5;

  return { group: g, screen };
}

/** Generic 16-bit console: gray rounded slab, raised deck with cartridge,
 *  purple sliders, controller ports, and a pad on a curved cable. */
export function buildConsole(): THREE.Group {
  const g = new THREE.Group();
  const gray = std(0xb6bac2, 0.8);
  const gray2 = std(0x9aa0ab, 0.8);
  const dark = std(0x2e3138, 0.7);
  const purple = std(0x7b68b8, 0.7);

  box(g, new RoundedBoxGeometry(2.3, 0.34, 1.6, 3, 0.07), gray, 0, 0.17, 0);
  box(g, new RoundedBoxGeometry(1.35, 0.22, 1.05, 3, 0.06), gray2, -0.25, 0.42, -0.1);
  box(g, new THREE.BoxGeometry(0.78, 0.22, 0.44), std(0x6f7480, 0.8), -0.25, 0.6, -0.1);
  box(g, new THREE.BoxGeometry(0.28, 0.06, 0.16), purple, 0.72, 0.37, -0.35);
  box(g, new THREE.BoxGeometry(0.28, 0.06, 0.16), purple, 0.72, 0.37, -0.05);
  const portGeo = new THREE.CylinderGeometry(0.09, 0.09, 0.06, 16);
  portGeo.rotateX(Math.PI / 2);
  box(g, portGeo, dark, -0.45, 0.17, 0.81);
  box(g, portGeo, dark, -0.05, 0.17, 0.81);

  // Pad: body, d-pad cross, four colored buttons, cable back to port one.
  const pad = new THREE.Group();
  pad.position.set(-0.75, 0, 1.35);
  pad.rotation.y = 0.45;
  g.add(pad);
  box(pad, new RoundedBoxGeometry(0.85, 0.12, 0.42, 2, 0.05), gray, 0, 0.06, 0);
  box(pad, new THREE.BoxGeometry(0.2, 0.05, 0.07), dark, -0.24, 0.12, 0);
  box(pad, new THREE.BoxGeometry(0.07, 0.05, 0.2), dark, -0.24, 0.12, 0);
  const btnGeo = new THREE.CylinderGeometry(0.04, 0.04, 0.05, 12);
  const btnCols = [0xe04a4a, 0xe8c33c, 0x4fae62, 0x4a6fe0];
  const btnPos: [number, number][] = [
    [0.24, -0.07],
    [0.31, 0],
    [0.17, 0],
    [0.24, 0.07],
  ];
  btnPos.forEach(([x, z], i) => box(pad, btnGeo, std(btnCols[i], 0.6), x, 0.12, z));
  const cable = new THREE.CatmullRomCurve3([
    new THREE.Vector3(-0.75, 0.05, 1.15),
    new THREE.Vector3(-0.85, 0.02, 1.0),
    new THREE.Vector3(-0.6, 0.02, 0.95),
    new THREE.Vector3(-0.45, 0.1, 0.84),
  ]);
  g.add(new THREE.Mesh(new THREE.TubeGeometry(cable, 24, 0.018, 6), std(0x3a3f49, 0.9)));

  return g;
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
