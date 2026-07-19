import { ActivityRail } from "./ActivityRail";
import "./studio.css";

// ActivityRail is a pure presentational nav: `active` + `filesOpen` fully
// determine which items highlight, and `onSelect` fires on click. No store,
// no LibraryPanel, no wasm core — the wired ActivityRailWired owns those.
const Default = () => <ActivityRail active="layers" />;

// Files toggled open: dual highlight (Files pressed + Layers still the active
// view) — the exact production state when the sketch library is open.
const FilesOpen = () => <ActivityRail active="layers" filesOpen />;

const Palette = () => <ActivityRail active="palette" />;

const Sprites = () => <ActivityRail active="sprites" />;

const Settings = () => <ActivityRail active="settings" />;

export default {
  Default,
  FilesOpen,
  Palette,
  Sprites,
  Settings,
};
