// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ToyCard } from "./ToyCard";
import type { WallCard } from "../api/apiClient";

vi.mock("../api/apiClient", () => ({
  addHeart: vi.fn(async () => {}),
  removeHeart: vi.fn(async () => {}),
}));
import { addHeart, removeHeart } from "../api/apiClient";

const card: WallCard = {
  id: "abc123",
  title: "Dusk",
  author: { handle: "ada", avatar: null },
  thumbUrl: "/blobs/thumb/abc123",
  clipUrl: "/blobs/clip/abc123",
  heartCount: 3,
  hearted: false,
};

afterEach(() => { cleanup(); vi.clearAllMocks(); });

function renderCard(props = {}) {
  return render(
    <MemoryRouter>
      <ToyCard card={card} signedIn {...props} />
    </MemoryRouter>,
  );
}

describe("ToyCard", () => {
  it("links to the permalink and shows title + author", () => {
    renderCard();
    expect(screen.getByText("Dusk")).toBeInTheDocument();
    expect(screen.getByText("ada")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /dusk/i })).toHaveAttribute("href", "/t/abc123");
  });

  it("renders an autoplaying muted looping clip with the thumb as poster", () => {
    const { container } = renderCard();
    const video = container.querySelector("video")!;
    expect(video).toHaveAttribute("poster", "/blobs/thumb/abc123");
    expect(video.muted).toBe(true);
    expect(video.loop).toBe(true);
    expect(video).toHaveAttribute("playsinline");
  });

  it("hearts on click when signed in (optimistic count bump)", async () => {
    renderCard();
    fireEvent.click(screen.getByRole("button", { name: /heart/i }));
    expect(addHeart).toHaveBeenCalledWith("abc123");
    expect(await screen.findByText("4")).toBeInTheDocument();
  });

  it("un-hearts an already-hearted card", async () => {
    render(
      <MemoryRouter><ToyCard card={{ ...card, hearted: true }} signedIn /></MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: /heart/i }));
    expect(removeHeart).toHaveBeenCalledWith("abc123");
  });

  it("disables hearting when signed out", () => {
    render(<MemoryRouter><ToyCard card={card} signedIn={false} /></MemoryRouter>);
    expect(screen.getByRole("button", { name: /heart/i })).toBeDisabled();
  });
});
