// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { ProfilePage } from "./ProfilePage";
import { makeProfile, makeWallCard } from "../fixtures";

vi.mock("../api/apiClient", () => ({
  getProfile: vi.fn(),
}));
let session: {
  user: null | { id: string; handle: string; avatar: string | null };
  loading: boolean;
} = {
  user: null,
  loading: false,
};
vi.mock("../api/session", () => ({ useSession: () => session }));
import { getProfile } from "../api/apiClient";

const profile = makeProfile({
  toys: [makeWallCard({ id: "a", title: "Toy a", heartCount: 1 })],
});
const mockGetProfile = getProfile as ReturnType<typeof vi.fn>;
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  session = { user: null, loading: false };
});

function renderAt(handle = "ada") {
  return render(
    <MemoryRouter initialEntries={[`/u/${handle}`]}>
      <Routes>
        <Route path="/u/:handle" element={<ProfilePage />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("ProfilePage", () => {
  it("fetches by handle and lists the user's toys", async () => {
    mockGetProfile.mockResolvedValue(profile);
    renderAt();
    expect(await screen.findByRole("heading", { name: "ada" })).toBeInTheDocument();
    expect(screen.getByText("Toy a")).toBeInTheDocument();
    expect(mockGetProfile).toHaveBeenCalledWith("ada");
  });

  it("shows an empty state when the user has no toys", async () => {
    mockGetProfile.mockResolvedValue(makeProfile({ toys: [] }));
    renderAt();
    expect(await screen.findByText(/no published toys/i)).toBeInTheDocument();
  });

  it("own profile has no Local editing section", async () => {
    session.user = { id: "u1", handle: "ada", avatar: null };
    mockGetProfile.mockResolvedValue(profile);
    renderAt();
    expect(await screen.findByRole("heading", { name: "ada" })).toBeInTheDocument();
    expect(screen.queryByText("Local editing")).toBeNull();
    expect(screen.queryByRole("button", { name: /create cli token/i })).toBeNull();
  });
});
