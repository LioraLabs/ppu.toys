// @vitest-environment jsdom
import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { renderMarkdown } from "./markdown";

const html = (md: string) => renderToStaticMarkup(<>{renderMarkdown(md)}</>);

describe("renderMarkdown", () => {
  it("renders headings with ids, tables, fences, lists and inline marks", () => {
    const out = html(
      [
        "# PPU registers → Lua",
        "",
        "Some **bold** and `code` and a [link](dma.md) and [frag](pad.md#where).",
        "",
        "| Register | Lua |",
        "|---|---|",
        "| `$2100` | `brightness` |",
        "",
        "```lua",
        "brightness = 15",
        "```",
        "",
        "- one",
        "- two",
        "  continued",
      ].join("\n"),
    );
    expect(out).toContain('<h1 id="ppu-registers-lua">');
    expect(out).toContain("<strong>bold</strong>");
    expect(out).toContain("<code>code</code>");
    expect(out).toContain('<a href="#dma">link</a>');
    expect(out).toContain('<a href="#where">frag</a>');
    expect(out).toContain("<th>Register</th>");
    expect(out).toContain("<td><code>$2100</code></td><td><code>brightness</code></td>");
    expect(out).toContain('<pre data-lang="lua"><code>brightness = 15</code></pre>');
    expect(out).toContain("<li>two continued</li>");
  });

  it("can nest a document beneath a page heading", () => {
    const out = renderToStaticMarkup(<>{renderMarkdown("# Chapter\n\n## Topic", "", 1)}</>);
    expect(out).toContain('<h2 id="chapter">Chapter</h2>');
    expect(out).toContain('<h3 id="topic">Topic</h3>');
  });
});
