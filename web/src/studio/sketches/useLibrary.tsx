import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { listSketches, onSketchesChanged, type SketchMeta } from "./sketchStore";
import { useOpenSketch, type OpenSketchState } from "./openSketch";

/** The two reactive store reads the library panel renders from: the persisted
 *  sketch list and the open-sketch state (for the open-row highlight). */
export interface LibraryData {
  sketches: SketchMeta[];
  open: OpenSketchState;
}

/** Story/test override for the library data. In the app NO provider is mounted,
 *  so the hook falls through to the live IndexedDB list + open-sketch store.
 *  Stories/tests wrap <LibraryDataProvider data={fixture}> to drive the panel
 *  from fixture data with no IndexedDB and no wasm core. */
const LibraryDataContext = createContext<LibraryData | null>(null);

export function LibraryDataProvider({
  data,
  children,
}: {
  data: LibraryData;
  children: ReactNode;
}) {
  return <LibraryDataContext.Provider value={data}>{children}</LibraryDataContext.Provider>;
}

/** Subscribe to the persisted sketch list (IndexedDB), refreshing on any store
 *  mutation. The live implementation behind the seam. */
function useSketchList(): SketchMeta[] {
  const [list, setList] = useState<SketchMeta[]>([]);
  useEffect(() => {
    let live = true;
    const refresh = () =>
      void listSketches().then((l) => {
        if (live) setList(l);
      });
    refresh();
    const off = onSketchesChanged(refresh);
    return () => {
      live = false;
      off();
    };
  }, []);
  return list;
}

/** The library panel reads the SHARED sketch store — same list + open context as
 *  the rest of the studio — UNLESS fixture data is injected via
 *  LibraryDataProvider (stories/tests), in which case that wins. */
export function useLibraryData(): LibraryData {
  const injected = useContext(LibraryDataContext);
  // Provider presence is fixed for a given mounted subtree (app: never;
  // story/test: always), so these early returns keep hook order stable while
  // avoiding the IndexedDB subscription + open-sketch read on the injected path.
  // eslint-disable-next-line react-hooks/rules-of-hooks
  if (injected) return injected;
  // eslint-disable-next-line react-hooks/rules-of-hooks
  const sketches = useSketchList();
  // eslint-disable-next-line react-hooks/rules-of-hooks
  const open = useOpenSketch();
  return { sketches, open };
}
