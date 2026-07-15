/** Browser MSW worker (Ladle stories, dev-mode mocking). */

import { setupWorker } from "msw/browser";

import { handlers } from "./handlers";

export const worker = setupWorker(...handlers);
