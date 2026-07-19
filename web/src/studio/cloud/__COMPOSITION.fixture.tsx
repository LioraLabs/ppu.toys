import { WorkspaceActions } from "./WorkspaceActions";
import "../../styles/tokens.css";
import "../studio.css";
import "./cloud.css";

// The production toolbar cloud composition. MSW supplies the session seam.
export default (
  <div style={{ display: "flex", justifyContent: "flex-end", padding: 24 }}>
    <WorkspaceActions />
  </div>
);
