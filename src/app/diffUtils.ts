export type DiffLineType = "add" | "del" | "context" | "header";

export interface DiffLine {
  type: DiffLineType;
  text: string;
}

// Guards diffLines' O(n*m) LCS table against pathologically large edits —
// beyond this, fall back to a plain "everything old removed, everything
// new added" block rather than hanging the UI thread on a huge table.
const MAX_DIFF_CELLS = 250_000;

/** Classic LCS-based line diff — good enough for the size of edits a
 * single edit_file call makes (a few lines to a few hundred), and gives a
 * real git-style diff (unchanged context kept, only the actual changed
 * lines marked) rather than just dumping the whole before/after blocks. */
export function diffLines(oldText: string, newText: string): DiffLine[] {
  const a = oldText.length ? oldText.split("\n") : [];
  const b = newText.length ? newText.split("\n") : [];
  const n = a.length;
  const m = b.length;

  if (n * m > MAX_DIFF_CELLS) {
    return [
      ...a.map((text): DiffLine => ({ type: "del", text })),
      ...b.map((text): DiffLine => ({ type: "add", text })),
    ];
  }

  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const result: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      result.push({ type: "context", text: a[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      result.push({ type: "del", text: a[i] });
      i++;
    } else {
      result.push({ type: "add", text: b[j] });
      j++;
    }
  }
  while (i < n) {
    result.push({ type: "del", text: a[i] });
    i++;
  }
  while (j < m) {
    result.push({ type: "add", text: b[j] });
    j++;
  }
  return result;
}

/** Parses an already-unified-diff string (apply_patch's own argument) into
 * displayable lines directly — no need to re-diff something that's
 * already a diff, just classify each line by its leading character. */
export function parsePatchDiff(patch: string): DiffLine[] {
  return patch.split("\n").map((line): DiffLine => {
    if (
      line.startsWith("+++") ||
      line.startsWith("---") ||
      line.startsWith("@@") ||
      line.startsWith("diff ") ||
      line.startsWith("index ")
    ) {
      return { type: "header", text: line };
    }
    if (line.startsWith("+")) return { type: "add", text: line.slice(1) };
    if (line.startsWith("-")) return { type: "del", text: line.slice(1) };
    if (line.startsWith(" ")) return { type: "context", text: line.slice(1) };
    return { type: "context", text: line };
  });
}
