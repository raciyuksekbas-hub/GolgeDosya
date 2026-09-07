import { describe, expect, it } from "vitest";
import { changePositionLabel, changeRowIndices, copyChangesText, missingBlockLabel } from "./uiLabels";
import type { ComparisonRow, DocumentBlock, DocumentChange } from "./core/types";

function block(text: string, label?: string): DocumentBlock {
  return { id: text, text, label, kind: label ? "article" : "paragraph", order: 0 };
}

describe("arayüz etiketleri", () => {
  it("seçim yokken toplamı, seçim varken X / Y konumunu gösterir", () => {
    expect(changePositionLabel(-1, 10)).toBe("10 değişiklik");
    expect(changePositionLabel(2, 10)).toBe("Değişiklik 3 / 10");
  });

  it("eklenen ve silinen maddeler için açık yer tutucu üretir", () => {
    const added = { id: "a", kind: "added", revised: block("Madde 3 Yeni hüküm", "Madde 3") } as ComparisonRow;
    const removed = { id: "r", kind: "removed", base: block("Madde 4 Eski hüküm", "Madde 4") } as ComparisonRow;
    expect(missingBlockLabel(added, "base")).toBe("Madde 3 eklenmiştir.");
    expect(missingBlockLabel(removed, "revised")).toBe("Madde 4 silinmiştir.");
  });

  it("madde numarası yoksa genel fakat düzgün yer tutucu üretir", () => {
    const added = { id: "a", kind: "added", revised: block("Yeni paragraf") } as ComparisonRow;
    expect(missingBlockLabel(added, "base")).toBe("Bu bölüm eklenmiştir.");
  });

  it("gruplanmış kartın bütün satır aralığını seçer ve tek satır kopyalar", () => {
    const grouped = {
      id: "g",
      kind: "removed",
      location: "Madde 11 — Belgelerin Saklanması",
      summary: ["Madde bütünüyle silindi.", "2 içerik paragrafı"],
      rowIndex: 4,
      rowIndices: [4, 5, 6],
      structuralGroup: { articleLabel: "Madde 11", contentCount: 2 },
    } as DocumentChange;

    expect(changeRowIndices(grouped)).toEqual([4, 5, 6]);
    expect(copyChangesText([grouped])).toBe("1. Madde 11 — Belgelerin Saklanması bütünüyle silindi.");
    expect(changePositionLabel(0, 1)).toBe("Değişiklik 1 / 1");
  });
});
