import { describe, expect, it } from "vitest";
import type { ComparisonResult, DocumentChange } from "../core/types";
import {
  buildComparisonViewModel, distributePercentages, filterChanges, formatDisplayIndex, summarize,
} from "./comparisonViewModel";

function change(partial: Partial<DocumentChange> & Pick<DocumentChange, "id" | "kind">): DocumentChange {
  return {
    location: "Madde 1",
    summary: [],
    rowIndex: 0,
    rowIndices: [],
    ...partial,
  } as DocumentChange;
}

function result(changes: DocumentChange[], rows: ComparisonResult["rows"] = []): ComparisonResult {
  return { rows, rawChanges: changes, changes };
}

describe("karşılaştırma view model", () => {
  it("sayıları ham sonuçtan türetir", () => {
    const model = buildComparisonViewModel(result([
      change({ id: "a", kind: "added" }),
      change({ id: "b", kind: "removed" }),
      change({ id: "c", kind: "modified" }),
      change({ id: "d", kind: "modified" }),
    ]));
    expect(model.summary.total).toBe(4);
    expect(model.summary.added).toBe(1);
    expect(model.summary.removed).toBe(1);
    expect(model.summary.modified).toBe(2);
  });

  it("numaralandırma belge sırasını izler ve iki hane ile biçimlenir", () => {
    const model = buildComparisonViewModel(result(
      Array.from({ length: 11 }, (_, index) => change({ id: `c${index}`, kind: "modified" })),
    ));
    expect(model.changes[0].index).toBe(1);
    expect(model.changes[0].displayIndex).toBe("01");
    expect(model.changes[9].displayIndex).toBe("10");
    expect(model.changes[10].displayIndex).toBe("11");
    expect(model.changes.map((item) => item.id)).toEqual(model.changes.map((item) => item.id));
  });

  it("aynı girdi için aynı kimlik ve sırayı üretir", () => {
    const input = result([change({ id: "x", kind: "added" }), change({ id: "y", kind: "removed" })]);
    const first = buildComparisonViewModel(input);
    const second = buildComparisonViewModel(input);
    expect(second.changes.map((c) => [c.id, c.displayIndex])).toEqual(first.changes.map((c) => [c.id, c.displayIndex]));
  });

  it("yüzdeler sıfırdan büyük toplamda tam 100 eder", () => {
    for (const counts of [[1, 1, 1], [2, 3, 5], [1, 0, 2], [7, 11, 13], [1, 1, 0]]) {
      const total = counts.reduce((sum, value) => sum + value, 0);
      const distributed = distributePercentages(counts, total);
      expect(distributed.reduce((sum, value) => sum + value, 0)).toBe(100);
    }
  });

  it("toplam sıfırken bütün oranlar sıfırdır", () => {
    expect(distributePercentages([0, 0, 0], 0)).toEqual([0, 0, 0]);
    expect(summarize([])).toMatchObject({ total: 0, addedPct: 0, removedPct: 0, modifiedPct: 0 });
  });

  it("üçe bölünen fark kümesinde kalanı en büyük paya verir", () => {
    const summary = summarize([{ kind: "added" }, { kind: "removed" }, { kind: "modified" }]);
    expect([summary.addedPct, summary.removedPct, summary.modifiedPct].reduce((a, b) => a + b)).toBe(100);
    expect(summary.addedPct).toBe(34);
  });

  it("eski ve yeni metni satırlardan türetir", () => {
    const rows: ComparisonResult["rows"] = [{
      id: "r0", kind: "modified", location: "Madde 1",
      base: { id: "b0", kind: "paragraph", text: "eski metin", order: 0 },
      revised: { id: "n0", kind: "paragraph", text: "yeni metin", order: 0 },
      baseFragments: [], revisedFragments: [],
    }];
    const model = buildComparisonViewModel(result([change({ id: "m", kind: "modified", rowIndex: 0 })], rows));
    expect(model.changes[0].leftText).toBe("eski metin");
    expect(model.changes[0].rightText).toBe("yeni metin");
  });

  it("motorun verdiği metin varsa onu tercih eder", () => {
    const model = buildComparisonViewModel(result([
      change({ id: "a", kind: "added", revisedText: "eklenen bölüm" }),
    ]));
    expect(model.changes[0].rightText).toBe("eklenen bölüm");
    expect(model.changes[0].leftText).toBeUndefined();
  });

  it("filtre toplamı değiştirmez, yalnız görünen alt kümeyi daraltır", () => {
    const model = buildComparisonViewModel(result([
      change({ id: "a", kind: "added" }),
      change({ id: "b", kind: "removed" }),
      change({ id: "c", kind: "added" }),
    ]));
    expect(filterChanges(model.changes, "all")).toHaveLength(3);
    expect(filterChanges(model.changes, "added")).toHaveLength(2);
    expect(filterChanges(model.changes, "added").map((c) => c.displayIndex)).toEqual(["01", "03"]);
    expect(model.summary.total).toBe(3);
  });

  it("görsel numara filtreden bağımsızdır", () => {
    expect(formatDisplayIndex(1)).toBe("01");
    expect(formatDisplayIndex(100)).toBe("100");
  });
});
