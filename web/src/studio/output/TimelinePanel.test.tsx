// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { EditorView } from "@codemirror/view";
import { undo } from "@codemirror/commands";
import { IDBFactory } from "fake-indexeddb";
import { TimelinePanel, saveTimeline } from "./TimelinePanel";
import { pokeMany } from "../pokes/pokeStore";
import { EditorPane } from "../EditorPane";
import { OutputCanvas } from "./OutputCanvas";
import { DEFAULT_TIMELINE, timelineConfig, timelineSettings } from "./timeline";
import { openContextFiles, openSketchStore } from "../sketches/openSketch";
import { transport } from "../transport/transport";

vi.mock("./presenter", () => ({
  Presenter: class {
    init() {
      return true;
    }
    resize() {}
    render() {}
    dispose() {}
  },
}));

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  openSketchStore._resetForTests();
  saveTimeline(
    [
      { name: "intro", time: 2 },
      { name: "finale", time: 12 },
    ],
    DEFAULT_TIMELINE,
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("navigates, links loop bounds, and preserves links through edits and reloads", () => {
  const seek = vi.spyOn(transport, "seek");
  render(<TimelinePanel />);
  fireEvent.click(screen.getByRole("button", { name: "Go to intro" }));
  expect(seek).toHaveBeenCalledWith(2);
  fireEvent.change(screen.getByLabelText("Loop in marker"), { target: { value: "intro" } });
  fireEvent.change(screen.getByLabelText("Loop out marker"), { target: { value: "finale" } });
  expect(screen.getByLabelText("Loop in seconds")).toBeDisabled();
  const row = screen.getByRole("button", { name: "Go to intro" }).parentElement!;
  fireEvent.change(within(row).getByLabelText("Marker name"), { target: { value: "opening" } });
  fireEvent.change(within(row).getByLabelText("intro time"), { target: { value: "4" } });
  fireEvent.click(within(row).getByRole("button", { name: "Save" }));
  expect(timelineConfig(openContextFiles(openSketchStore.state()))).toMatchObject({
    loopIn: 4,
    loopOut: 12,
    loopInMarker: "opening",
    loopOutMarker: "finale",
  });
  fireEvent.click(screen.getByRole("button", { name: "Delete opening" }));
  expect(timelineSettings.get()).toMatchObject({ loopIn: 4, loopInMarker: undefined });
  expect(screen.getByLabelText("Loop in seconds")).toBeEnabled();
});

it("edits the scrubber range independently of markers and loop bounds", () => {
  const seek = vi.spyOn(transport, "seek");
  render(<OutputCanvas />);
  fireEvent.change(screen.getByLabelText("Go to marker"), { target: { value: "finale" } });
  expect(seek).toHaveBeenCalledWith(12);
  for (const value of ["60", "5"]) {
    const length = screen.getByLabelText("Timeline length in seconds");
    fireEvent.change(length, { target: { value } });
    fireEvent.blur(length);
    expect(screen.getByRole("slider", { name: "Timeline" })).toHaveAttribute("max", value);
    expect(timelineConfig(openContextFiles(openSketchStore.state())).end).toBe(Number(value));
  }
  const length = screen.getByLabelText("Timeline length in seconds");
  fireEvent.change(length, { target: { value: "" } });
  fireEvent.blur(length);
  expect(length).toHaveValue(5);
  expect(timelineSettings.get().end).toBe(5);
});

it("keeps the linked editor tab editable and live as the panel changes", () => {
  const { container } = render(
    <>
      <EditorPane onSources={() => ({ ok: true })} />
      <TimelinePanel />
    </>,
  );
  const tab = screen.getByRole("tab", { name: /timeline.lua/ });
  expect(tab).toHaveAttribute("draggable", "false");
  expect(within(tab).queryByRole("button")).not.toBeInTheDocument();
  expect(tab).toHaveTextContent("⚡");
  fireEvent.click(tab);
  const editor = () => container.querySelector(".cm-content")!;
  expect(editor()).toHaveAttribute("contenteditable", "true");
  expect(editor()).toHaveTextContent("intro = 2");
  const row = screen.getByRole("button", { name: "Go to intro" }).parentElement!;
  fireEvent.change(within(row).getByLabelText("intro time"), { target: { value: "7" } });
  fireEvent.click(within(row).getByRole("button", { name: "Save" }));
  expect(editor()).toHaveTextContent("intro = 7");
  fireEvent.click(screen.getByRole("tab", { name: /main.lua/ }));
  fireEvent.click(screen.getByRole("button", { name: "Delete intro" }));
  fireEvent.click(screen.getByRole("button", { name: "Delete finale" }));
  expect(tab).toHaveTextContent("⚙");
  act(() => pokeMany([{ lvalue: "TM", expr: "1" }]));
  expect(tab).toHaveTextContent("⚙");
  fireEvent.click(tab);
  expect(editor()).not.toHaveTextContent("intro =");
  expect(editor()).toHaveAttribute("contenteditable", "true");
});

it("syncs code edits and undo, retaining valid state through incomplete typing", () => {
  const { container } = render(
    <>
      <EditorPane onSources={() => ({ ok: true })} />
      <TimelinePanel />
    </>,
  );
  fireEvent.click(screen.getByRole("tab", { name: /timeline.lua/ }));
  const view = EditorView.findFromDOM(container.querySelector(".cm-editor")!)!;
  const write = (source: string) =>
    act(() => view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: source } }));
  const source = `-- Keep my cue notes
-- timeline: end=60 in=0 out=30 loop=true
-- loop markers: in=intro out=outro
markers = { intro = 3, outro = 20 }`;
  write(source);
  expect(screen.getByLabelText("intro time")).toHaveValue(3);
  expect(timelineSettings.get()).toMatchObject({ end: 60, loopIn: 3, loopOut: 20 });
  write(source.replace("intro = 3", "intro ="));
  expect(screen.getByRole("alert")).toHaveTextContent("last valid markers");
  expect(screen.getByLabelText("intro time")).toHaveValue(3);
  expect(screen.getByLabelText("intro time")).toBeDisabled();
  expect(saveTimeline([], DEFAULT_TIMELINE)).toBe(false);
  expect(view.state.doc.toString()).toContain("intro =,");
  write(source);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  const row = screen.getByRole("button", { name: "Go to intro" }).parentElement!;
  fireEvent.change(within(row).getByLabelText("intro time"), { target: { value: "5" } });
  fireEvent.click(within(row).getByRole("button", { name: "Save" }));
  expect(view.state.doc.toString()).toContain("intro = 5");
  expect(view.state.doc.toString()).toContain("-- Keep my cue notes");
  act(() => {
    undo(view);
  });
  expect(screen.getByLabelText("intro time")).toHaveValue(3);
  expect(timelineSettings.get().loopIn).toBe(3);
  fireEvent.click(screen.getByRole("tab", { name: /pokes.lua/ }));
  expect(container.querySelector(".cm-content")).toHaveAttribute("contenteditable", "false");
});
