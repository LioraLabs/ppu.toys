/** Mock-data root: both stories (component props) and future MSW handlers
 *  draw from these fixtures. */

import type { WallCard } from "../api/apiClient";

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
