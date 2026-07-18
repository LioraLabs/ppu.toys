import type { Story, StoryDefault } from "@ladle/react";
import { DropZone } from "./DropZone";
import "../studio.css";

// DropZone is a pure props component: given an `error` string (or null) and an
// `onFiles` sink it renders the PNG picker / drag target with no wasm core and
// no asset pipeline on the render path. The drag-over highlight is local UI
// state; the convert/register work lives in DropZoneWired (see OutputCanvas).
export default {
  title: "Studio/Output/DropZone",
} satisfies StoryDefault;

const log = (files: FileList | File[]) =>
  console.log("DropZone onFiles:", Array.from(files).map((f) => f.name));

export const Default: Story = () => (
  <div style={{ display: "flex", width: 260 }}>
    <DropZone error={null} onFiles={log} />
  </div>
);

export const WithError: Story = () => (
  <div style={{ display: "flex", width: 260 }}>
    <DropZone error="Only PNG files are supported" onFiles={log} />
  </div>
);
