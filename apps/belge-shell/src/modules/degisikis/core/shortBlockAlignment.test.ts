import { describe, expect, it } from "vitest";
import { makeBlocks } from "./normalize";
import { compareDocuments } from "./compare";
import type { ComparisonResult } from "./types";

const compare = (base: string[], revised: string[]): ComparisonResult => compareDocuments(
  makeBlocks(base.map((text) => ({ text })), "base"),
  makeBlocks(revised.map((text) => ({ text })), "revised"),
);

const changed = (result: ComparisonResult) =>
  result.changes.map((change) => [change.kind, change.baseText, change.revisedText]);

/** Every block a card claims on one side, taken from its recorded provenance. */
const provenanceIds = (result: ComparisonResult) =>
  result.changes.map((change) => [
    change.provenance?.baseSourceBlockIds ?? [],
    change.provenance?.revisedSourceBlockIds ?? [],
  ]);

describe("ardışık kısa bloklarda sınır aşan eşleştirme", () => {
  it("üç kısa blokta yalnız değişen bloğu kart yapar", () => {
    const result = compare(
      ["AV. Mehmet Demir", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
      ["AV. Mehmet Demirel", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
    );

    expect(changed(result)).toEqual([["modified", "AV. Mehmet Demir", "AV. Mehmet Demirel"]]);
    expect(result.rows.map((row) => row.kind)).toEqual(["modified", "unchanged", "unchanged"]);
  });

  it("dört kısa blokta yalnız ortadaki değişikliği kart yapar", () => {
    const result = compare(
      ["TARAF VEKİLİ", "AV. Mehmet Demir", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
      ["TARAF VEKİLİ", "AV. Mehmet Demirel", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
    );

    expect(changed(result)).toEqual([["modified", "AV. Mehmet Demir", "AV. Mehmet Demirel"]]);
    expect(result.rows.map((row) => row.kind)).toEqual(["unchanged", "modified", "unchanged", "unchanged"]);
  });

  it("araya eklenen kısa bloğu ekleme sayar, komşuları replacement yapmaz", () => {
    const result = compare(
      ["AV. Mehmet Demir", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
      ["AV. Mehmet Demir", "TARAF VEKİLİ", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
    );

    expect(changed(result)).toEqual([["added", undefined, "TARAF VEKİLİ"]]);
    expect(result.rows.filter((row) => row.kind !== "unchanged")).toHaveLength(1);
  });

  it("silinen kısa bloğu silme sayar, komşuları replacement yapmaz", () => {
    const result = compare(
      ["AV. Mehmet Demir", "TARAF VEKİLİ", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
      ["AV. Mehmet Demir", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
    );

    expect(changed(result)).toEqual([["removed", "TARAF VEKİLİ", undefined]]);
    expect(result.rows.filter((row) => row.kind !== "unchanged")).toHaveLength(1);
  });

  it("gerçek imza bloğunda yalnız soyadı farkını tek kart olarak verir", () => {
    const result = compare(
      ["AV. Yunus Emre EROL", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
      ["AV. Yunus Emre EROLL", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary).toEqual(["“EROL” → “EROLL”"]);
    expect(result.changes[0].baseText).toBe("AV. Yunus Emre EROL");
    expect(result.changes[0].revisedText).toBe("AV. Yunus Emre EROLL");
    // The replacement must stay inside one legitimate local pairing.
    expect(provenanceIds(result)).toEqual([[["base-0"], ["revised-0"]]]);
    expect(result.rows.map((row) => row.kind)).toEqual(["modified", "unchanged", "unchanged"]);
  });

  it("üç kısa bloğun tamamı gerçekten değiştiğinde hiçbirini değişmemiş saymaz", () => {
    const result = compare(
      ["AV. Mehmet Demir", "ARABULUCU", "ARB. AV. EDA YILMAZ"],
      ["AV. Sinan Kaya", "TARAF VEKİLİ", "ARB. AV. NUR AYDIN"],
    );

    expect(result.rows.some((row) => row.kind === "unchanged")).toBe(false);
    const rendered = JSON.stringify(changed(result));
    for (const token of ["Sinan Kaya", "TARAF VEKİLİ", "NUR AYDIN"]) {
      expect(rendered).toContain(token);
    }
    for (const token of ["Mehmet Demir", "ARABULUCU", "EDA YILMAZ"]) {
      expect(rendered).toContain(token);
    }
  });

  it("tekrar eden kısa metnin her occurrence'ını kendi provenance'ıyla korur", () => {
    const result = compare(
      ["ARABULUCU", "AV. Ali Vural", "ARABULUCU", "AV. Can Doğan"],
      ["ARABULUCU", "AV. Ali Vurall", "ARABULUCU", "AV. Can Doğan"],
    );

    expect(changed(result)).toEqual([["modified", "AV. Ali Vural", "AV. Ali Vurall"]]);
    expect(provenanceIds(result)).toEqual([[["base-1"], ["revised-1"]]]);
    // Both repeated occurrences keep their own row rather than collapsing into one.
    expect(result.rows.map((row) => [row.base?.id, row.revised?.id])).toEqual([
      ["base-0", "revised-0"],
      ["base-1", "revised-1"],
      ["base-2", "revised-2"],
      ["base-3", "revised-3"],
    ]);
  });

  it("gerçek split/merge davranışını bastırmaz", () => {
    const first = "Taraflar, işbu protokolün imzalanmasından itibaren doğacak her türlü uyuşmazlığın çözümünde İstanbul mahkemelerinin yetkili olduğunu kabul ederler.";
    const second = "Bu yetki münhasır olmayıp tarafların genel hükümlere başvurma hakkı saklıdır.";
    const result = compare([`${first} ${second}`], [first, second]);

    expect(result.changes).toHaveLength(0);
  });
});
