// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";

const channels: FakeChannel[] = [];
const { setPlaying, seek } = vi.hoisted(() => ({ setPlaying: vi.fn(), seek: vi.fn() }));
class FakeChannel {
  onmessage: ((event: MessageEvent) => void) | null = null;
  postMessage = vi.fn();
  close = vi.fn();
  constructor(readonly name: string) {
    channels.push(this);
  }
}
vi.stubGlobal("BroadcastChannel", FakeChannel);
vi.mock("../transport/transport", () => ({ transport: { setPlaying, seek } }));
vi.mock("../../components/ReadOnlyPlayer", () => ({
  ReadOnlyPlayer: ({ files, raw }: { files: unknown[]; raw: boolean }) => (
    <div>{raw ? `raw ${files.length}` : "player"}</div>
  ),
}));

import { STUDIO_RAW_CHANNEL, StudioRawOutput } from "./StudioRawOutput";

afterEach(() => {
  cleanup();
  channels.length = 0;
});

it("requests and renders the Studio's latest draft", () => {
  render(<StudioRawOutput />);
  const channel = channels[0];
  expect(channel.name).toBe(STUDIO_RAW_CHANNEL);
  expect(channel.postMessage).toHaveBeenCalledWith({ type: "request" });

  act(() =>
    channel.onmessage?.({
      data: { type: "state", files: [{ name: "main.lua", source: "" }], sources: [] },
    } as MessageEvent),
  );
  expect(screen.getByText("raw 1")).toBeInTheDocument();

  act(() =>
    channel.onmessage?.({ data: { type: "clock", t: 4.5, playing: true } } as MessageEvent),
  );
  expect(setPlaying).toHaveBeenCalledWith(false);
  expect(seek).toHaveBeenCalledWith(4.5);

  window.dispatchEvent(new KeyboardEvent("keydown", { code: "Space" }));
  window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyR" }));
  expect(channel.postMessage).toHaveBeenCalledWith({ type: "toggle" });
  expect(channel.postMessage).toHaveBeenCalledWith({ type: "restart" });
});
