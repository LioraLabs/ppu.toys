import type { Story, StoryDefault } from "@ladle/react";
import { ActivityRail } from "./ActivityRail";
import "./studio.css";

// ActivityRail is a pure presentational nav: `active` + `filesOpen` fully
// determine which items highlight, and `onSelect` fires on click. No store,
// no LibraryPanel, no wasm core — the wired ActivityRailWired owns those.
export default {
  title: "Studio/ActivityRail",
} satisfies StoryDefault;

export const Default: Story = () => <ActivityRail active="layers" />;

// Files toggled open: dual highlight (Files pressed + Layers still the active
// view) — the exact production state when the sketch library is open.
export const FilesOpen: Story = () => <ActivityRail active="layers" filesOpen />;

export const Palette: Story = () => <ActivityRail active="palette" />;

export const Sprites: Story = () => <ActivityRail active="sprites" />;

export const Settings: Story = () => <ActivityRail active="settings" />;
