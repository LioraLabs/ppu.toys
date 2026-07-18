import { useAssets } from "../assets/useAssets";
import { DropZone } from "./DropZone";

/** Wired container: reads the drop/convert/register pipeline off useAssets
 *  (which touches the shared transport + sketch store) and hands the error
 *  string + file sink down to the presentational DropZone. No story — this is
 *  a thin singleton-reading wrapper; the presentational DropZone is storied. */
export function DropZoneWired() {
  const { error, addFiles } = useAssets();
  // addFiles is stable (useAssets memoizes it with []); pass it straight through
  // so DropZone's onFiles-keyed useCallbacks keep their identity across renders.
  return <DropZone error={error} onFiles={addFiles} />;
}
