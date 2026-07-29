// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { editorSettings, parseVim } from "./editorSettings";

describe("editorSettings", () => {
  beforeEach(() => {
    localStorage.clear();
    editorSettings._resetForTests();
  });

  it("defaults to plain keybindings (vim off)", () => {
    expect(editorSettings.vim()).toBe(false);
  });

  it("toggles and persists", () => {
    editorSettings.toggleVim();
    expect(editorSettings.vim()).toBe(true);
    expect(localStorage.getItem("ppu.editor.vim")).toBe("1");
    editorSettings.toggleVim();
    expect(editorSettings.vim()).toBe(false);
    expect(localStorage.getItem("ppu.editor.vim")).toBe("0");
  });

  it("notifies subscribers", () => {
    let calls = 0;
    const off = editorSettings.subscribe(() => calls++);
    editorSettings.setVim(true);
    off();
    editorSettings.setVim(false);
    expect(calls).toBe(1);
  });

  it('parseVim treats anything but "1" as off', () => {
    expect(parseVim("1")).toBe(true);
    expect(parseVim("0")).toBe(false);
    expect(parseVim(null)).toBe(false);
    expect(parseVim("yes")).toBe(false);
  });
});
