/** Mock-data root: both stories (component props) and MSW handlers draw from
 *  these fixtures. Pure data only — no transport/ppuCore/router/msw imports. */

import type { Me, Profile, ToyFull, WallCard, WallPage } from "../api/apiClient";

export function makeWallCard(overrides?: Partial<WallCard>): WallCard {
  return {
    id: "abc123",
    title: "Dusk",
    author: { handle: "ada", avatar: null },
    thumbUrl: "/blobs/thumb/abc123",
    clipUrl: "/blobs/clip/abc123",
    heartCount: 3,
    hearted: false,
    ...overrides,
  };
}

export const wallCard: WallCard = makeWallCard();

/** A second card, so wall/profile lists have more than one entry. */
export const wallCard2: WallCard = makeWallCard({ id: "def456", title: "Ember" });

export function makeMe(overrides?: Partial<Me>): Me {
  return {
    id: "1",
    handle: "ada",
    isAdmin: false,
    ...overrides,
  };
}

export const me: Me = makeMe();

export function makeWallPage(overrides?: Partial<WallPage>): WallPage {
  return {
    toys: [wallCard, wallCard2],
    nextPage: null,
    ...overrides,
  };
}

export const wallPage: WallPage = makeWallPage();

export function makeProfile(overrides?: Partial<Profile>): Profile {
  return {
    user: { handle: "ada", avatar: null },
    toys: [wallCard, wallCard2],
    ...overrides,
  };
}

export const profile: Profile = makeProfile();

export function makeToyFull(overrides?: Partial<ToyFull>): ToyFull {
  return {
    id: "abc123",
    title: "Dusk",
    description: "A quiet sunset scene.",
    state: "published",
    files: [{ name: "main.lua", source: "-- code" }],
    sources: [],
    heartCount: 3,
    hearted: false,
    forkedFrom: null,
    author: { id: "1", handle: "ada", avatar: null },
    ...overrides,
  };
}

export const toyFull: ToyFull = makeToyFull();
