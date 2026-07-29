import { frameResult, frameScreens } from "../../fixtures";
import { ComposeTab } from "./ComposeTab";
import { makeFixtureCompositor } from "./compose/storyCompositor";
import "./compose/compose.css";
import "./inspector.css";
import "../pokes/pokes.css";

// ComposeTab is the production assembly of every compose section.
export default (
  <ComposeTab c={makeFixtureCompositor(frameResult)} screens={frameScreens} />
);
