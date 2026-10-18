<!-- The board's source, in CodeMirror, with the compiler's problems underlined. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { EditorView, basicSetup } from "codemirror";
  import { keymap } from "@codemirror/view";
  import { Prec } from "@codemirror/state";
  import { indentWithTab } from "@codemirror/commands";
  import { rust } from "@codemirror/lang-rust";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { type Diagnostic, lintGutter, setDiagnostics } from "@codemirror/lint";
  import type { LineDiff } from "../lib/diff";
  import { diffMarks, setDiff } from "../lib/diff-marks";
  import type { Problem } from "../lib/protocol";

  let {
    problems = [],
    onedit,
    onrun,
  }: { problems?: Problem[]; onedit?: () => void; onrun?: () => void } = $props();

  /// one-dark's syntax colors on the page's own panels.
  const panels = EditorView.theme(
    {
      "&": { backgroundColor: "var(--workspace-bg)" },
      ".cm-gutters": { backgroundColor: "var(--workspace-bg)", borderRight: "1px solid var(--line-light)" },
      ".cm-activeLine": { backgroundColor: "#ffffff08" },
      ".cm-activeLineGutter": { backgroundColor: "#ffffff08" },
    },
    { dark: true },
  );

  let parent: HTMLDivElement;
  let view: EditorView | undefined = $state();
  /// Replacing the whole text (a new example) is not an edit.
  let loading = false;

  onMount(() => {
    view = new EditorView({
      parent,
      extensions: [
        basicSetup,
        keymap.of([
          indentWithTab,
          {
            key: "Mod-Enter",
            run: () => {
              onrun?.();
              return true;
            },
          },
        ]),
        rust(),
        oneDark,
        Prec.highest(panels),
        lintGutter(),
        diffMarks,
        EditorView.updateListener.of((u) => {
          if (u.docChanged && !loading) onedit?.();
        }),
      ],
    });
    return () => view?.destroy();
  });

  /// The offset of a 1-based line and column, clamped to the document.
  function offset(line: number, col: number): number {
    const doc = view!.state.doc;
    const l = doc.line(Math.min(Math.max(line, 1), doc.lines));
    return Math.min(l.from + Math.max(col - 1, 0), l.to);
  }

  $effect(() => {
    if (!view) return;
    const diagnostics: Diagnostic[] = problems
      .filter((p) => p.span)
      .map((p) => {
        const from = offset(p.span!.line, p.span!.col);
        const to = Math.max(from + 1, offset(p.span!.endLine, p.span!.endCol));
        return { from, to: Math.min(to, view!.state.doc.length), severity: p.level, message: p.message };
      });
    view.dispatch(setDiagnostics(view.state, diagnostics));
  });

  export function text(): string {
    return view?.state.doc.toString() ?? "";
  }

  /// Replace the whole text. With a diff, mark what it changed and scroll the
  /// first change into view; without one, clear any marks.
  export function load(source: string, diff: LineDiff | null = null) {
    loading = true;
    view!.dispatch({ changes: { from: 0, to: view!.state.doc.length, insert: source } });
    view!.dispatch({ effects: setDiff.of(diff) });
    loading = false;
    const first = diff && Math.min(...diff.added, ...diff.removed.filter((n) => n <= view!.state.doc.lines));
    if (first && Number.isFinite(first)) {
      const line = view!.state.doc.line(first);
      view!.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
    } else {
      view!.dispatch({ effects: EditorView.scrollIntoView(0) });
    }
  }

  /// Replace a range, as if typed.
  export function replace(from: number, to: number, insert: string) {
    view!.dispatch({ changes: { from, to, insert } });
  }

  export function reveal(line: number, col: number) {
    view!.dispatch({ selection: { anchor: offset(line, col) }, scrollIntoView: true });
    view!.focus();
  }
</script>

<div class="editor" bind:this={parent}></div>

<style>
  .editor {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .editor :global(.cm-editor) {
    height: 100%;
  }
  .editor :global(.cm-focused) {
    outline: none;
  }
  .editor :global(.cm-scroller) {
    font: 12.5px/1.5 var(--mono);
  }
</style>
