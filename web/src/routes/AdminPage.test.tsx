// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { AdminPage } from "./AdminPage";
import { getAdminOverview, getStarterTemplate, type AdminOverview } from "../api/apiClient";
vi.mock("../api/apiClient", async (original) => ({
  ...(await original<typeof import("../api/apiClient")>()),
  getAdminOverview: vi.fn(),
  getStarterTemplate: vi.fn(),
}));
vi.mock("../api/session", () => {
  const user = { isAdmin: true };
  return { useSession: () => ({ user, loading: false }) };
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it.each([0, 4])(
  "shows creator activity and a meaningful funnel with %i creators",
  async (creators) => {
    const counts = { hour: 1, day: 2, week: 3, total: 4 };
    const data: AdminOverview = {
      activity: {
        asOf: 172800,
        users: counts,
        toys: counts,
        published: counts,
        creators: counts,
        funnel: { creators, publishers: creators / 2, repeat_publishers: creators / 4 },
        daily: [{ day: 172800, users: 1, toys: 2, published: 3, creators: 4 }],
      },
      toys: [],
      users: [],
      featuredToys: [],
      featuredToyId: "",
      featuredToyIds: [],
      storage: { usedBytes: 0, limitBytes: 1024, warning: false },
    };
    vi.mocked(getAdminOverview).mockResolvedValue(data);
    vi.mocked(getStarterTemplate).mockResolvedValue({ name: "starter", files: [] });
    render(<AdminPage />);
    const row = await screen.findByRole("row", { name: "Active creators 1 2 3 4" });
    expect(
      within(row)
        .getAllByRole("cell")
        .map((cell) => cell.textContent),
    ).toEqual(["1", "2", "3", "4"]);
    expect(screen.getByRole("row", { name: "Toys first published 1 2 3 4" })).toBeInTheDocument();
    if (creators) {
      expect(screen.getByText("50% of creators")).toBeInTheDocument();
      expect(screen.getByText("25% of creators")).toBeInTheDocument();
    } else {
      expect(screen.getAllByText("No creators yet")).toHaveLength(3);
    }
    fireEvent.click(screen.getByText("Daily activity · past 14 days (UTC)"));
    expect(screen.getByRole("row", { name: "1970-01-03 (partial) 1 2 3 4" })).toBeInTheDocument();
  },
);
