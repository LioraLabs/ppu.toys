import { DropZone } from "./DropZone";
import "../studio.css";

// DropZone is a pure props component: given an `error` string (or null) and an
// `onFiles` sink it renders the PNG picker / drag target with no wasm core and
// no asset pipeline on the render path. The drag-over highlight is local UI
// state; the convert/register work lives in DropZoneWired (see OutputCanvas).
const log = (files: FileList | File[]) =>
  console.log("DropZone onFiles:", Array.from(files).map((f) => f.name));

const Default = () => (
  <div style={{ display: "flex", width: 260 }}>
    <DropZone error={null} onFiles={log} />
  </div>
);

const WithError = () => (
  <div style={{ display: "flex", width: 260 }}>
    <DropZone error="Only PNG files are supported" onFiles={log} />
  </div>
);

export default {
  Default,
  WithError,
};
