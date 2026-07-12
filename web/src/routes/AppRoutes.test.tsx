// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { AppRoutes } from "./AppRoutes";

// Studio pulls in the whole engine stack; stub it for routing tests.
vi.mock("../studio/Studio", () => ({ Studio: () => <div>studio-stub</div> }));
// Data pages fetch on mount; stub the client so routing tests stay pure.
vi.mock("../api/apiClient", () => ({
  getWall: vi.fn(async () => ({ toys: [], nextPage: null })),
  getToy: vi.fn(async () => ({ files: [], sources: [], author: {}, title: "", description: "" })),
  getProfile: vi.fn(async () => ({ user: { handle: "x", avatar: null }, toys: [] })),
  SIGN_IN_URL: "/api/auth/discord",
}));
vi.mock("../api/session", () => ({
  useSession: () => ({ user: null, loading: false }),
  sessionStore: { refresh: vi.fn() },
}));

afterEach(() => cleanup());

function at(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AppRoutes />
    </MemoryRouter>,
  );
}

describe("routing", () => {
  it("renders the studio under /studio", () => {
    at("/studio");
    expect(screen.getByText("studio-stub")).toBeInTheDocument();
  });

  it("renders the ToS page under /tos", () => {
    at("/tos");
    expect(screen.getByText(/terms of service/i)).toBeInTheDocument();
  });

  it("renders the privacy page under /privacy", () => {
    at("/privacy");
    expect(screen.getByText(/privacy/i)).toBeInTheDocument();
  });
});
