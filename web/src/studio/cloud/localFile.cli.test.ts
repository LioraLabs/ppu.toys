// Pins the CLI's committed sample.ppu.json output to the Studio parser so neither side drifts silently.
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";
import { PPU_FILE_VERSION, parseFile } from "./localFile";

describe("parseFile / crates/ppu-cli sample fixture", () => {
  it("parses the CLI's committed sample.ppu.json without error", () => {
    const text = readFileSync(
      new URL("../../../../crates/ppu-cli/tests/fixtures/sample.ppu.json", import.meta.url),
      "utf8",
    );
    expect(JSON.parse(text).version).toBe(PPU_FILE_VERSION);

    const parsed = parseFile(text);

    expect(parsed.name).toBe("Sample toy");
    expect(parsed.files.map((f) => f.name)).toEqual(["main.lua", "pokes.lua"]);
    expect(parsed.sources.map((s) => s.name)).toEqual(["sky"]);
    expect(parsed.sources[0].kind).toBe("bg");
    expect(Array.from(parsed.sources[0].payload)).toEqual([1, 2, 3, 4, 5]);
    expect(parsed.origin).toEqual({ id: "toy-123", revision: 4, authorId: "u1" });
  });
});
