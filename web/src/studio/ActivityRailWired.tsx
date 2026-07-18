import { useState } from "react";
import { ActivityRail, type RailItemId } from "./ActivityRail";
import { LibraryPanel } from "./sketches/LibraryPanel";

/** Wired container: owns the Files-panel toggle state and mounts LibraryPanel,
 *  handing the presentational ActivityRail its `active`/`filesOpen` props. The
 *  "files" action toggles the library and still forwards; later tickets claim
 *  the other rail actions. Render-identical to the pre-split ActivityRail. */
export function ActivityRailWired() {
  const [filesOpen, setFilesOpen] = useState(false);
  const select = (id: RailItemId) => {
    if (id === "files") setFilesOpen((v) => !v);
  };
  return (
    <>
      <ActivityRail active="layers" filesOpen={filesOpen} onSelect={select} />
      {filesOpen && <LibraryPanel onClose={() => setFilesOpen(false)} />}
    </>
  );
}
