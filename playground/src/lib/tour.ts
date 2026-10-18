// The tour: the indicator board built up one step at a time, from an outline
// to the finished `examples/indicator`. Each step is a complete board in
// ../tour/<n>-<name>.rs whose `//!` header is its title (first line) and note
// (the paragraph after it).
import { type LineDiff, lineDiff } from "./diff";

const files = import.meta.glob("../../tour/*.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export interface Step {
  title: string;
  note: string;
  source: string;
}

const number = (path: string) => Number(path.split("/").at(-1)!.split("-")[0]);

export const steps: Step[] = Object.entries(files)
  .sort(([a], [b]) => number(a) - number(b))
  .map(([, source]) => {
    const doc = source
      .split("\n")
      .filter((line) => line.startsWith("//!"))
      .map((line) => line.replace(/^\/\/! ?/, ""));
    const blank = doc.indexOf("");
    return {
      title: doc[0].replace(/\.$/, ""),
      note: doc.slice(blank + 1).join(" ").trim(),
      source,
    };
  });

/// The name the tour goes by in the example picker and the URL.
export const TOUR = "tour";

/// The lines a step's `//!` header takes.
const header = (source: string) => {
  const lines = source.split("\n");
  let n = 0;
  while (n < lines.length && lines[n].startsWith("//!")) n++;
  return { n, body: lines.slice(n).join("\n") };
};

/// What step `n` changed in the code since the step before it. The headers
/// are left out: they are the steps' notes, and differ every time.
export function stepDiff(n: number): LineDiff | null {
  if (n <= 0) return null;
  const before = header(steps[n - 1].source);
  const after = header(steps[n].source);
  const diff = lineDiff(before.body, after.body);
  return { added: diff.added.map((l) => l + after.n), removed: diff.removed.map((l) => l + after.n) };
}
