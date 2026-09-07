import type { DiffFragment } from "./types";

const TOKEN_RE = /[\p{L}\p{N}._%+-]+@[\p{L}\p{N}.-]+\.\p{L}{2,}|\p{L}+(?:['’]\p{L}+)*|\p{N}+(?:[.,:/-]\p{N}+)*|[%₺€$]|[^\s]/gu;

export function tokenize(text: string): string[] {
  return text.match(TOKEN_RE) ?? [];
}

function joinTokens(tokens: string[]): string {
  return tokens.reduce((result, token, index) => {
    if (index === 0) return token;
    const previous = tokens[index - 1];
    const noSpaceBefore = /^[,.;:!?%)\]}»”’]$/u.test(token);
    const noSpaceAfterPrevious = /^[([{«“‘]$/u.test(previous);
    return result + (noSpaceBefore || noSpaceAfterPrevious ? "" : " ") + token;
  }, "");
}

interface Operation {
  kind: DiffFragment["kind"];
  token: string;
}

export interface WordDiffHunk {
  removed: string;
  added: string;
}

export interface WordDiffResult {
  base: DiffFragment[];
  revised: DiffFragment[];
  hunks: WordDiffHunk[];
}

function compact(parts: Operation[]): DiffFragment[] {
  const groups: Array<{ kind: DiffFragment["kind"]; tokens: string[] }> = [];
  for (const part of parts) {
    const last = groups.at(-1);
    if (last?.kind === part.kind) last.tokens.push(part.token);
    else groups.push({ kind: part.kind, tokens: [part.token] });
  }
  return groups.map((group) => ({ kind: group.kind, text: joinTokens(group.tokens) }));
}

function diffOperations(base: string[], revised: string[]): Operation[] {
  let prefix = 0;
  while (prefix < base.length && prefix < revised.length && base[prefix] === revised[prefix]) prefix++;
  let suffix = 0;
  while (
    suffix < base.length - prefix
    && suffix < revised.length - prefix
    && base[base.length - 1 - suffix] === revised[revised.length - 1 - suffix]
  ) suffix++;

  const baseMiddle = base.slice(prefix, base.length - suffix);
  const revisedMiddle = revised.slice(prefix, revised.length - suffix);
  const operations: Operation[] = base.slice(0, prefix).map((token) => ({ kind: "equal", token }));

  if (baseMiddle.length * revisedMiddle.length > 160_000) {
    operations.push(...baseMiddle.map((token): Operation => ({ kind: "removed", token })));
    operations.push(...revisedMiddle.map((token): Operation => ({ kind: "added", token })));
  } else {
    const cols = revisedMiddle.length + 1;
    const matrix = new Uint16Array((baseMiddle.length + 1) * cols);
    for (let i = 1; i <= baseMiddle.length; i++) {
      for (let j = 1; j <= revisedMiddle.length; j++) {
        const at = i * cols + j;
        matrix[at] = baseMiddle[i - 1] === revisedMiddle[j - 1]
          ? matrix[(i - 1) * cols + j - 1] + 1
          : Math.max(matrix[(i - 1) * cols + j], matrix[i * cols + j - 1]);
      }
    }

    const middle: Operation[] = [];
    let i = baseMiddle.length;
    let j = revisedMiddle.length;
    while (i > 0 || j > 0) {
      if (i > 0 && j > 0 && baseMiddle[i - 1] === revisedMiddle[j - 1]) {
        middle.push({ kind: "equal", token: baseMiddle[i - 1] });
        i--;
        j--;
      } else if (j > 0 && (i === 0 || matrix[i * cols + j - 1] >= matrix[(i - 1) * cols + j])) {
        middle.push({ kind: "added", token: revisedMiddle[j - 1] });
        j--;
      } else {
        middle.push({ kind: "removed", token: baseMiddle[i - 1] });
        i--;
      }
    }
    operations.push(...middle.reverse());
  }

  for (let index = suffix; index > 0; index--) operations.push({ kind: "equal", token: base[base.length - index] });
  return operations;
}

function changeHunks(operations: Operation[]): WordDiffHunk[] {
  const result: WordDiffHunk[] = [];
  let removed: string[] = [];
  let added: string[] = [];
  const flush = () => {
    if (removed.length || added.length) result.push({ removed: joinTokens(removed), added: joinTokens(added) });
    removed = [];
    added = [];
  };
  for (const operation of operations) {
    if (operation.kind === "equal") flush();
    else if (operation.kind === "removed") removed.push(operation.token);
    else added.push(operation.token);
  }
  flush();
  return result;
}

export function wordDiff(baseText: string, revisedText: string): WordDiffResult {
  const operations = diffOperations(tokenize(baseText), tokenize(revisedText));
  return {
    base: compact(operations.filter((operation) => operation.kind !== "added")),
    revised: compact(operations.filter((operation) => operation.kind !== "removed")),
    hunks: changeHunks(operations),
  };
}
