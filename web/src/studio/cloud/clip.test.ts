// @vitest-environment jsdom
import { describe, it, expect } from "vitest";
import { isRecordingSupported, recordClip } from "./clip";

describe("clip recorder support guard", () => {
  it("reports unsupported when MediaRecorder/captureStream are unavailable (as in this test env)", () => {
    expect(isRecordingSupported()).toBe(false);
  });

  it("rejects recordClip() before touching MediaRecorder or canvas APIs", async () => {
    await expect(recordClip()).rejects.toThrow(/recording|supported/i);
  });
});
