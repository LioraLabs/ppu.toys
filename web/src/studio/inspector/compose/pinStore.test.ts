import { afterEach, describe, expect, it } from "vitest";
import { ppuCore } from "../../../ppu/instance";
import { transport } from "../../transport/transport";
import { REG, effectiveReg, toggleMaskBit } from "./model";
import { pinsSnapshot, releaseAllPins, releasePin, screensFor, writePin, writePins } from "./pinStore";

/** These run against the REAL singletons (mock core + shared transport), which
 *  is the point: they verify the wiring the tabs and overlay share. */

const regValue = (name: string) =>
  transport.getSnapshot().frame.registers.find((r) => r.name === name)?.value;

afterEach(() => {
  releaseAllPins();
});

describe("pin store", () => {
  it("writePin pins + re-renders: the register readout shows the pinned value", () => {
    transport.scrub(0.5); // land on a nonzero clock so the script value is live
    writePin(REG.COLDATA, 0x7abc);
    expect(ppuCore.listPins()).toEqual([{ addr: REG.COLDATA, value: 0x7abc }]);
    expect(regValue("COLDATA")).toBe(0x7abc);
  });

  it("releasePin restores the script-driven value", () => {
    transport.scrub(0.5);
    const script = regValue("COLDATA");
    writePin(REG.COLDATA, 0x7abc);
    expect(regValue("COLDATA")).toBe(0x7abc);
    releasePin(REG.COLDATA);
    expect(regValue("COLDATA")).toBe(script);
    expect(pinsSnapshot()).toEqual([]);
  });

  it("matrix toggle end-to-end: pin created, effective readout shows it", () => {
    const frame = () => transport.getSnapshot().frame.registers;
    const before = effectiveReg(frame(), pinsSnapshot(), REG.TM); // mock omits TM -> power-on 0x1f
    expect(before).toEqual({ value: 0x1f, pinned: false });
    const w = toggleMaskBit(REG.TM, before.value, 0);
    writePin(w.addr, w.value);
    expect(effectiveReg(frame(), pinsSnapshot(), REG.TM)).toEqual({ value: 0x1e, pinned: true });
  });

  it("writePins applies a multi-register batch (combine writes both LOG regs)", () => {
    writePins([
      { addr: REG.WBGLOG, value: 0xaa },
      { addr: REG.WOBJLOG, value: 0x0a },
    ]);
    expect(ppuCore.listPins()).toEqual([
      { addr: REG.WBGLOG, value: 0xaa },
      { addr: REG.WOBJLOG, value: 0x0a },
    ]);
  });

  it("pinsSnapshot keeps a stable reference while pins are unchanged", () => {
    writePin(REG.COLDATA, 0x1234);
    const a = pinsSnapshot();
    expect(pinsSnapshot()).toBe(a);
    writePin(REG.TM, 0x17);
    const b = pinsSnapshot();
    expect(b).not.toBe(a);
    expect(pinsSnapshot()).toBe(b);
  });

  it("recompile keeps pins; ▶ Run (transport.restart) clears them", () => {
    writePin(REG.COLDATA, 0x7abc);
    transport.setSources([{ name: "main.lua", source: "-- edited" }]);
    expect(pinsSnapshot()).toEqual([{ addr: REG.COLDATA, value: 0x7abc }]);
    transport.restart();
    expect(pinsSnapshot()).toEqual([]);
  });

  it("screensFor caches per frame object and hands back core buffers", () => {
    const frame = transport.getSnapshot().frame;
    const s = screensFor(frame);
    expect(screensFor(frame)).toBe(s);
    expect(s.main.length).toBe(256 * 224 * 4);
    expect(s.sub.length).toBe(256 * 224 * 4);
    expect(s.mathMask.length).toBe(256 * 224);
  });
});
