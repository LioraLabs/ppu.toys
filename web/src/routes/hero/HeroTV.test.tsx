// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import HeroTV from "./HeroTV";
import { makeWallCard } from "../../fixtures";

vi.mock("../../api/apiClient", () => ({ getWall: vi.fn(), getToy: vi.fn() }));
// The stage needs real WebGL; wiring is what's under test.
vi.mock("./HeroStage", () => ({ HeroStage: () => <div data-testid="stage" /> }));
vi.mock("../../ppu/instance", () => ({ ppuCore: {} }));
vi.mock("../../studio/transport/transport", () => ({
  transport: {
    setSources: vi.fn(),
    addSource: vi.fn(),
    subscribe: vi.fn(() => () => {}),
    getSnapshot: vi.fn(() => ({ frame: { framebuffer: new Uint8ClampedArray(4) } })),
  },
}));
import { getWall, getToy } from "../../api/apiClient";
import { transport } from "../../studio/transport/transport";

const mockGetWall = getWall as ReturnType<typeof vi.fn>;
const mockGetToy = getToy as ReturnType<typeof vi.fn>;

const toy = {
  id: "t1",
  title: "Mode 7 Road",
  description: "",
  state: "published",
  revision: 1,
  files: [{ name: "main.lua", source: "-- hi" }],
  // btoa("hi") payload exercises the decode+push path; builtin ref is skipped.
  sources: [
    { name: "sheet", kind: "m10", builtinId: null, options: {}, meta: {}, payload: "aGk=" },
    { name: "ref", kind: "builtin", builtinId: "b", options: {}, meta: {}, payload: null },
  ],
  heartCount: 0,
  hearted: false,
  forkedFrom: null,
  author: { id: "u1", handle: "alex", avatar: null },
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function renderHero() {
  return render(
    <MemoryRouter>
      <HeroTV fallback={<div data-testid="fallback" />} />
    </MemoryRouter>,
  );
}

describe("HeroTV", () => {
  it("fetches the top popular toy, pushes its program, and links the stage to its permalink", async () => {
    mockGetWall.mockResolvedValue({ toys: [makeWallCard({ id: "t1" })], nextPage: null });
    mockGetToy.mockResolvedValue(toy);
    renderHero();
    expect(await screen.findByText(/Mode 7 Road/)).toBeInTheDocument();
    expect(mockGetWall).toHaveBeenCalledWith("popular", 0);
    expect(mockGetToy).toHaveBeenCalledWith("t1");
    expect(transport.setSources).toHaveBeenCalledWith(toy.files);
    expect(transport.addSource).toHaveBeenCalledTimes(1); // payload-less builtin skipped
    // Sources must register BEFORE setSources: the setup-stage dma() only
    // places sources it can already see.
    const addOrder = (transport.addSource as ReturnType<typeof vi.fn>).mock.invocationCallOrder[0];
    const setOrder = (transport.setSources as ReturnType<typeof vi.fn>).mock.invocationCallOrder[0];
    expect(addOrder).toBeLessThan(setOrder);
    expect(screen.getByTitle(/Mode 7 Road/)).toHaveAttribute("href", "/t/t1");
    expect(screen.getByTestId("stage")).toBeInTheDocument();
  });

  it("renders the fallback when the wall is empty", async () => {
    mockGetWall.mockResolvedValue({ toys: [], nextPage: null });
    renderHero();
    expect(await screen.findByTestId("fallback")).toBeInTheDocument();
  });
});
