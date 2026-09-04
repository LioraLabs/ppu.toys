import { describe, expect, it } from "vitest";
import { markerSource, timelineConfig, timelineMarkers } from "./timeline";

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
