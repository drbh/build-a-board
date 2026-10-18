// A line diff between two versions of a board, for showing what a tour step
// changed: which lines of the new text are new, and where lines of the old
// text were removed. A plain LCS: a step is a couple of hundred lines.

export interface LineDiff {
  /// 1-based lines of the new text that are not in the old one.
  added: number[];
  /// 1-based lines of the new text that removed lines sat just before
  /// (one past the last line when they were at the end).
  removed: number[];
}

export function lineDiff(before: string, after: string): LineDiff {
  const a = before.split("\n");
  const b = after.split("\n");
  // lcs[i][j]: the longest common subsequence of a[i..] and b[j..].
  const lcs = Array.from({ length: a.length + 1 }, () => new Uint16Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const added: number[] = [];
  const removed = new Set<number>();
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      i++;
      j++;
    } else if (j < b.length && (i === a.length || lcs[i][j + 1] >= lcs[i + 1][j])) {
      added.push(++j);
    } else {
      removed.add(j + 1);
      i++;
    }
  }
  return { added, removed: [...removed] };
}
