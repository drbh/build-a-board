// CodeMirror marks for a tour step's line diff: added lines tinted green with
// a bar at their left, and a red rule where lines were removed. The marks map
// through edits, so they stay on their lines while the reader types.
import { type Extension, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";
import type { LineDiff } from "./diff";

/// Show a diff (null clears it).
export const setDiff = StateEffect.define<LineDiff | null>();

const added = Decoration.line({ class: "cm-diff-added" });
const removedAbove = Decoration.line({ class: "cm-diff-removed-above" });
const removedBelow = Decoration.line({ class: "cm-diff-removed-below" });

const field = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    marks = marks.map(tr.changes);
    for (const effect of tr.effects) {
      if (!effect.is(setDiff)) continue;
      if (!effect.value) {
        marks = Decoration.none;
        continue;
      }
      const doc = tr.state.doc;
      const ranges = [];
      for (const n of effect.value.added) ranges.push(added.range(doc.line(n).from));
      for (const n of effect.value.removed) {
        if (n <= doc.lines) {
          ranges.push(removedAbove.range(doc.line(n).from));
        } else {
          ranges.push(removedBelow.range(doc.line(doc.lines).from));
        }
      }
      marks = Decoration.set(ranges, true);
    }
    return marks;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const theme = EditorView.baseTheme({
  ".cm-diff-added": { backgroundColor: "#69b78018", boxShadow: "inset 3px 0 #69b780" },
  ".cm-diff-removed-above": { boxShadow: "inset 0 2px #cf6545" },
  ".cm-diff-removed-below": { boxShadow: "inset 0 -2px #cf6545" },
  ".cm-diff-added.cm-diff-removed-above": { boxShadow: "inset 3px 0 #69b780, inset 0 2px #cf6545" },
});

export const diffMarks: Extension = [field, theme];
