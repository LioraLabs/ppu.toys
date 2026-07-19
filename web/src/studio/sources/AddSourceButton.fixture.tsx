import { AddSourceButton } from "./AddSourceButton";
import "./sources.css";

// AddSourceButton is a self-contained toggle: it renders the "+ Source" ghost
// button and mounts AddSourceDialog on click. Rendered closed here (the button
// alone); the dialog has its own story. No wasm on the render path.
const Default = () => (
  <div style={{ padding: 16 }}>
    <AddSourceButton />
  </div>
);

export default {
  Default,
};
