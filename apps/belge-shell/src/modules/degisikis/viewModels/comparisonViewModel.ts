import type { ComparisonResult, ComparisonRow, DocumentChange } from "../core/types";

export type ChangeKind = DocumentChange["kind"];

/**
 * Karşılaştırma motorunun ham çıktısını arayüz ve rapor için normalize eder.
 * Motor, extractor'lar ve veri modeli değişmez; bu katman yalnız okur ve türetir.
 */
export interface ComparisonChange {
  id: string;
  /** 1 tabanlı görsel sıra. */
  index: number;
  /** Rayda görünen biçim: "01", "02", … */
  displayIndex: string;
  kind: ChangeKind;
  /** Motorun ürettiği konum etiketi, ör. "Madde 4.1". */
  sectionLabel: string;
  leftText?: string;
  rightText?: string;
  /** Motorun ürettiği kısa diff özeti satırları. */
  summary: string[];
  /** İki paneldeki karşılığı olan satır indeksleri. */
  rowIndices: number[];
  structural?: { articleLabel: string; contentCount: number };
}

export interface ComparisonSummary {
  total: number;
  added: number;
  removed: number;
  modified: number;
  addedPct: number;
  removedPct: number;
  modifiedPct: number;
}

export interface ComparisonViewModel {
  changes: ComparisonChange[];
  summary: ComparisonSummary;
  rowCount: number;
}

export function changeRowIndicesOf(change: DocumentChange): number[] {
  return change.rowIndices.length ? change.rowIndices : [change.rowIndex];
}

export function formatDisplayIndex(index: number): string {
  return String(index).padStart(2, "0");
}

/**
 * Yüzdeler en büyük kalan yöntemiyle dağıtılır; böylece sıfırdan büyük bir
 * toplamda üç oran her zaman tam 100 eder ve yuvarlama kaybı oluşmaz.
 */
export function distributePercentages(counts: number[], total: number): number[] {
  if (total <= 0) return counts.map(() => 0);
  const exact = counts.map((count) => (count / total) * 100);
  const result = exact.map(Math.floor);
  let remainder = 100 - result.reduce((sum, value) => sum + value, 0);
  const byFraction = exact
    .map((value, index) => ({ index, fraction: value - Math.floor(value) }))
    .sort((a, b) => b.fraction - a.fraction || a.index - b.index);
  for (const entry of byFraction) {
    if (remainder <= 0) break;
    result[entry.index] += 1;
    remainder -= 1;
  }
  return result;
}

function textFromRows(rows: ComparisonRow[], indices: number[], side: "base" | "revised"): string | undefined {
  const texts = indices
    .map((index) => rows[index])
    .filter((row): row is ComparisonRow => !!row)
    .map((row) => (side === "base" ? row.base?.text : row.revised?.text))
    .filter((text): text is string => !!text && text.trim().length > 0);
  return texts.length ? texts.join("\n") : undefined;
}

export function buildComparisonViewModel(comparison: ComparisonResult): ComparisonViewModel {
  const changes: ComparisonChange[] = comparison.changes.map((change, position) => {
    const index = position + 1;
    const rowIndices = changeRowIndicesOf(change);
    return {
      id: change.id,
      index,
      displayIndex: formatDisplayIndex(index),
      kind: change.kind,
      sectionLabel: change.location,
      leftText: change.baseText ?? textFromRows(comparison.rows, rowIndices, "base"),
      rightText: change.revisedText ?? textFromRows(comparison.rows, rowIndices, "revised"),
      summary: change.summary,
      rowIndices,
      structural: change.structuralGroup,
    };
  });

  return { changes, summary: summarize(changes), rowCount: comparison.rows.length };
}

export function summarize(changes: Pick<ComparisonChange, "kind">[]): ComparisonSummary {
  const total = changes.length;
  const added = changes.filter((change) => change.kind === "added").length;
  const removed = changes.filter((change) => change.kind === "removed").length;
  const modified = changes.filter((change) => change.kind === "modified").length;
  const [addedPct, removedPct, modifiedPct] = distributePercentages([added, removed, modified], total);
  return { total, added, removed, modified, addedPct, removedPct, modifiedPct };
}

export type ChangeFilter = "all" | ChangeKind;

export function filterChanges(changes: ComparisonChange[], filter: ChangeFilter): ComparisonChange[] {
  return filter === "all" ? changes : changes.filter((change) => change.kind === filter);
}
