import { useEffect, useState } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { CoreStage } from "../cosmos/FixtureStage";
import { worker } from "../mocks/browser";
import { makeMe, makeToyFull, makeWallCard } from "../fixtures";
import starPatrol from "../fixtures/starPatrol.json";
import { Play } from "./Play";

// The actual route and WASM player, with a pinned copy of the public Star Patrol
// toy from the mobile report. No duplicate page markup: editing Play edits this.
const toys = [
  { ...starPatrol, tags: ["playable", "arcade"] },
  makeToyFull({
    id: "dusk",
    title: "Dusk",
    tags: ["ambient"],
    files: [
      {
        name: "main.lua",
        source: `function frame(t, f)
  brightness = 15
  cgram[0] = rgb(80 + 40 * math.sin(t), 40, 140)
end`,
      },
    ],
  }),
];

function Fixture() {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const liked = new Set(toys.filter((toy) => toy.hearted).map((toy) => toy.id));
    worker.use(
      http.get("/api/me", () => HttpResponse.json(makeMe())),
      http.get("/api/toys", ({ request }) => {
        const params = new URL(request.url).searchParams;
        return HttpResponse.json({
          toys: toys
            .filter(
              (toy) =>
                (!params.get("tag") || toy.tags.includes(params.get("tag")!)) &&
                (!params.get("author") || toy.author.handle === params.get("author")),
            )
            .map((toy) => makeWallCard({ ...toy })),
          nextPage: null,
        });
      }),
      http.get("/api/toys/:id", ({ params }) => {
        const toy = toys.find((toy) => toy.id === params.id);
        return toy
          ? HttpResponse.json({
              ...toy,
              hearted: liked.has(toy.id),
              heartCount: toy.heartCount + Number(liked.has(toy.id)) - Number(toy.hearted),
            })
          : new HttpResponse(null, { status: 404 });
      }),
      http.put("/api/toys/:id/heart", ({ params }) => {
        liked.add(String(params.id));
        return new HttpResponse(null, { status: 204 });
      }),
      http.delete("/api/toys/:id/heart", ({ params }) => {
        liked.delete(String(params.id));
        return new HttpResponse(null, { status: 204 });
      }),
    );
    setReady(true);
    return () => worker.resetHandlers();
  }, []);

  return ready ? (
    <div>
      <CoreStage>
        <MemoryRouter initialEntries={[`/t/${starPatrol.id}/play`]}>
          <Routes>
            <Route path="/t/:id/play" element={<Play />} />
          </Routes>
        </MemoryRouter>
      </CoreStage>
    </div>
  ) : (
    <p role="status">Loading player fixture…</p>
  );
}

export const Mobile = () => <Fixture />;
export default { Mobile, Responsive: () => <Fixture /> };
