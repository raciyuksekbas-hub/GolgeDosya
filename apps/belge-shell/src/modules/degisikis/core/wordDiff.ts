import type { DiffFragment } from "./types";

const TOKEN_RE = /[\p{L}\p{N}._%+-]+@[\p{L}\p{N}.-]+\.\p{L}{2,}|\p{L}+(?:['’]\p{L}+)*|\p{N}+(?:[.,:/-]\p{N}+)*|[%₺€$]|[^\s]/gu;

export function tokenize(text: string): string[] {
  return text.match(TOKEN_RE) ?? [];
}

/**
 * Sözcük birimi ve kaynaktaki yeri.
 *
 * Parça metni sözcüklerin tahminle yeniden birleştirilmesiyle DEĞİL, kaynak
 * metinden kesilerek kurulur. Eskiden "T.C." "T. C.", "A.Ş.'nin"
 * "A. Ş. ' nin", "%18" "% 18" diye yeniden yazılıyordu; hukuk metninde tutar,
 * kısaltma ve atıf belgede nasıl yazılmışsa öyle görünmeli.
 */
interface SourceToken {
  text: string;
  start: number;
  end: number;
}

function sourceTokens(text: string): SourceToken[] {
  return Array.from(text.matchAll(TOKEN_RE), (match) => ({
    text: match[0],
    start: match.index,
    end: match.index + match[0].length,
  }));
}

/** Ardışık sözcüklerin kaynaktaki metni (aradaki boşluklar dahil). */
function sourceSpan(text: string, tokens: SourceToken[], from: number, to: number): string {
  return from > to ? "" : text.slice(tokens[from].start, tokens[to].end);
}

interface Operation {
  kind: DiffFragment["kind"];
  /** Temel metindeki sözcük sırası (eklenen sözcükte yok). */
  base?: number;
  /** Değişik metindeki sözcük sırası (çıkarılan sözcükte yok). */
  revised?: number;
}

interface Side {
  text: string;
  tokens: SourceToken[];
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

/**
 * Aynı türden ardışık sözcükler tek parça olur. Bir taraftaki ardışık
 * sözcükler kaynakta da ardışıktır; parça metni kaynaktan kesilir. Parçanın
 * önünde kaynakta boşluk yoksa `spaceBefore: false` taşır ("18.500" + ".").
 */
function compact(side: Side, parts: Operation[], index: (operation: Operation) => number): DiffFragment[] {
  const groups: Array<{ kind: DiffFragment["kind"]; from: number; to: number }> = [];
  for (const part of parts) {
    const at = index(part);
    const last = groups.at(-1);
    if (last?.kind === part.kind) last.to = at;
    else groups.push({ kind: part.kind, from: at, to: at });
  }
  return groups.map((group) => {
    const fragment: DiffFragment = { kind: group.kind, text: sourceSpan(side.text, side.tokens, group.from, group.to) };
    if (group.from > 0 && side.tokens[group.from - 1].end === side.tokens[group.from].start) fragment.spaceBefore = false;
    return fragment;
  });
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
  const operations: Operation[] = base.slice(0, prefix).map((_, index) => ({ kind: "equal", base: index, revised: index }));

  if (baseMiddle.length * revisedMiddle.length > 160_000) {
    operations.push(...baseMiddle.map((_, index): Operation => ({ kind: "removed", base: prefix + index })));
    operations.push(...revisedMiddle.map((_, index): Operation => ({ kind: "added", revised: prefix + index })));
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
        middle.push({ kind: "equal", base: prefix + i - 1, revised: prefix + j - 1 });
        i--;
        j--;
      } else if (j > 0 && (i === 0 || matrix[i * cols + j - 1] >= matrix[(i - 1) * cols + j])) {
        middle.push({ kind: "added", revised: prefix + j - 1 });
        j--;
      } else {
        middle.push({ kind: "removed", base: prefix + i - 1 });
        i--;
      }
    }
    operations.push(...middle.reverse());
  }

  for (let index = suffix; index > 0; index--) {
    operations.push({ kind: "equal", base: base.length - index, revised: revised.length - index });
  }
  return operations;
}

/** Bir hunk'ın çıkarılan ve eklenen sözcükleri kendi tarafında ardışıktır. */
function changeHunks(base: Side, revised: Side, operations: Operation[]): WordDiffHunk[] {
  const result: WordDiffHunk[] = [];
  let removed: number[] = [];
  let added: number[] = [];
  const flush = () => {
    if (removed.length || added.length) {
      result.push({
        removed: removed.length ? sourceSpan(base.text, base.tokens, removed[0], removed.at(-1)!) : "",
        added: added.length ? sourceSpan(revised.text, revised.tokens, added[0], added.at(-1)!) : "",
      });
    }
    removed = [];
    added = [];
  };
  for (const operation of operations) {
    if (operation.kind === "equal") flush();
    else if (operation.kind === "removed") removed.push(operation.base!);
    else added.push(operation.revised!);
  }
  flush();
  return result;
}

export function wordDiff(baseText: string, revisedText: string): WordDiffResult {
  const base: Side = { text: baseText, tokens: sourceTokens(baseText) };
  const revised: Side = { text: revisedText, tokens: sourceTokens(revisedText) };
  const operations = diffOperations(base.tokens.map((token) => token.text), revised.tokens.map((token) => token.text));
  return {
    base: compact(base, operations.filter((operation) => operation.kind !== "added"), (operation) => operation.base!),
    revised: compact(revised, operations.filter((operation) => operation.kind !== "removed"), (operation) => operation.revised!),
    hunks: changeHunks(base, revised, operations),
  };
}
