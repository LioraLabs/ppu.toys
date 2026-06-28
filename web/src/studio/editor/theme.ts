import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";

/** Dark theme matching design tokens (handoff "The Code Editor"). */
const ppuEditorTheme = EditorView.theme(
  {
    "&": {
      color: "#c8ccd6",
      backgroundColor: "#0d0f14",
      fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
      fontSize: "13px",
      height: "100%",
    },
    ".cm-scroller": { fontFamily: "inherit", lineHeight: "21px" },
    ".cm-content": { caretColor: "#ff9540" },
    "&.cm-focused .cm-cursor": { borderLeftColor: "#ff9540" },
    ".cm-cursor": { borderLeftColor: "#ff9540" },
    ".cm-fat-cursor": { background: "#ff9540", color: "#0d0f14" },
    "&:not(.cm-focused) .cm-fat-cursor": { background: "none", outline: "1px solid #ff9540" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "rgba(255,149,64,0.20)",
    },
    ".cm-activeLine": { backgroundColor: "rgba(255,149,64,0.06)" },
    ".cm-gutters": {
      backgroundColor: "#0d0f14",
      color: "#3f4654",
      border: "none",
    },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "#9aa1ae" },
    ".cm-tooltip": {
      backgroundColor: "#16181f",
      border: "1px solid #2f3542",
      borderRadius: "6px",
    },
    ".cm-tooltip-autocomplete ul li[aria-selected]": {
      backgroundColor: "#231a10",
      color: "#ffce9a",
    },
    ".cm-tooltip.cm-completionInfo": { backgroundColor: "#16181f", color: "#9aa1ae" },
    ".cm-vim-panel": {
      backgroundColor: "#12141a",
      color: "#e7e9ef",
      fontFamily: '"IBM Plex Mono", ui-monospace, monospace',
      fontSize: "12px",
    },
    ".cm-vim-panel input": { color: "#e7e9ef" },
  },
  { dark: true },
);

const ppuHighlight = HighlightStyle.define([
  { tag: t.comment, color: "#5b616e", fontStyle: "italic" },
  { tag: [t.keyword, t.controlKeyword, t.definitionKeyword, t.modifier, t.operatorKeyword], color: "#ff4d9d" },
  { tag: [t.number, t.integer, t.float], color: "#ff9540" },
  { tag: [t.string, t.special(t.string)], color: "#7ddc8b" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "#ffd166" },
  { tag: [t.atom, t.bool, t.null], color: "#5fc9e8" },
  { tag: [t.operator, t.punctuation, t.separator, t.bracket], color: "#9aa1ae" },
  { tag: [t.variableName, t.propertyName], color: "#c8ccd6" },
]);

export const ppuTheme: Extension = [ppuEditorTheme, syntaxHighlighting(ppuHighlight)];
