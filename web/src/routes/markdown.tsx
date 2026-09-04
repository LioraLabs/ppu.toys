import type { ReactNode } from "react";

/** Render the markdown subset the docs/ pages use: #/##/### headings, fenced
 *  code, pipe tables, `-` lists, paragraphs, and inline code / **bold** /
 *  [links]. Nothing else, on purpose — the docs are the contract, and a real
 *  renderer is a dependency for a format we only write ourselves. Relative
 *  `x.md` links become in-page anchors so the stacked pages cross-link. */
export function renderMarkdown(md: string, keyPrefix = "", headingOffset = 0): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  let k = 0;
  const key = () => `${keyPrefix}${k++}`;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i++;
      continue;
    }
    const h = /^(#{1,3})\s+(.*)$/.exec(line);
    if (h) {
      const text = h[2].trim();
      const id = slug(text);
      const props = { id, children: inline(text) };
      const headingKey = key();
      const level = h[1].length + headingOffset;
      out.push(
        level === 1 ? (
          <h1 key={headingKey} {...props} />
        ) : level === 2 ? (
          <h2 key={headingKey} {...props} />
        ) : level === 3 ? (
          <h3 key={headingKey} {...props} />
        ) : (
          <h4 key={headingKey} {...props} />
        ),
      );
      i++;
      continue;
    }
    if (line.startsWith("```")) {
      const lang = line.slice(3).trim();
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) body.push(lines[i++]);
      i++; // closing fence
      out.push(
        <pre key={key()} data-lang={lang || undefined}>
          <code>{body.join("\n")}</code>
        </pre>,
      );
      continue;
    }
    if (line.startsWith("|")) {
      const rows: string[] = [];
      while (i < lines.length && lines[i].startsWith("|")) rows.push(lines[i++]);
      const cells = (r: string) =>
        r
          .trim()
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split(/(?<!\\)\|/)
          .map((c) => c.trim());
      const [head, sep, ...body] = rows;
      const isSep = sep !== undefined && /^\|?\s*:?-{2,}/.test(sep);
      const data = isSep ? body : rows.slice(1);
      out.push(
        <table key={key()}>
          <thead>
            <tr>
              {cells(head).map((c, ci) => (
                <th key={ci}>{inline(c)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.map((r, ri) => (
              <tr key={ri}>
                {cells(r).map((c, ci) => (
                  <td key={ci}>{inline(c)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>,
      );
      continue;
    }
    if (/^\s*[-*]\s+/.test(line) || /^\s*\d+\.\s+/.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const items: string[] = [];
      while (i < lines.length && /^\s*(?:[-*]|\d+\.)\s+/.test(lines[i])) {
        let item = lines[i++].replace(/^\s*(?:[-*]|\d+\.)\s+/, "");
        // lazy continuation lines (indented) belong to the item
        while (
          i < lines.length &&
          /^\s{2,}\S/.test(lines[i]) &&
          !/^\s*(?:[-*]|\d+\.)\s+/.test(lines[i])
        )
          item += " " + lines[i++].trim();
        items.push(item);
      }
      const li = items.map((it, n) => <li key={n}>{inline(it)}</li>);
      out.push(ordered ? <ol key={key()}>{li}</ol> : <ul key={key()}>{li}</ul>);
      continue;
    }
    // paragraph: run until a blank line or a block opener
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{1,3}\s|```|\||\s*[-*]\s+|\s*\d+\.\s+)/.test(lines[i])
    )
      para.push(lines[i++].trim());
    out.push(<p key={key()}>{inline(para.join(" "))}</p>);
  }
  return out;
}

export function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/`/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Inline: `code`, **bold**, [text](href). A `foo.md` href points at the
 *  stacked section for that file (`#foo`); `foo.md#x` keeps its fragment. */
function inline(text: string): ReactNode[] {
  const re = /(`[^`]*`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g;
  const parts: ReactNode[] = [];
  let last = 0;
  let n = 0;
  for (const m of text.matchAll(re)) {
    const idx = m.index ?? 0;
    if (idx > last) parts.push(text.slice(last, idx));
    const tok = m[0];
    if (tok.startsWith("`")) parts.push(<code key={n++}>{tok.slice(1, -1)}</code>);
    else if (tok.startsWith("**")) parts.push(<strong key={n++}>{tok.slice(2, -2)}</strong>);
    else {
      const lm = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(tok)!;
      const href = lm[2].replace(/^([\w-]+)\.md(#.*)?$/, (_, f, frag) => frag ?? `#${f}`);
      parts.push(
        <a key={n++} href={href}>
          {lm[1]}
        </a>,
      );
    }
    last = idx + tok.length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}
