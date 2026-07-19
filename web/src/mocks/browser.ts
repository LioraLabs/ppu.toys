/** Browser MSW worker (Cosmos fixtures, dev-mode mocking). */

import { setupWorker } from "msw/browser";

import { handlers } from "./handlers";

export const worker = setupWorker(...handlers);
