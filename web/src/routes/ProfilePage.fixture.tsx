import { useEffect, useState } from "react";
import { http, HttpResponse } from "msw";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { worker } from "../mocks/browser";
import { makeProfile } from "../fixtures";
import { ProfilePage } from "./ProfilePage";

// Page story: ProfilePage reads :handle from the router and fetches getProfile()
// internally; the global MSW worker answers GET /api/users/:handle from fixtures.
// The route is wired with MemoryRouter + a matching entry so useParams resolves.
function atProfile(handle = "ada") {
  return (
    <MemoryRouter initialEntries={[`/u/${handle}`]}>
      <Routes>
        <Route path="/u/:handle" element={<ProfilePage />} />
      </Routes>
    </MemoryRouter>
  );
}

const Default = () => atProfile();

const NoToys = () => {
  useState(() => {
    worker.use(http.get("/api/users/:handle", () => HttpResponse.json(makeProfile({ toys: [] }))));
    return null;
  });
  useEffect(() => () => worker.resetHandlers(), []);
  return atProfile();
};

export default {
  Default,
  NoToys,
};
