import type { ComparisonRow, DocumentChange } from "./core/types";

export type DocumentSide = "base" | "revised";

/**
 * Fark türlerinin TEK adı. Liste, ray, ekran okuyucu ve raporlar bunu kullanır;
 * dört ayrı kopya vardı ve bir sözcük değişikliği dört yerde yapılmak
 * zorundaydı.
 */
export const KIND_LABEL: Record<"added" | "removed" | "modified", string> = {
  added: "Eklenen",
  removed: "Silinen",
  modified: "Değiştirilen",
};

export function changePositionLabel(selectedIndex: number, total: number): string {
  return selectedIndex >= 0 ? `Değişiklik ${selectedIndex + 1} / ${total}` : `${total} değişiklik`;
}

export function missingBlockLabel(row: ComparisonRow, side: DocumentSide): string {
  const added = side === "base" && row.kind === "added";
  const source = added ? row.revised : row.base;
  const label = source?.label;
  if (label) return `${label} ${added ? "eklenmiştir" : "silinmiştir"}.`;
  return `Bu bölüm ${added ? "eklenmiştir" : "silinmiştir"}.`;
}

export function changeRowIndices(change: DocumentChange): number[] {
  return change.rowIndices.length ? change.rowIndices : [change.rowIndex];
}

export function copyChangesText(changes: DocumentChange[]): string {
  return changes.map((change, index) => {
    const prefix = `${index + 1}. ${change.location}`;
    if (change.structuralGroup) {
      return `${prefix} bütünüyle ${change.kind === "added" ? "eklendi" : "silindi"}.`;
    }
    if (change.kind === "added") return `${prefix}: ${change.revisedText ? `“${change.revisedText}” bölümü eklendi.` : "Bölüm eklendi."}`;
    if (change.kind === "removed") return `${prefix}: ${change.baseText ? `“${change.baseText}” bölümü silindi.` : "Bölüm silindi."}`;
    return `${prefix}: ${change.summary.join("; ")}`;
  }).join("\n");
}
