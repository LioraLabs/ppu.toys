import { decodeBase64, encodeBase64 } from "../../api/base64";
import { serializeWorkspace } from "./serialize";
import type { SketchFile, SketchSource, SketchOrigin } from "../sketches/sketchStore";
import { openContextLabel, MAIN_FILE, type OpenSketchState } from "../sketches/openSketch";
import { openAsSketch } from "./openCloudToy";
import type { SourceKind, SourceReport, ConvertSourceOptions, SourceMeta } from "../../ppu/core";

const SOURCE_KINDS: Record<SourceKind, true> = { bg: true, m7: true, obj: true, sheet: true };
const REPORT_MODES: Record<SourceReport["mode"], true> = {
  tile: true,
  m7: true,
  obj: true,
  sheet: true,
};

/** The `.ppu.json` file format tag. Bumped whenever the body shape changes;
 *  parseFile rejects anything else outright rather than guessing. */
export const PPU_FILE_VERSION = "ppu.toys/1";
/** The `.ppusrc.json` format tag: one `.ppu.json` source record on its own, so
 *  a single asset can travel between toys. */
export const PPU_SOURCE_FILE_VERSION = "ppu.toys/source/1";

type PpuFileBody = {
  version: string;
  title: string;
  description: string;
  files: SketchFile[];
  sources: ReturnType<typeof serializeWorkspace>["sources"];
  origin?: SketchOrigin;
};

/** Serialize the open sketch to `.ppu.json` text: the same files/sources the
 *  cloud save path uses (serializeWorkspace), plus a version tag, title and
 *  the sketch's origin (if linked) so a re-opened file still points home. */
export function serializeToFile(state: OpenSketchState): string {
  const { files, sources } = serializeWorkspace(state);
  const origin = state.context.sketch.origin;
  const body: PpuFileBody = {
    version: PPU_FILE_VERSION,
    title: openContextLabel(state),
    description: "",
    files,
    sources,
    ...(origin ? { origin } : {}),
  };
  return JSON.stringify(body, null, 2);
}

/** Serialize one source to `.ppusrc.json` text — the same record the cloud
 *  and `.ppu.json` paths emit, plus a version tag. */
export function serializeSourceToFile(s: SketchSource): string {
  return JSON.stringify(
    {
      version: PPU_SOURCE_FILE_VERSION,
      name: s.name,
      kind: s.kind,
      options: s.options,
      meta: s.meta,
      payload: encodeBase64(s.payload),
    },
    null,
    2,
  );
}

/** Parse and validate `.ppusrc.json` text into a SketchSource. */
export function parseSourceFile(text: string): SketchSource {
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    throw new Error("Not valid JSON.");
  }
  if (!isRecord(body) || body.version === undefined) {
    throw new Error("Not a ppu.toys source file.");
  }
  if (body.version !== PPU_SOURCE_FILE_VERSION) {
    throw new Error(`Unknown source file version: ${String(body.version)}.`);
  }
  return parseSource(body, 0);
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null;
}

/** Validate one serialized source record (an entry of a `.ppu.json`
 *  `sources[]`, or a whole `.ppusrc.json`) into a SketchSource. */
function parseSource(s: unknown, i: number): SketchSource {
  if (
    !isRecord(s) ||
    typeof s.name !== "string" ||
    typeof s.kind !== "string" ||
    typeof s.payload !== "string"
  ) {
    throw new Error(`Source ${i} is missing a name, kind, or payload.`);
  }
  if (!Object.prototype.hasOwnProperty.call(SOURCE_KINDS, s.kind)) {
    throw new Error(`Source "${s.name}" is malformed (unknown kind).`);
  }
  if (s.options !== undefined && !isRecord(s.options)) {
    throw new Error(`Source "${s.name}" is malformed (invalid options).`);
  }
  const meta = s.meta;
  if (
    !isRecord(meta) ||
    typeof meta.width !== "number" ||
    typeof meta.height !== "number" ||
    !isRecord(meta.report) ||
    typeof meta.report.mode !== "string" ||
    !Object.prototype.hasOwnProperty.call(REPORT_MODES, meta.report.mode) ||
    !isRecord(meta.report.report) ||
    (meta.report.mode !== "m7" && !Array.isArray(meta.report.report.overflows))
  ) {
    throw new Error(`Source "${s.name}" is malformed (missing or invalid meta).`);
  }
  let payload: Uint8Array;
  try {
    payload = decodeBase64(s.payload);
  } catch {
    throw new Error(`Source "${s.name}" has an invalid base64 payload.`);
  }
  return {
    name: s.name,
    kind: s.kind as SourceKind,
    options: (s.options ?? {}) as ConvertSourceOptions,
    payload,
    meta: meta as unknown as SourceMeta,
  };
}

/** Parse and validate `.ppu.json` text into a sketch's constituent parts.
 *  Rejects (throws a human-readable Error) unparsable JSON, an unknown
 *  version tag, missing/wrong-typed fields, or a source whose base64
 *  payload can't be decoded — before anything touches a store. */
export function parseFile(text: string): {
  name: string;
  files: SketchFile[];
  sources: SketchSource[];
  origin?: SketchOrigin;
} {
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    throw new Error("Not valid JSON.");
  }
  if (!isRecord(body) || body.version === undefined) {
    throw new Error("Not a ppu.toys sketch file.");
  }
  if (body.version !== PPU_FILE_VERSION) {
    throw new Error(`Unknown file version: ${String(body.version)}.`);
  }
  if (typeof body.title !== "string") throw new Error("Missing or invalid title.");
  if (!Array.isArray(body.files)) throw new Error("Missing or invalid files.");
  if (!Array.isArray(body.sources)) throw new Error("Missing or invalid sources.");

  const files: SketchFile[] = body.files.map((f, i) => {
    if (!isRecord(f) || typeof f.name !== "string" || typeof f.source !== "string") {
      throw new Error(`File ${i} is missing a name or source.`);
    }
    return { name: f.name, source: f.source };
  });
  if (!files.some((f) => f.name === MAIN_FILE)) {
    throw new Error(`File is missing ${MAIN_FILE}.`);
  }

  const sources: SketchSource[] = body.sources.map(parseSource);

  let origin: SketchOrigin | undefined;
  if (body.origin !== undefined) {
    const o = body.origin;
    if (
      !isRecord(o) ||
      typeof o.id !== "string" ||
      typeof o.revision !== "number" ||
      typeof o.authorId !== "string"
    ) {
      throw new Error("Malformed origin.");
    }
    origin = { id: o.id, revision: o.revision, authorId: o.authorId };
  }

  return { name: body.title, files, sources, origin };
}

type SaveFilePickerOptions = {
  suggestedName?: string;
  types?: { description: string; accept: Record<string, string[]> }[];
};
type PickerWindow = Window & {
  showSaveFilePicker?: (opts: SaveFilePickerOptions) => Promise<FileSystemFileHandle>;
};

/** Handles are cached per sketch id, never persisted — they're only good for
 *  the life of the page, which is exactly how long Save should stay silent. */
const handles = new Map<string, FileSystemFileHandle>();

/** Save the open sketch as a `.ppu.json`. On Chromium the first save opens
 *  the native file picker and caches its handle; every later save of the
 *  SAME sketch writes to that handle silently (a different sketch always
 *  gets its own picker call). Everywhere else, each save is a download via a
 *  throwaway anchor. Resolves `true` when something was actually written,
 *  `false` when the picker was cancelled (AbortError) — the toolbar must not
 *  report "Saved" for a no-op. A handle that fails to write (the file was
 *  moved/deleted) is evicted so the next Save re-prompts instead of failing
 *  forever. */
export async function saveLocalFile(state: OpenSketchState): Promise<boolean> {
  const text = serializeToFile(state);
  const filename = `${openContextLabel(state).replace(/\//g, "-")}.ppu.json`;
  const id = state.context.sketch.id;

  if ("showSaveFilePicker" in window) {
    let handle = handles.get(id);
    if (!handle) {
      try {
        handle = await (window as PickerWindow).showSaveFilePicker!({
          suggestedName: filename,
          types: [{ description: "ppu.toys sketch", accept: { "application/json": [".json"] } }],
        });
      } catch (e) {
        if (e instanceof Error && e.name === "AbortError") return false;
        throw e;
      }
      handles.set(id, handle);
    }
    try {
      const writable = await handle.createWritable();
      await writable.write(text);
      await writable.close();
    } catch (e) {
      handles.delete(id);
      throw e;
    }
    return true;
  }

  downloadBlob(new Blob([text], { type: "application/json" }), filename);
  return true;
}

/** Download `blob` as `filename` via a throwaway anchor. */
export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  // deferred: some Firefox builds cancel a download if its blob URL is
  // revoked within the same tick as the click.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

/** Safe download basename for a source: its name with path separators swapped. */
export const sourceFileStem = (name: string) => name.replace(/\//g, "-");

/** Save one source as `<name>.ppusrc.json`. Always a download: a source is
 *  exported once to carry elsewhere, so the silent re-save handle cache that
 *  sketches get would never pay off. */
export function saveSourceFile(s: SketchSource): void {
  downloadBlob(
    new Blob([serializeSourceToFile(s)], { type: "application/json" }),
    `${sourceFileStem(s.name)}.ppusrc.json`,
  );
}

/** Test hook: forget every cached save-picker handle. */
export function _resetLocalFileForTests(): void {
  handles.clear();
}

/** Open a `.ppu.json` file into a new local sketch: validated fully before
 *  any store call, then minted, opened, and re-linked to its origin (if
 *  any) via the shared tail also used by openCloudToy. */
export async function openLocalFile(file: File): Promise<void> {
  const parsed = parseFile(await file.text());
  await openAsSketch(parsed.name || "untitled", parsed.files, parsed.sources, parsed.origin);
}
