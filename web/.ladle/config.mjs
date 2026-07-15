/** @type {import("@ladle/react").UserConfig} */
export default {
  stories: "src/**/*.stories.{js,jsx,ts,tsx}",
  // Ladle aliases `msw` to an empty module in the production build unless its
  // MSW addon is enabled, which breaks `ladle build` the moment any bundled
  // module imports msw (our global Provider starts the worker → mocks/browser →
  // mocks/handlers → msw). Enabling it keeps msw in the bundle so MSW-backed
  // page stories render in the built catalog and under `shoot`. The addon's
  // own runtime only activates for stories that declare `msw` meta (none of
  // ours do — they use the worker started in `.ladle/components.tsx`), so this
  // is purely the build-alias fix.
  addons: {
    msw: { enabled: true },
  },
};
