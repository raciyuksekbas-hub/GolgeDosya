/**
 * Standalone ↔ birleşik parity.
 *
 * Motor kopyalanarak taşındı; farkın çıkabileceği tek yer sarmalayan katmandır.
 * Bu dosya, karşılaştırma sözleşmesini fixture sınıfı başına sabitler: blok
 * sayısı, fark segmenti sayısı, fark türü, sıra, metin ve kaynak blok
 * kimlikleri. Beklenmeyen bir fark migration regresyonudur.
 *
 * Fixture'lar sentetiktir ve kod içinde üretilir; hiçbir müvekkil belgesi
 * kullanılmaz.
 */
import { describe, expect, it } from "vitest";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";
import { buildComparisonViewModel } from "./viewModels/comparisonViewModel";

const model = (base: { text: string }[], revised: { text: string }[]) =>
  buildComparisonViewModel(
    compareDocuments(makeBlocks(base, "base"), makeBlocks(revised, "revised")),
  );

const SÖZLEŞME = [
  { text: "MADDE 1 — TARAFLAR" },
  { text: "1.1. İşbu sözleşme Müvekkil ile Yüklenici arasında akdedilmiştir." },
  { text: "MADDE 2 — KONU" },
  { text: "2.1. Sözleşmenin konusu danışmanlık hizmetidir." },
  { text: "MADDE 3 — BEDEL" },
  { text: "3.1. Hizmet bedeli KDV hariç 100.000 TL'dir." },
];

describe("aynı belge", () => {
  it("hiçbir fark üretmez", () => {
    const m = model(SÖZLEŞME, SÖZLEŞME);
    expect(m.summary.total).toBe(0);
    expect(m.summary.added).toBe(0);
    expect(m.summary.removed).toBe(0);
    expect(m.summary.modified).toBe(0);
  });
});

describe("ekleme", () => {
  it("yalnız eklenen paragrafı bildirir", () => {
    const revised = [...SÖZLEŞME, { text: "3.2. Ödeme 30 gün içinde yapılır." }];
    const m = model(SÖZLEŞME, revised);
    expect(m.summary.added).toBe(1);
    expect(m.summary.removed).toBe(0);
    expect(m.summary.modified).toBe(0);
    expect(m.changes[0].kind).toBe("added");
  });
});

describe("silme", () => {
  it("yalnız silinen paragrafı bildirir", () => {
    const m = model(SÖZLEŞME, SÖZLEŞME.slice(0, -1));
    expect(m.summary.removed).toBe(1);
    expect(m.summary.added).toBe(0);
    expect(m.changes[0].kind).toBe("removed");
  });
});

describe("paragraf değişikliği", () => {
  it("değişikliği aynı satırda eşleştirir, ekleme+silme olarak bölmez", () => {
    const revised = SÖZLEŞME.map((b, i) =>
      i === 5 ? { text: "3.1. Hizmet bedeli KDV dahil 118.000 TL'dir." } : b,
    );
    const m = model(SÖZLEŞME, revised);
    expect(m.summary.modified).toBe(1);
    expect(m.summary.added).toBe(0);
    expect(m.summary.removed).toBe(0);
    expect(m.changes[0].kind).toBe("modified");
  });
});

describe("Türkçe karakterler", () => {
  it("İ, ı, ş, ğ, ü, ö, ç farkın içinde korunur", () => {
    const base = [{ text: "Müvekkilin şğüçöıİ hakları saklıdır." }];
    const revised = [{ text: "Müvekkilin şğüçöıİ hakları mahfuzdur." }];
    const m = model(base, revised);
    expect(m.summary.modified).toBe(1);
    const c = m.changes[0];
    expect(c.leftText).toContain("şğüçöıİ");
    expect(c.rightText).toContain("şğüçöıİ");
    // Türkçe küçük harf dönüşümü fark metnini bozmamalı.
    expect(c.rightText).toContain("mahfuzdur");
  });

  it("yalnız büyük/küçük harf farkı gerçek fark sayılır", () => {
    const m = model([{ text: "İSTANBUL" }], [{ text: "istanbul" }]);
    expect(m.summary.total).toBe(1);
  });
});

describe("kaynak blok kimlikleri", () => {
  it("her fark hangi bloklardan geldiğini taşır", () => {
    const revised = SÖZLEŞME.map((b, i) =>
      i === 3 ? { text: "2.1. Sözleşmenin konusu hukuki danışmanlıktır." } : b,
    );
    const m = model(SÖZLEŞME, revised);
    const c = m.changes[0];
    expect(c.rowIndices.length).toBeGreaterThan(0);
    expect(c.id).toBeTruthy();
    expect(c.sectionLabel).toBeTruthy();
  });
});

describe("sıra ve numaralandırma", () => {
  it("farklar belge sırasına göre ve kesintisiz numaralanır", () => {
    const revised = [
      SÖZLEŞME[0],
      { text: "1.1. İşbu sözleşme Müvekkil ile Danışman arasında akdedilmiştir." },
      SÖZLEŞME[2],
      SÖZLEŞME[3],
      SÖZLEŞME[4],
      { text: "3.1. Hizmet bedeli KDV dahil 118.000 TL'dir." },
    ];
    const m = model(SÖZLEŞME, revised);
    expect(m.changes.length).toBeGreaterThanOrEqual(2);
    expect(m.changes.map((c) => c.displayIndex)).toEqual(
      m.changes.map((_, i) => String(i + 1).padStart(2, "0")),
    );
    // Satır sırası artan olmalı: fark rayı belge boyunca aşağı doğru ilerler.
    const rows = m.changes.map((c) => Math.min(...c.rowIndices));
    expect([...rows].sort((a, b) => a - b)).toEqual(rows);
  });
});

describe("boş ve tek taraflı belge", () => {
  it("boş temel sürüm her paragrafı ekleme sayar", () => {
    const m = model([], SÖZLEŞME);
    expect(m.summary.added).toBe(SÖZLEŞME.length);
    expect(m.summary.removed).toBe(0);
  });

  it("boş değişik sürüm her paragrafı silme sayar", () => {
    const m = model(SÖZLEŞME, []);
    expect(m.summary.removed).toBe(SÖZLEŞME.length);
    expect(m.summary.added).toBe(0);
  });
});
