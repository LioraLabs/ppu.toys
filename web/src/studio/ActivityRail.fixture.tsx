import { ActivityRail } from "./ActivityRail";
import "./studio.css";

// ActivityRail is a pure presentational nav: `active` + `assetsOpen` fully
// determine which items highlight, and `onSelect` fires on click. No store,
// no AssetsPanel, no wasm core — the wired ActivityRailWired owns those.
const Default = () => <ActivityRail active="layers" />;

// Assets toggled open: dual highlight (Assets pressed + Layers still the
// active view) — the exact production state when the assets flyout is open.
const AssetsOpen = () => <ActivityRail active="layers" assetsOpen />;

const Palette = () => <ActivityRail active="palette" />;

const Sprites = () => <ActivityRail active="sprites" />;

const Settings = () => <ActivityRail active="settings" />;

export default {
  Default,
  AssetsOpen,
  Palette,
  Sprites,
  Settings,
};
