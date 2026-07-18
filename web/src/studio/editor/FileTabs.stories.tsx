import type { Story, StoryDefault } from "@ladle/react";
import { FileTabs } from "./FileTabs";
import { sketchFiles } from "../../fixtures";

// FileTabs is a pure props component: given the ordered file names plus the
// active/error/generated/poked sets and CRUD+reorder handlers it renders the
// editor tab bar with no wasm core, transport, or sketch store on the render
// path. All state (rename edit box, drag target) is local UI state.
export default {
  title: "Studio/Editor/FileTabs",
} satisfies StoryDefault;

const names = sketchFiles.map((f) => f.name);
const GENERATED: ReadonlySet<string> = new Set(["pokes.lua"]);
const ERRORS: ReadonlySet<string> = new Set(["enemies.lua"]);

const handlers = {
  onSelect: (name: string) => console.log("select", name),
  onAdd: () => console.log("add"),
  onRename: (from: string, to: string) => (console.log("rename", from, to), true),
  onDelete: (name: string) => console.log("delete", name),
  onReorder: (from: number, to: number) => console.log("reorder", from, to),
};

export const Default: Story = () => (
  <FileTabs
    files={names}
    active="main.lua"
    errorFiles={ERRORS}
    generated={GENERATED}
    onSelect={handlers.onSelect}
    onAdd={handlers.onAdd}
    onRename={handlers.onRename}
    onDelete={handlers.onDelete}
    onReorder={handlers.onReorder}
  />
);

// pokes.lua has pokes applied: the generated glyph swaps ⚙ → accent ⚡.
export const Poked: Story = () => (
  <FileTabs
    files={names}
    active="pokes.lua"
    errorFiles={ERRORS}
    generated={GENERATED}
    pokedFiles={GENERATED}
    onSelect={handlers.onSelect}
    onAdd={handlers.onAdd}
    onRename={handlers.onRename}
    onDelete={handlers.onDelete}
    onReorder={handlers.onReorder}
  />
);
