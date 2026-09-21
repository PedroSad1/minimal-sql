import { syntaxHighlighting } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import {
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
} from "@codemirror/view";
import { classHighlighter } from "@lezer/highlight";

const chrome = EditorView.theme({
  "&": {
    height: "100%",
    backgroundColor: "var(--bks-text-editor-bg-color)",
    color: "var(--bks-text-editor-fg-color)",
    fontSize: "13px",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-scroller": {
    fontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
    lineHeight: "1.45",
  },
  ".cm-content": {
    caretColor: "var(--bks-text-editor-cursor-bg-color)",
    padding: "0.75rem 0",
  },
  ".cm-gutters": {
    backgroundColor: "var(--bks-text-editor-gutter-bg-color)",
    color: "var(--bks-text-editor-linenumber-fg-color)",
    border: "none",
  },
  ".cm-gutterElement": {
    padding: "0 8px 0 12px",
  },
  ".cm-activeLine": {
    backgroundColor: "var(--bks-text-editor-activeline-bg-color)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "var(--bks-text-editor-activeline-gutter-bg-color)",
    color: "var(--bks-text-editor-linenumber-active-fg-color)",
  },
  ".cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":
    {
      backgroundColor: "var(--bks-text-editor-selected-bg-color) !important",
    },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--bks-text-editor-cursor-bg-color)",
  },
  ".cm-placeholder": {
    color: "var(--text)",
  },
});

export const graphiteEditor: Extension = [
  chrome,
  drawSelection(),
  dropCursor(),
  highlightActiveLine(),
  highlightActiveLineGutter(),
  syntaxHighlighting(classHighlighter),
];
