import { useState } from "react";
import registers from "../../../docs/registers.md?raw";
import display from "../../../docs/display.md?raw";
import backgrounds from "../../../docs/backgrounds.md?raw";
import sprites from "../../../docs/sprites.md?raw";
import mode7 from "../../../docs/mode7.md?raw";
import colorMath from "../../../docs/color-math.md?raw";
import windows from "../../../docs/windows.md?raw";
import scanlines from "../../../docs/scanlines.md?raw";
import sources from "../../../docs/sources.md?raw";
import dma from "../../../docs/dma.md?raw";
import pad from "../../../docs/pad.md?raw";
import { renderMarkdown } from "./markdown";
import { useDocumentTitle } from "./useDocumentTitle";
import "./docs.css";

const CHAPTERS = [
  { id: "registers", title: "The PPU pipeline", description: "Follow a pixel from memory to the display.", registers: "VRAM → layers → screens → math", md: registers },
  { id: "display", title: "Display", description: "Brightness, blanking, modes, and mosaic.", registers: "INIDISP · BGMODE · MOSAIC", md: display },
  { id: "backgrounds", title: "Backgrounds", description: "Tilemaps, tilesets, placement, and scrolling.", registers: "BGnSC · BGnNBA · BGnOFS", md: backgrounds },
  { id: "sprites", title: "Sprites", description: "Objects, OAM, animation, and priority.", registers: "OBSEL · OAMADD · OAM", md: sprites },
  { id: "mode7", title: "Mode 7", description: "Transform one tile plane with a matrix.", registers: "M7SEL · M7A–D · M7X/Y", md: mode7 },
  { id: "color-math", title: "Screens & color math", description: "Compose layers, fixed colors, and translucency.", registers: "TM · TS · CGWSEL · CGADSUB", md: colorMath },
  { id: "windows", title: "Windows", description: "Mask layers and effects by screen region.", registers: "WH0–3 · WSEL · WLOG · TMW/TSW", md: windows },
  { id: "scanlines", title: "Scanline effects", description: "Change register state while the frame is drawn.", registers: "HDMA", md: scanlines },
  { id: "sources", title: "Sources & palettes", description: "Turn PNGs into tiles, maps, and SNES colors.", registers: "2bpp · 4bpp · 8bpp · BGR555", md: sources },
  { id: "dma", title: "Memory & dma()", description: "Place source data in VRAM and CGRAM.", registers: "VRAM · CGRAM · DMA", md: dma },
  { id: "pad", title: "Controller input", description: "Read held buttons and detect presses.", registers: "JOYPAD", md: pad },
] as const;

export function Docs() {
  useDocumentTitle("Learn the PPU");
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const matches = needle
    ? CHAPTERS.filter((chapter) =>
        [chapter.title, chapter.description, chapter.registers, chapter.md]
          .join("\n")
          .toLowerCase()
          .includes(needle),
      )
    : CHAPTERS;
  return (
    <div className="docs">
      <header className="docs-hero">
        <p className="docs-kicker">The field guide</p>
        <h1>Registers make the picture.</h1>
        <p>
          Learn the SNES PPU as a signal path. Each concept names the real
          registers, explains what they control, and shows the Lua that moves them.
        </p>
        <label className="docs-search">
          <span>Search the guide</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Try CGADSUB, palettes, or scroll"
          />
        </label>
      </header>

      {needle && (
        <p className="docs-result-count" role="status">
          {matches.length} matching {matches.length === 1 ? "chapter" : "chapters"}
        </p>
      )}
      {matches.length > 0 ? (
        <nav className="docs-map" aria-label="Documentation chapters">
          {matches.map((chapter) => {
            const index = CHAPTERS.indexOf(chapter);
            return (
              <a key={chapter.id} href={`#${chapter.id}`}>
                <span className="docs-map-number" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                <strong>{chapter.title}</strong>
                <span>{chapter.description}</span>
                <code>{chapter.registers}</code>
              </a>
            );
          })}
        </nav>
      ) : (
        <p className="docs-no-results">
          No matching chapter. Try a register such as <code>INIDISP</code>, a
          concept such as <code>palette</code>, or a Lua name such as <code>bg</code>.
        </p>
      )}

      <div className="docs-layout">
        <nav className="docs-toc" aria-label="On this page">
          <p>Signal path</p>
          {CHAPTERS.map((chapter, index) => (
            <a key={chapter.id} href={`#${chapter.id}`}>
              <span>{String(index + 1).padStart(2, "0")}</span>
              {chapter.title}
            </a>
          ))}
        </nav>

        <div className="docs-content">
          {CHAPTERS.map((chapter, index) => (
            <article key={chapter.id} id={chapter.id} className={`docs-section docs-section-${(index % 5) + 1}`}>
              <div className="docs-register-line" aria-hidden="true">
                <span>{String(index + 1).padStart(2, "0")}</span>
                <code>{chapter.registers}</code>
              </div>
              {renderMarkdown(chapter.md, `${chapter.id}-`, 1)}
            </article>
          ))}
        </div>
      </div>
    </div>
  );
}
