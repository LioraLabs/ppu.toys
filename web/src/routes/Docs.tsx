import registers from "../../../docs/registers.md?raw";
import dma from "../../../docs/dma.md?raw";
import pad from "../../../docs/pad.md?raw";
import { renderMarkdown } from "./markdown";
import { useDocumentTitle } from "./useDocumentTitle";
import "./docs.css";

/** The quick reference: the repo's docs/*.md rendered in-site, stacked on one
 *  page so a toy author can Ctrl-F the whole DSL. Same files the README
 *  links, so there is one source of truth. */
const PAGES: { id: string; title: string; md: string }[] = [
  { id: "registers", title: "Registers → Lua", md: registers },
  { id: "dma", title: "Sources and dma()", md: dma },
  { id: "pad", title: "Controller: pad", md: pad },
];

export function Docs() {
  useDocumentTitle("Reference");
  return (
    <div className="doc-page docs">
      <nav className="docs-toc" aria-label="Reference sections">
        {PAGES.map((p) => (
          <a key={p.id} href={`#${p.id}`}>
            {p.title}
          </a>
        ))}
      </nav>
      {PAGES.map((p) => (
        <article key={p.id} id={p.id} className="docs-section">
          {renderMarkdown(p.md, `${p.id}-`)}
        </article>
      ))}
    </div>
  );
}
