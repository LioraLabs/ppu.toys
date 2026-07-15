import { useEffect, useState } from "react";
import type { Story, StoryDefault } from "@ladle/react";
import { http, HttpResponse } from "msw";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { worker } from "../mocks/browser";
import { Permalink } from "./Permalink";

// Page story: Permalink reads :id from the router and fetches getToy() internally;
// the global MSW worker answers GET /api/toys/:id from the toyFull fixture. The
// embedded ReadOnlyPlayer stays blank here — no core is loaded in the catalog, so
// it renders its frame without booting wasm (see the core guard in ReadOnlyPlayer).
export default {
  title: "Pages/Permalink",
} satisfies StoryDefault;

function atToy(id = "abc123") {
  return (
    <MemoryRouter initialEntries={[`/t/${id}`]}>
      <Routes>
        <Route path="/t/:id" element={<Permalink />} />
      </Routes>
    </MemoryRouter>
  );
}

export const Default: Story = () => atToy();

export const NotFound: Story = () => {
  useState(() => {
    worker.use(http.get("/api/toys/:id", () => new HttpResponse(null, { status: 404 })));
    return null;
  });
  useEffect(() => () => worker.resetHandlers(), []);
  return atToy("nope");
};
