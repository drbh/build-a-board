// `?selftest`: a scripted session on the indicator board (draw it, move the
// header 3 mm, make a typo, fix it), reported to the dev or preview server,
// which appends it to .cache/selftest.jsonl.
import type { Outcome } from "./session.svelte";

export interface Editing {
  text(): string;
  replace(from: number, to: number, insert: string): void;
  compileNow(): void;
  changes(): string;
  svgBytes(): number;
}

export function selftest(editing: Editing): (outcome: Outcome) => Promise<void> {
  const session: unknown[] = [];
  const edit = (find: string, offset: number, length: number, insert: string) => {
    const at = editing.text().indexOf(find);
    if (at < 0) throw new Error(`selftest: ${find} is not in the board`);
    editing.replace(at + offset, at + offset + length, insert);
    editing.compileNow();
  };
  return async (outcome) => {
    session.push({
      kind: outcome.kind,
      text: outcome.text,
      wallMs: outcome.wallMs,
      compileMs: outcome.compileMs,
      changes: editing.changes(),
      problems: outcome.problems.map((p) => ({ level: p.level, message: p.message.split("\n")[0], span: p.span })),
      svgBytes: editing.svgBytes(),
      parts: outcome.drawing?.parts,
    });
    if (session.length === 1) edit('.place("J1", (24, 28.73))', 14, 2, "27");
    else if (session.length === 2) edit("track(V5, ", 6, 2, "V55");
    else if (session.length === 3) edit("track(V55, ", 6, 3, "V5");
    else await fetch("selftest", { method: "POST", body: JSON.stringify({ userAgent: navigator.userAgent, session }) });
  };
}
