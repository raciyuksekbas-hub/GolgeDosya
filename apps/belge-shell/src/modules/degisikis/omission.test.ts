// Görünür metnin karşılaştırmadan DÜŞMEMESİ.
//
// SAHA BULGUSU: iki gerçek Word sözleşmesi karşılaştırıldığında çok az fark
// çıktı. Kök nedenlerden biri burada: `makeBlocks` tamamı 1–4 haneli sayı
// olan her bloğu SESSİZCE siliyordu. Sözleşmede bu bloklar tutar, yıl ve
// madde numarası taşıyan tablo hücreleridir. Bedel 1500'den 1800'e çekilse
// Karşılaştır hiçbir fark göstermiyordu — paranın yaşadığı yerde sessiz bir
// yanlış negatif.
//
// Bastırma PDF'te meşrudur (orada tek başına duran sayı sayfa numarasıdır).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { makeBlocks } from "./core/normalize";
import { compareDocuments } from "./core/compare";
import { buildComparisonViewModel } from "./viewModels/comparisonViewModel";

const rows = (texts: string[]) => texts.map((text) => ({ text }));

describe("sayı taşıyan blok düşmez", () => {
  it("DOCX'te tutar, yıl ve madde numarası korunur", () => {
    const source = ["MADDE 5 — BEDEL", "2026", "1500", "12", "Toplam bedel"];
    const blocks = makeBlocks(rows(source), "docx");
    expect(blocks).toHaveLength(source.length);
    expect(blocks.map((b) => b.text)).toEqual(source);
  });

  it("UDF'te de korunur", () => {
    expect(makeBlocks(rows(["2026", "1500"]), "udf")).toHaveLength(2);
  });

  it("boş blok yine atılır", () => {
    expect(makeBlocks(rows(["", "   ", "Madde"]), "docx")).toHaveLength(1);
  });

  it("PDF'te sayfa numarası bastırması KORUNUR", () => {
    // Burada tek başına duran sayı gerçekten yapı artığıdır.
    expect(makeBlocks(rows(["Sözleşme metni", "12"]), "pdf")).toHaveLength(1);
  });
});

describe("bedel değişikliği görülür", () => {
  // Asıl sözleşme: bir hukukçu tutarın değiştiğini GÖRMELİ.
  const base = makeBlocks(rows(["MADDE 5 — BEDEL", "1500", "Türk Lirası"]), "docx");
  const revised = makeBlocks(rows(["MADDE 5 — BEDEL", "1800", "Türk Lirası"]), "docx");
  const model = buildComparisonViewModel(compareDocuments(base, revised));

  it("fark üretilir", () => {
    expect(model.summary.total).toBeGreaterThan(0);
  });

  it("eski ve yeni tutar farkta görünür", () => {
    const text = JSON.stringify(model.changes);
    expect(text).toContain("1500");
    expect(text).toContain("1800");
  });
});

describe("uyarı dili gerçeği söyler", () => {
  // SAHA: tamamı okunmuş iki sözleşmede kullanıcı "Belgenin bir bölümü
  // okunamadı ve karşılaştırmaya girmedi" uyarısını gördü, ardına ham stil
  // adları döküldü (Gövde, Normal (Web), List Paragraph, Table Paragraph).
  // Hiçbiri kayıp değildi: mammoth tanımadığı stildeki paragrafı düz <p>
  // olarak üretir. Uyarı yanlış alarmdı, üstelik parser jargonuydu.
  const source = readFileSync(
    resolve(dirname(fileURLToPath(import.meta.url)), "CompareWorkspace.tsx"),
    "utf8",
  );
  const extractors = readFileSync(
    resolve(dirname(fileURLToPath(import.meta.url)), "core/extractors.ts"),
    "utf8",
  );

  it("stil tanımama mesajı kayıp kanalına girmez", () => {
    expect(extractors).toContain("FORMAT_ONLY_MESSAGE");
    expect(extractors).toMatch(/warnings: messages\.filter\(\(m\) => !FORMAT_ONLY_MESSAGE/);
    expect(extractors).toMatch(/notes: messages\.filter\(\(m\) => FORMAT_ONLY_MESSAGE/);
  });

  it("eski .doc bilgisi uyarı değil not", () => {
    // Kusursuz okunan her `.doc` "bir bölümü okunamadı" bannerını
    // tetikliyordu.
    const idx = extractors.indexOf("Eski Word biçimi yerel olarak");
    expect(idx).toBeGreaterThan(-1);
    expect(extractors.slice(idx - 120, idx)).toContain("notes:");
  });

  it("ham ayrıştırıcı metni ekrana basılmaz", () => {
    expect(source).not.toContain("{extractionWarnings.join(");
    expect(source).not.toContain("{simplifications.join(");
  });

  it("sadeleştirme kendi dürüst cümlesiyle söylenir", () => {
    expect(source).toContain("sadeleştirildi; metin");
    expect(source).toContain("karşılaştırmaya dahil edildi");
  });

  it("teşhis geliştirme günlüğünde korunur", () => {
    expect(source).toContain('logFailure("degisikis extraction messages"');
  });
});
