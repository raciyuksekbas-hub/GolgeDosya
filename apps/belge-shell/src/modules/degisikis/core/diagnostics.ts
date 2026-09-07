import { looseMatchForm } from "./normalize";
import type { ComparisonResult, DocumentBlock } from "./types";

export type DiagnosticSide = "base" | "revised";

export interface DiagnosticSentinel {
  id: string;
  side: DiagnosticSide;
  needle: string;
}

export interface PipelineDiagnosticInput {
  baseExtracted: readonly string[];
  revisedExtracted: readonly string[];
  baseBlocks: readonly DocumentBlock[];
  revisedBlocks: readonly DocumentBlock[];
  comparison: ComparisonResult;
  renderedRowIndices?: ReadonlySet<number>;
}

export interface SentinelDiagnostic {
  id: string;
  side: DiagnosticSide;
  stages: {
    extractedFinalText: boolean;
    documentBlock: boolean;
    alignedPair: boolean;
    wordDiff: boolean;
    userChange: boolean;
    renderer: boolean;
  };
  indexes: {
    extracted: number;
    block: number;
    row: number;
    change: number;
  };
  firstMissingStage: keyof SentinelDiagnostic["stages"] | null;
}

function containsNeedle(text: string, needle: string): boolean {
  const normalizedNeedle = looseMatchForm(needle);
  return !!normalizedNeedle && looseMatchForm(text).includes(normalizedNeedle);
}

export function diagnoseSentinels(
  input: PipelineDiagnosticInput,
  sentinels: readonly DiagnosticSentinel[],
): SentinelDiagnostic[] {
  return sentinels.map((sentinel) => {
    const extracted = sentinel.side === "base" ? input.baseExtracted : input.revisedExtracted;
    const blocks = sentinel.side === "base" ? input.baseBlocks : input.revisedBlocks;
    const extractedIndex = extracted.findIndex((text) => containsNeedle(text, sentinel.needle));
    const blockIndex = blocks.findIndex((block) => containsNeedle(block.text, sentinel.needle));
    const rowIndex = input.comparison.rows.findIndex((row) => {
      const block = sentinel.side === "base" ? row.base : row.revised;
      return !!block && containsNeedle(block.text, sentinel.needle);
    });
    const row = input.comparison.rows[rowIndex];
    const changeIndex = input.comparison.changes.findIndex((change) => change.rowIndices.includes(rowIndex));
    const changedFragments = sentinel.side === "base" ? row?.baseFragments : row?.revisedFragments;
    const expectedKind = sentinel.side === "base" ? "removed" : "added";
    const stages: SentinelDiagnostic["stages"] = {
      extractedFinalText: extractedIndex >= 0,
      documentBlock: blockIndex >= 0,
      alignedPair: rowIndex >= 0,
      wordDiff: rowIndex >= 0 && row?.kind !== "unchanged" && !!changedFragments?.some((fragment) => fragment.kind === expectedKind),
      userChange: changeIndex >= 0,
      renderer: changeIndex >= 0 && (input.renderedRowIndices?.has(rowIndex) ?? true),
    };
    const firstMissingStage = (Object.keys(stages) as Array<keyof typeof stages>).find((stage) => !stages[stage]) ?? null;
    return {
      id: sentinel.id,
      side: sentinel.side,
      stages,
      indexes: { extracted: extractedIndex, block: blockIndex, row: rowIndex, change: changeIndex },
      firstMissingStage,
    };
  });
}
