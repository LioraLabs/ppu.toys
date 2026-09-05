import { describe, expect, it } from "vitest";
import {
  DEFAULT_TIMELINE,
  markerSource,
  timelineConfig,
  timelineMarkers,
  parseTimeline,
  updateMarkerSource,
} from "./timeline";

describe("timeline markers", () => {
  it("round-trips named Lua marker values in chronological order", () => {
    const source = markerSource(
      [
        { name: "finale", time: 12.5 },
        { name: "intro", time: 1 },
      ],
      { end: 45, loopIn: 1, loopOut: 12.5, looping: true },
    );
    expect(timelineMarkers([{ name: "timeline.lua", source }])).toEqual([
      { name: "intro", time: 1 },
      { name: "finale", time: 12.5 },
    ]);
    expect(source).toContain("markers = {");
    expect(source).toContain("finale = 12.500");
    expect(timelineConfig([{ name: "timeline.lua", source }])).toEqual({
      end: 45,
      loopIn: 1,
      loopOut: 12.5,
      looping: true,
    });
  });
});

it("reads inline tables, decimals, scientific notation and Lua comments", () => {
  const source = `--[[ markers = { fake = 9 } ]]
markers = { intro = .5; -- intro cue
  outro = 1.2e2, --[=[ another = 7 ]=]
}`;
  expect(parseTimeline(source).markers).toEqual([
    { name: "intro", time: 0.5 },
    { name: "outro", time: 120 },
  ]);
});

it("preserves comments and untouched formatting through rename, delete, and append", () => {
  const source = `-- My cues
markers = {
  intro -- name note
    = 2.50, -- intro note
  old = 6; -- keep this note
  outro=12 -- last note
}
-- End of cues
`;
  const markers = [
    { name: "opening", time: 3 },
    { name: "outro", time: 12 },
    { name: "finale", time: 30 },
  ];
  const next = updateMarkerSource(source, markers, DEFAULT_TIMELINE, {
    from: "intro",
    to: "opening",
  });
  expect(parseTimeline(next).markers).toEqual(markers);
  for (const comment of [
    "-- My cues",
    "-- name note",
    "-- intro note",
    "-- keep this note",
    "-- last note",
    "-- End of cues",
  ])
    expect(next).toContain(comment);
  expect(next).toContain("outro=12,");
  expect(next).toContain("opening -- name note");
});

it.each([
  "markers = { intro = }",
  "markers = { intro = 2",
  "markers = { intro = 2 + 3 }",
  "markers = { intro = 1, intro = 2 }",
  "markers = { end = 3 }",
  "markers = { intro = -1 }",
  "markers = { intro = 1e999 }",
  "--[[ unfinished",
  "markers = {} extra = 3",
  "-- timeline: end=bad in=0 out=30 loop=false\nmarkers = {}",
])("refuses incomplete or unsupported source without rewriting it: %s", (source) => {
  expect(() => updateMarkerSource(source, [], DEFAULT_TIMELINE)).toThrow();
});
