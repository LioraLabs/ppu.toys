import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";

/** Editor theme wired to the design-token CSS vars (handoff "The Code Editor"),
 *  so CM6 follows the light/dark toggle. Comment + string colors are literal
 *  per the handoff's highlight table. */
const ppuEditorTheme = EditorView.theme(
  {
    "&": {
      color: "var(--txt2)",
      backgroundColor: "var(--editor)",
      fontFamily: "var(--font-mono)",
      fontSize: "13px",
      height: "100%",
    },
    ".cm-scroller": { fontFamily: "inherit", lineHeight: "21px" },
    ".cm-content": { caretColor: "var(--orange)" },
    "&.cm-focused .cm-cursor": { borderLeftColor: "var(--orange)" },
    ".cm-cursor": { borderLeftColor: "var(--orange)" },
    ".cm-fat-cursor": { background: "var(--orange)", color: "var(--editor)" },
    "&:not(.cm-focused) .cm-fat-cursor": {
      background: "none",
      outline: "1px solid var(--orange)",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "color-mix(in srgb, var(--orange) 22%, transparent)",
    },
    ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--orange) 6%, transparent)" },
    ".cm-gutters": {
      backgroundColor: "var(--editor)",
      color: "var(--faint)",
      border: "none",
    },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--mid)" },
    ".cm-tooltip": {
      backgroundColor: "var(--panel2)",
      border: "1px solid var(--line)",
      borderRadius: "6px",
    },
    ".cm-tooltip-autocomplete ul li[aria-selected]": {
      backgroundColor: "color-mix(in srgb, var(--orange) 16%, transparent)",
      color: "var(--orange)",
    },
    ".cm-tooltip.cm-completionInfo": { backgroundColor: "var(--panel2)", color: "var(--mid)" },
    ".cm-vim-panel": {
      backgroundColor: "var(--panel2)",
      color: "var(--txt)",
      fontFamily: "var(--font-mono)",
      fontSize: "12px",
    },
    ".cm-vim-panel input": { color: "var(--txt)" },
  },
  { dark: true },
);

const ppuHighlight = HighlightStyle.define([
  { tag: t.comment, color: "#5b616e", fontStyle: "italic" },
  { tag: [t.keyword, t.controlKeyword, t.definitionKeyword, t.modifier, t.operatorKeyword], color: "var(--magenta)" },
  { tag: [t.number, t.integer, t.float], color: "var(--orange)" },
  { tag: [t.string, t.special(t.string)], color: "#7ddc8b" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--yellow)" },
  { tag: [t.atom, t.bool, t.null], color: "var(--cyan)" },
  { tag: [t.operator, t.punctuation, t.separator, t.bracket], color: "var(--mid)" },
  { tag: [t.variableName, t.propertyName], color: "var(--txt2)" },
]);

export const ppuTheme: Extension = [ppuEditorTheme, syntaxHighlighting(ppuHighlight)];
