/** Demo model + assembly helpers, shared by the bundled demos (demos.ts) and
 *  the tutorial toys (tutorials/). Pure + node-safe (no DOM). */
import { EMPTY_POKES } from "../pokes/pokes";
import type { SourceKind, ConvertSourceOptions } from "../../ppu/core";

export interface DemoAsset {
  /** Literal slot id referenced from Lua (bg[n].source / obj.sheet). */
  id: string;
  width: number;
  height: number;
  data: Uint8ClampedArray; // width*height*4 RGBA
  /** Format the generator commits to at bind time (matches the demo's mode). */
  kind: SourceKind;
  options: ConvertSourceOptions;
}

export interface DemoFile {
  name: string;
  source: string;
}

export interface Demo {
  id: string;
  label: string;
  /** Single-file form. For multi-file demos this is the files joined in tab
   *  order with "\n" — the concatenation the parity golden proves equivalent. */
  source: string;
  /** Multi-file demos only. Tab order = chunk execution order (PICO-8 scope). */
  files?: DemoFile[];
  assets: DemoAsset[];
}

/** Ordered files of a demo — single-file demos present as one main.lua. */
export function demoFiles(d: Demo): DemoFile[] {
  return d.files ?? [{ name: "main.lua", source: d.source }];
}

/** Files joined in tab order with "\n" — the Demo.source doc contract above. */
function demoSource(files: DemoFile[]): string {
  return files.map((f) => f.source).join("\n");
}

/** Build a Demo from its non-pokes files: prepends the generated pokes.lua
 *  and derives `source` from the full (pokes-included) file list. */
export function demo(id: string, label: string, files: DemoFile[], assets: DemoAsset[]): Demo {
  const withPokes = [{ name: "pokes.lua", source: EMPTY_POKES }, ...files];
  return { id, label, source: demoSource(withPokes), files: withPokes, assets };
}
