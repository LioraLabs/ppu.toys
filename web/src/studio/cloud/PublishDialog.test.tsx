// @vitest-environment jsdom
import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { PublishDialog } from "./PublishDialog";
import { openSketchStore } from "../sketches/openSketch";
import { _resetSketchStoreForTests } from "../sketches/sketchStore";
import type { Me, ToyFull } from "../../api/apiClient";

vi.mock("../../api/apiClient", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api/apiClient")>()),
  getToy: vi.fn(),
  createToy: vi.fn(),
  updateToy: vi.fn(),
  publishToy: vi.fn(),
}));
vi.mock("../../api/session", () => ({
  useSession: vi.fn(),
}));
vi.mock("./clip", () => ({
  recordClip: vi.fn(),
}));
vi.mock("./serialize", () => ({
  serializeWorkspace: () => ({
    files: [{ name: "main.lua", source: "x" }],
    sources: [],
  }),
}));

import { getToy, createToy, updateToy, publishToy, ApiError } from "../../api/apiClient";
import { useSession } from "../../api/session";
import { recordClip } from "./clip";

const mockUseSession = useSession as unknown as ReturnType<typeof vi.fn>;
const mockGetToy = getToy as unknown as ReturnType<typeof vi.fn>;
const mockCreateToy = createToy as unknown as ReturnType<typeof vi.fn>;
const mockUpdateToy = updateToy as unknown as ReturnType<typeof vi.fn>;
const mockPublishToy = publishToy as unknown as ReturnType<typeof vi.fn>;
const mockRecordClip = recordClip as unknown as ReturnType<typeof vi.fn>;

const USER: Me = { id: "1", handle: "ada", avatar: null, isAdmin: false };
const CLIP = { clip: new Blob(), thumb: new Blob() };

function makeToy(overrides?: Partial<ToyFull>): ToyFull {
  return {
    id: "abc123",
    title: "Dusk",
    tags: [],
    description: "A quiet sunset scene.",
    state: "published",
    revision: 1,
    files: [{ name: "main.lua", source: "x" }],
    sources: [],
    heartCount: 0,
    hearted: false,
    forkedFrom: null,
    author: { id: "1", handle: "ada", avatar: null },
    ...overrides,
  };
}

function renderDialog(onClose: () => void = () => undefined) {
  return render(
    <MemoryRouter>
      <PublishDialog onClose={onClose} />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  openSketchStore._resetForTests();
  mockUseSession.mockReset();
  mockUseSession.mockReturnValue({ user: USER, loading: false });
  mockGetToy.mockReset();
  mockGetToy.mockResolvedValue(makeToy());
  mockCreateToy.mockReset();
  mockUpdateToy.mockReset();
  mockPublishToy.mockReset();
  mockRecordClip.mockReset();
  mockRecordClip.mockResolvedValue(CLIP);
});
afterEach(() => cleanup());

describe("PublishDialog — no origin", () => {
  it("offers only 'Publish new toy'; click records the clip then creates then publishes, and binds origin", async () => {
    mockCreateToy.mockResolvedValue({ id: "new1", revision: 1 });
    mockPublishToy.mockResolvedValue({ id: "new1", state: "published" });
    renderDialog();

    expect(screen.getByRole("button", { name: "Publish new toy" })).toBeInTheDocument();
    expect(screen.queryByText(/^Update t\//)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Publish new toy" }));

    await waitFor(() => expect(mockPublishToy).toHaveBeenCalledTimes(1));
    expect(mockRecordClip).toHaveBeenCalledTimes(1);
    const body = mockCreateToy.mock.calls[0][0];
    expect(body.forkedFrom).toBeUndefined();
    expect(mockPublishToy.mock.calls[0][0]).toBe("new1");
    expect(openSketchStore.state().context.sketch.origin).toEqual({
      id: "new1",
      revision: 1,
      authorId: "1",
    });
  });

  it("recordClip rejecting calls neither createToy nor updateToy, and shows the error", async () => {
    mockRecordClip.mockRejectedValue(new Error("no camera"));
    renderDialog();

    fireEvent.click(screen.getByRole("button", { name: "Publish new toy" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("no camera"));
    expect(mockCreateToy).not.toHaveBeenCalled();
    expect(mockUpdateToy).not.toHaveBeenCalled();
  });

  it("create succeeds but publish rejects: origin stays unset", async () => {
    mockCreateToy.mockResolvedValue({ id: "new1", revision: 1 });
    mockPublishToy.mockRejectedValue(new Error("upload failed"));
    renderDialog();

    fireEvent.click(screen.getByRole("button", { name: "Publish new toy" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("upload failed"));
    expect(openSketchStore.state().context.sketch.origin).toBeUndefined();
  });
});

describe("PublishDialog — owned origin", () => {
  beforeEach(() => {
    openSketchStore.setOrigin({ id: "abc123", revision: 1, authorId: "1" });
  });

  it("shows Update + Publish as new toy, prefilled title from getToy", async () => {
    renderDialog();

    expect(await screen.findByDisplayValue("Dusk")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update t/abc123" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Publish as new toy" })).toBeInTheDocument();
  });

  it("Update calls updateToy(origin.id, origin.revision, body) then publishToy(origin.id), and bumps the origin revision", async () => {
    mockUpdateToy.mockResolvedValue({ revision: 2 });
    mockPublishToy.mockResolvedValue({ id: "abc123", state: "published" });
    renderDialog();
    await screen.findByDisplayValue("Dusk");

    fireEvent.click(screen.getByRole("button", { name: "Update t/abc123" }));

    await waitFor(() => expect(mockPublishToy).toHaveBeenCalledTimes(1));
    expect(mockUpdateToy.mock.calls[0][0]).toBe("abc123");
    expect(mockUpdateToy.mock.calls[0][1]).toBe(1);
    expect(mockPublishToy.mock.calls[0][0]).toBe("abc123");
    expect(mockCreateToy).not.toHaveBeenCalled();
    expect(openSketchStore.state().context.sketch.origin).toEqual({
      id: "abc123",
      revision: 2,
      authorId: "1",
    });
  });

  it.each([
    [429, /one per minute/i],
    [409, /changed elsewhere/i],
  ])(
    "updateToy rejecting with a %i shows the alert, keeps the dialog open, never calls publishToy",
    async (status, message) => {
      mockUpdateToy.mockRejectedValue(new ApiError(`PUT /api/toys/abc123 → ${status}`, status));
      renderDialog();
      await screen.findByDisplayValue("Dusk");

      fireEvent.click(screen.getByRole("button", { name: "Update t/abc123" }));

      await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(message));
      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(mockPublishToy).not.toHaveBeenCalled();
    },
  );

  it("updateToy rejecting with a 404 flips to the gone state, offering only Publish new toy", async () => {
    mockUpdateToy.mockRejectedValue(new ApiError("PUT /api/toys/abc123 → 404", 404));
    renderDialog();
    await screen.findByDisplayValue("Dusk");

    fireEvent.click(screen.getByRole("button", { name: "Update t/abc123" }));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("t/abc123 no longer exists"),
    );
    expect(screen.getByRole("button", { name: "Publish new toy" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update t/abc123" })).not.toBeInTheDocument();
  });

  it("prefill getToy rejecting with a 404 shows the gone alert, offering only Publish new toy", async () => {
    mockGetToy.mockRejectedValue(new ApiError("GET /api/toys/abc123 → 404", 404));
    renderDialog();

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("t/abc123 no longer exists"),
    );
    expect(screen.getByRole("button", { name: "Publish new toy" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update t/abc123" })).not.toBeInTheDocument();
  });

  it("Publish as new toy creates a new toy, never calls updateToy, and rebinds origin to the new id", async () => {
    mockCreateToy.mockResolvedValue({ id: "new2", revision: 1 });
    mockPublishToy.mockResolvedValue({ id: "new2", state: "published" });
    renderDialog();
    await screen.findByDisplayValue("Dusk");

    fireEvent.click(screen.getByRole("button", { name: "Publish as new toy" }));

    await waitFor(() => expect(mockPublishToy).toHaveBeenCalledTimes(1));
    expect(mockUpdateToy).not.toHaveBeenCalled();
    const body = mockCreateToy.mock.calls[0][0];
    expect(body.forkedFrom).toBeUndefined();
    expect(openSketchStore.state().context.sketch.origin).toEqual({
      id: "new2",
      revision: 1,
      authorId: "1",
    });
  });
});

describe("PublishDialog — foreign origin", () => {
  beforeEach(() => {
    openSketchStore.setOrigin({ id: "other1", revision: 5, authorId: "someone-else" });
  });

  it("offers only 'Publish new toy, forked from t/<id>'; click sends forkedFrom === origin.id", async () => {
    mockCreateToy.mockResolvedValue({ id: "new3", revision: 1 });
    mockPublishToy.mockResolvedValue({ id: "new3", state: "published" });
    renderDialog();

    const btn = screen.getByRole("button", { name: "Publish new toy, forked from t/other1" });
    fireEvent.click(btn);

    await waitFor(() => expect(mockPublishToy).toHaveBeenCalledTimes(1));
    expect(mockCreateToy.mock.calls[0][0].forkedFrom).toBe("other1");
    expect(mockUpdateToy).not.toHaveBeenCalled();
    expect(openSketchStore.state().context.sketch.origin).toEqual({
      id: "new3",
      revision: 1,
      authorId: "1",
    });
  });
});

it("validates and saves normalized tags before recording", async () => {
  mockCreateToy.mockResolvedValue({ id: "tagged", revision: 1 });
  mockPublishToy.mockResolvedValue({ id: "tagged", state: "published" });
  renderDialog();
  const tags = screen.getByLabelText("Tags");
  fireEvent.change(tags, { target: { value: "bad tag" } });
  fireEvent.click(screen.getByRole("button", { name: "Publish new toy" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Use up to 5 tags");
  expect(mockRecordClip).not.toHaveBeenCalled();
  fireEvent.change(tags, { target: { value: " Playable, arcade, playable " } });
  fireEvent.click(screen.getByRole("button", { name: "Publish new toy" }));
  await waitFor(() =>
    expect(mockCreateToy).toHaveBeenCalledWith(
      expect.objectContaining({ tags: ["playable", "arcade"] }),
    ),
  );
});
