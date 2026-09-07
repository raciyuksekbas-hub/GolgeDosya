export type BlockKind = "heading" | "article" | "paragraph" | "list" | "table";

export interface SourceTextUnit {
  id: string;
  text: string;
  kind: BlockKind;
  order: number;
  location?: string;
}

export interface DocumentBlock {
  id: string;
  kind: BlockKind;
  text: string;
  label?: string;
  order: number;
  page?: number;
  location?: string;
  sourceListOrdinal?: string;
  listLevel?: number;
  sourceClauseNumber?: string;
  appendixLabel?: string;
  appendixSection?: string;
  sourceBlockIds?: string[];
  sourceTexts?: string[];
  sourceUnits?: SourceTextUnit[];
}

export interface LocalDocument {
  name: string;
  extension: "pdf" | "doc" | "docx" | "udf";
  size: number;
  blocks: DocumentBlock[];
  warnings: string[];
}

export type FragmentKind = "equal" | "added" | "removed";

export interface DiffFragment {
  kind: FragmentKind;
  text: string;
}

export type ChangeKind = "added" | "removed" | "modified";

export interface ComparisonRow {
  id: string;
  base?: DocumentBlock;
  revised?: DocumentBlock;
  kind: "unchanged" | ChangeKind;
  baseFragments: DiffFragment[];
  revisedFragments: DiffFragment[];
  location: string;
  composite?: boolean;
  segmentationOnly?: boolean;
}

export interface DocumentChange {
  id: string;
  kind: ChangeKind;
  location: string;
  summary: string[];
  rowIndex: number;
  rowIndices: number[];
  baseText?: string;
  revisedText?: string;
  structuralGroup?: {
    articleLabel: string;
    contentCount: number;
  };
  presentation?: {
    segmentCount: number;
    visibleSegmentIds: string[];
    collapsedSegmentIds: string[];
  };
  provenance?: {
    atomicPairIds: string[];
    baseSourceBlockIds: string[];
    revisedSourceBlockIds: string[];
  };
}

export interface ComparisonResult {
  rows: ComparisonRow[];
  rawChanges: DocumentChange[];
  changes: DocumentChange[];
  trace?: {
    rows: Array<{
      rowId: string;
      cardIds: string[];
      structuralGroup: string;
      alignmentReason: string;
      splitMergeGroup?: { baseSourceBlockIds: string[]; revisedSourceBlockIds: string[] };
      oldSourceUnits: SourceTextUnit[];
      newSourceUnits: SourceTextUnit[];
      atomicPairs: Array<{
        atomicPairId: string;
        oldSourceBlockIds: string[];
        newSourceBlockIds: string[];
        oldText?: string;
        newText?: string;
      }>;
      tokenDiffHunks: Array<{
        removed: string;
        added: string;
        atomicPairId: string;
        oldSourceBlockIds: string[];
        newSourceBlockIds: string[];
        replacementReason: "same-atomic-pair" | "separate-add-delete";
      }>;
      finalPresentationSegments: Array<{
        cardId: string;
        kind: ChangeKind;
        summary: string[];
        visibleSegmentIds: string[];
        collapsedSegmentIds: string[];
      }>;
    }>;
  };
}
