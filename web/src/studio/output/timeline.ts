import type { SourceFile } from "../../ppu/core";
import { useSyncExternalStore } from "react";

export const TIMELINE_FILE = "timeline.lua";
export interface TimelineMarker {
  name: string;
  time: number;
}

export interface TimelineSettings {
  end: number;
  loopIn: number;
  loopOut: number;
  looping: boolean;
  loopInMarker?: string;
  loopOutMarker?: string;
}

export const DEFAULT_TIMELINE: TimelineSettings = {
  end: 30,
  loopIn: 0,
  loopOut: 30,
  looping: false,
  loopInMarker: undefined,
  loopOutMarker: undefined,
};
interface Token {
  text: string;
  from: number;
  to: number;
}
interface Entry {
  marker: TimelineMarker;
  tokens: Token[];
}
const NUMBER = /^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
const KEYWORDS = new Set(
  "and break do else elseif end false for function goto if in local nil not or repeat return then true until while".split(
    " ",
  ),
);
export const validMarkerName = (name: string) =>
  /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) && !KEYWORDS.has(name);

/** A declarative markers table, never evaluated as Lua. Token offsets let panel
 * edits preserve comments and formatting. Expressions stay editable in code,
 * but must become numeric literals before the panel can safely edit them. */
export function parseTimeline(source: string) {
  const tokens: Token[] = [];
  const comments: Token[] = [];
  let offset = 0;
  const fail = (message: string): never => {
    throw new Error(message);
  };
  while (offset < source.length) {
    const rest = source.slice(offset);
    const space = rest.match(/^\s+/);
    if (space) {
      offset += space[0].length;
      continue;
    }
    if (rest.startsWith("--")) {
      const long = rest.match(/^--\[(=*)\[/);
      let length: number;
      if (long) {
        const end = rest.indexOf(`]${long[1]}]`, long[0].length);
        if (end < 0) fail("Close the block comment in timeline.lua.");
        length = end + long[1].length + 2;
      } else length = rest.search(/[\r\n]/) < 0 ? rest.length : rest.search(/[\r\n]/);
      comments.push({ text: rest.slice(0, length), from: offset, to: offset + length });
      offset += length;
      continue;
    }
    const text = rest.match(
      /^(?:[A-Za-z_][A-Za-z0-9_]*|(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?|[{}=,;])/,
    )?.[0];
    if (!text) fail("Use named numeric values in markers = { intro = 2.5, outro = 12 }.");
    tokens.push({ text: text!, from: offset, to: offset + text!.length });
    offset += text!.length;
  }
  let i = 0;
  const take = (text: string) => {
    if (tokens[i]?.text !== text) fail(`Expected '${text}' in timeline.lua.`);
    return tokens[i++];
  };
  take("markers");
  take("=");
  take("{");
  const entries: Entry[] = [];
  const names = new Set<string>();
  while (tokens[i]?.text !== "}") {
    const start = i;
    const name = tokens[i++]?.text ?? "";
    if (!validMarkerName(name) || names.has(name))
      fail("Marker names must be unique Lua identifiers.");
    names.add(name);
    take("=");
    const number = tokens[i++]?.text ?? "";
    if (!NUMBER.test(number) || !Number.isFinite(Number(number)))
      fail(`Give ${name} a finite, non-negative number of seconds.`);
    if ([",", ";"].includes(tokens[i]?.text)) i++;
    else if (tokens[i]?.text !== "}") fail("Separate markers with a comma.");
    entries.push({ marker: { name, time: Number(number) }, tokens: tokens.slice(start, i) });
  }
  const close = take("}");
  if (tokens[i]?.text === ";") i++;
  if (i !== tokens.length) fail("Keep timeline.lua to a markers table and comments.");
  const markers = entries
    .map((entry) => entry.marker)
    .sort((a, b) => a.time - b.time || a.name.localeCompare(b.name));
  let config = { ...DEFAULT_TIMELINE };
  const meta = comments.filter((comment) => /^-- timeline:/.test(comment.text));
  const refs = comments.filter((comment) => /^-- loop markers:/.test(comment.text));
  if (meta.length > 1 || refs.length > 1)
    fail("Keep one timeline settings comment and one loop markers comment.");
  if (meta.length) {
    const match = meta[0].text.match(
      /^-- timeline: end=(\S+) in=(\S+) out=(\S+) loop=(true|false)\s*$/,
    );
    if (
      !match ||
      !match.slice(1, 4).every((value) => NUMBER.test(value) && Number.isFinite(Number(value))) ||
      Number(match[1]) < 1
    )
      fail("Timeline settings need a length of at least 1 and non-negative loop times.");
    config = {
      ...config,
      end: Number(match![1]),
      loopIn: Number(match![2]),
      loopOut: Number(match![3]),
      looping: match![4] === "true",
    };
  }
  if (refs.length) {
    const match = refs[0].text.match(/^-- loop markers: in=(\w*) out=(\w*)\s*$/);
    if (!match) fail("Check the loop marker references in timeline.lua.");
    config.loopInMarker = match![1] || undefined;
    config.loopOutMarker = match![2] || undefined;
  }
  return {
    markers,
    config: resolveTimeline(config, markers),
    entries,
    close,
    meta: meta[0],
    refs: refs[0],
  };
}

function sourceOf(files: readonly SourceFile[]) {
  return (
    files.find((file) => file.name === TIMELINE_FILE)?.source ?? markerSource([], DEFAULT_TIMELINE)
  );
}
export function timelineMarkers(files: readonly SourceFile[]): TimelineMarker[] {
  return parseTimeline(sourceOf(files)).markers;
}
export function timelineConfig(files: readonly SourceFile[]): TimelineSettings {
  return parseTimeline(sourceOf(files)).config;
}

let settings: TimelineSettings = DEFAULT_TIMELINE;
let markers: TimelineMarker[] = [];
let error: string | undefined;
let lastSource: string | undefined;
let lastSession: number | undefined;
const listeners = new Set<() => void>();
export const timelineSettings = {
  get: () => settings,
  markers: () => markers,
  error: () => error,
  subscribe: (listener: () => void) => {
    listeners.add(listener);
    return () => void listeners.delete(listener);
  },
};

/** Called synchronously by the sketch store: no effect can overwrite a newer
 * editor change, and incomplete typing keeps the last valid panel values. */
export function syncTimeline(files: readonly SourceFile[], session: number) {
  const source = sourceOf(files);
  if (source === lastSource && session === lastSession) return;
  if (session !== lastSession) {
    settings = DEFAULT_TIMELINE;
    markers = [];
  }
  lastSource = source;
  lastSession = session;
  try {
    const parsed = parseTimeline(source);
    settings = parsed.config;
    markers = parsed.markers;
    error = undefined;
  } catch (cause) {
    error = (cause as Error).message;
  }
  for (const listener of listeners) listener();
}
export function useTimelineSettings() {
  return useSyncExternalStore(timelineSettings.subscribe, timelineSettings.get);
}
export function useTimelineMarkers() {
  return useSyncExternalStore(timelineSettings.subscribe, timelineSettings.markers);
}
export function useTimelineError() {
  return useSyncExternalStore(timelineSettings.subscribe, timelineSettings.error);
}

export function resolveTimeline(
  config: TimelineSettings,
  markers: readonly TimelineMarker[],
): TimelineSettings {
  const start = markers.find((marker) => marker.name === config.loopInMarker);
  const finish = markers.find((marker) => marker.name === config.loopOutMarker);
  return {
    ...config,
    loopIn: start?.time ?? config.loopIn,
    loopOut: finish?.time ?? config.loopOut,
    loopInMarker: start?.name,
    loopOutMarker: finish?.name,
  };
}

export function markerSource(
  markers: readonly TimelineMarker[],
  config: TimelineSettings = settings,
): string {
  const rows = [...markers]
    .sort((a, b) => a.time - b.time || a.name.localeCompare(b.name))
    .map(({ name, time }) => `  ${name} = ${time.toFixed(3)},`);
  return `-- timeline.lua · edit here or in the Markers panel. Values are seconds.\n-- timeline: end=${config.end} in=${config.loopIn} out=${config.loopOut} loop=${config.looping}\n-- loop markers: in=${config.loopInMarker ?? ""} out=${config.loopOutMarker ?? ""}\nmarkers = {\n${rows.join("\n")}\n}\n`;
}

/** Patch only marker tokens and settings comments; leave user comments intact. */
export function updateMarkerSource(
  source: string,
  markers: readonly TimelineMarker[],
  config: TimelineSettings,
  rename?: { from: string; to: string },
): string {
  const parsed = parseTimeline(source); // Refuse to overwrite an incomplete edit.
  const pending = new Map(markers.map((marker) => [marker.name, marker]));
  const edits: { from: number; to: number; text: string }[] = [];
  let last: Entry | undefined;
  for (const entry of parsed.entries) {
    const name = entry.marker.name === rename?.from ? rename.to : entry.marker.name;
    const next =
      rename && entry.marker.name === rename.to && rename.from !== rename.to
        ? undefined
        : pending.get(name);
    if (next) {
      last = entry;
      pending.delete(name);
      if (name !== entry.marker.name) edits.push({ ...entry.tokens[0], text: name });
      if (next.time !== entry.marker.time)
        edits.push({ ...entry.tokens[2], text: String(next.time) });
    } else {
      for (const token of entry.tokens) edits.push({ ...token, text: "" });
    }
  }
  if (pending.size) {
    if (last && last.tokens.length === 3)
      edits.push({ from: last.tokens[2].to, to: last.tokens[2].to, text: "," });
    edits.push({
      from: parsed.close.from,
      to: parsed.close.from,
      text: `\n${[...pending.values()].map(({ name, time }) => `  ${name} = ${time},`).join("\n")}\n`,
    });
  }
  const meta = `-- timeline: end=${config.end} in=${config.loopIn} out=${config.loopOut} loop=${config.looping}`;
  const refs = `-- loop markers: in=${config.loopInMarker ?? ""} out=${config.loopOutMarker ?? ""}`;
  let prefix = "";
  for (const [token, text] of [
    [parsed.meta, meta],
    [parsed.refs, refs],
  ] as const) {
    if (token) edits.push({ ...token, text });
    else prefix += text + "\n";
  }
  for (const edit of edits.sort((a, b) => b.from - a.from))
    source = source.slice(0, edit.from) + edit.text + source.slice(edit.to);
  const result = prefix + source;
  parseTimeline(result); // Invalid panel input never reaches the stored source.
  return result;
}
