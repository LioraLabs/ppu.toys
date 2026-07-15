// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { ProfilePage } from "./ProfilePage";
import { makeProfile, makeWallCard } from "../fixtures";

vi.mock("../api/apiClient", () => ({ getProfile: vi.fn() }));
vi.mock("../api/session", () => ({ useSession: () => ({ user: null, loading: false }) }));
import { getProfile } from "../api/apiClient";

const profile = makeProfile({
  toys: [makeWallCard({ id: "a", title: "Toy a", heartCount: 1 })],
});
const mockGetProfile = getProfile as ReturnType<typeof vi.fn>;
afterEach(() => { cleanup(); vi.clearAllMocks(); });

function renderAt(handle = "ada") {
  return render(
    <MemoryRouter initialEntries={[`/u/${handle}`]}>
      <Routes><Route path="/u/:handle" element={<ProfilePage />} /></Routes>
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
});
