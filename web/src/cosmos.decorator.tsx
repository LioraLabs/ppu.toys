import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import "./styles/tokens.css";
import { worker } from "./mocks/browser";

const workerReady = worker.start({ onUnhandledRequest: "bypass", quiet: true }).catch((error) => {
  console.error(error);
});

export default function CosmosRoot({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let live = true;
    void workerReady.then(() => live && setReady(true));
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = "dark";
  }, []);

  useEffect(() => {
    if (ready) document.body.dataset.cosmosReady = "true";
    return () => {
      delete document.body.dataset.cosmosReady;
    };
  }, [ready]);

  return ready ? <>{children}</> : null;
}
