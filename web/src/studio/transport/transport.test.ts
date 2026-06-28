import { describe, it, expect, beforeEach } from "vitest";
import { transport } from "./transport";
import { LOOP_SECONDS } from "../output/clock";

describe("transport store", () => {
  beforeEach(() => {
    transport.setPlaying(true);
    transport.scrub(0);
  });

  it("getSnapshot is stable until something changes", () => {
    const a = transport.getSnapshot();
    expect(transport.getSnapshot()).toBe(a);
    transport.step(16);
    expect(transport.getSnapshot()).not.toBe(a);
  });

  it("step advances the clock and produces a frame", () => {
    transport.scrub(0);
    const t0 = transport.getSnapshot().t;
    transport.step(100);
    const s = transport.getSnapshot();
    expect(s.t).toBeGreaterThan(t0);
    expect(s.frame.framebuffer.length).toBeGreaterThan(0);
  });

  it("setPlaying toggles play state and zeroes fps when paused", () => {
    transport.setPlaying(false);
    expect(transport.getSnapshot().playing).toBe(false);
    expect(transport.getSnapshot().fps).toBe(0);
    transport.setPlaying(true);
    expect(transport.getSnapshot().playing).toBe(true);
  });

  it("scrub maps a 0..1 fraction onto the loop", () => {
    transport.scrub(0.5);
    expect(transport.getSnapshot().t).toBeCloseTo(LOOP_SECONDS * 0.5, 5);
  });

  it("setSource forwards to the core and refreshes the snapshot", () => {
    const before = transport.getSnapshot();
    const res = transport.setSource("function frame() end");
    expect(res.ok).toBe(true);
    expect(transport.getSnapshot()).not.toBe(before);
  });

  it("setLayerVisible reaches the shared core (hiding bg1 darkens output)", () => {
    transport.scrub(0.05);
    transport.setLayerVisible("bg1", false);
    const fb = transport.getSnapshot().frame.framebuffer;
    expect(fb[0]).toBe(0);
    transport.setLayerVisible("bg1", true);
  });
});
