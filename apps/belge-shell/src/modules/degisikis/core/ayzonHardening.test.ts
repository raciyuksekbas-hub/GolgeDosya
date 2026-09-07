import { DOMParser } from "@xmldom/xmldom";
import { strToU8, zipSync } from "fflate";
import mammoth from "mammoth";
import { describe, expect, it } from "vitest";
import { compareDocuments } from "./compare";
import { htmlEntriesFromRoot } from "./extractors";
import { makeBlocks, normalizeTechnicalNoise } from "./normalize";

function ayzonDocx(paragraphs: string[]): Uint8Array {
  const body = paragraphs.map((text) => {
    const escaped = text.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
    return `<w:p><w:r><w:t>${escaped}</w:t></w:r></w:p>`;
  }).join("");
  const document = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`;
  const contentTypes = `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`;
  const relationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
  return zipSync({
    "[Content_Types].xml": strToU8(contentTypes),
    "_rels/.rels": strToU8(relationships),
    "word/document.xml": strToU8(document),
  });
}

async function blocksFromDocx(paragraphs: string[], prefix: string) {
  const html = (await mammoth.convertToHtml({ buffer: Buffer.from(ayzonDocx(paragraphs)) })).value;
  const root = new DOMParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;
  return makeBlocks(htmlEntriesFromRoot(root), prefix);
}

function summaries(result: ReturnType<typeof compareDocuments>): string {
  return result.changes.flatMap((change) => change.summary).join(" ");
}

describe("Ayzon regression hardening", () => {
  it("form placeholder uzunluğu ve Word run bölünmesi fark üretmez", () => {
    const base = makeBlocks([
      { text: "Ad Soyad: ………………………." },
      { text: "Adres: . . . . . . . ." },
      { text: "________________" },
    ], "base");
    const revised = makeBlocks([
      { text: "Ad Soyad: …………………" },
      { text: "Adres: ..........." },
      { text: "__________" },
    ], "revised");

    expect(compareDocuments(base, revised).changes).toHaveLength(0);
  });

  it("sentetik DOCX ingestion placeholder ve EK locator davranışını korur", async () => {
    const base = await blocksFromDocx([
      "Ad Soyad: ……………………….",
      "EK-2",
      "1. Teslim Edilen Ekipman",
      "Ekipman listesi eskidir.",
    ], "base");
    const revised = await blocksFromDocx([
      "Ad Soyad: …………………",
      "EK-2",
      "1. Teslim Edilen Ekipman",
      "Ekipman listesi günceldir.",
    ], "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].location).toBe("EK-2 / 1. Teslim Edilen Ekipman");
    expect(summaries(result)).not.toMatch(/[…._]{6,}/u);
  });

  it("placeholder normalization gerçek ellipsis, tarih ve tutarı bastırmaz", () => {
    expect(normalizeTechnicalNoise("Cümle burada... devam eder.")).toBe("Cümle burada... devam eder.");
    expect(normalizeTechnicalNoise("30.04.2025 tarihinde 28.075,50 TL")).toBe("30.04.2025 tarihinde 28.075,50 TL");
    const result = compareDocuments(
      makeBlocks([{ text: "Tarih: 30.04.2025 Tutar: 28.075,50 TL" }], "base"),
      makeBlocks([{ text: "Tarih: 30.04.2026 Tutar: 29.075,50 TL" }], "revised"),
    );
    expect(result.changes).toHaveLength(1);
    expect(summaries(result)).toContain("30.04.2025");
    expect(summaries(result)).toContain("28.075,50");
  });

  it("Madde 6 içinde yakına taşınan ortak cümleleri add/delete saymaz", () => {
    const exactMoved = "Çalışana ait SGK bildirimleri, ücret bordroları ile vergi ve prim kesintileri mevzuata uygun şekilde İşveren tarafından yürütülür.";
    const oldMoved = `İkramiye, prim, masraf iadesi ve varsa diğer yan hakların niteliği de aynı şekilde bordroda ayrıştırılır. ${exactMoved}`;
    const newMoved = `Ücret, ikramiye, prim, masraf iadesi ve varsa diğer yan hakların niteliği bordroda ayrıştırılır. ${exactMoved}`;
    const result = compareDocuments(
      makeBlocks([
        { text: "MADDE 6 – ÜCRET VE YAN HAKLAR" },
        { text: "Ücret her ay banka hesabına ödenir." },
        { text: `Yol ve yemek ücretleri ayrıca karşılanır. ${oldMoved}` },
      ], "base"),
      makeBlocks([
        { text: "MADDE 6 – ÜCRET VE YAN HAKLAR" },
        { text: `Ücret her ay banka hesabına ödenir. ${newMoved}` },
      ], "revised"),
    );

    expect(result.changes.some((change) => change.kind === "removed" && change.baseText?.includes("Yol ve yemek ücretleri"))).toBe(true);
    expect(result.changes.some((change) => change.baseText?.includes(exactMoved) || change.revisedText?.includes(exactMoved))).toBe(false);
    expect(summaries(result)).not.toContain("SGK bildirimleri");
    expect(summaries(result)).toContain("Ücret");
    expect(summaries(result)).toContain("de aynı şekilde");
  });

  it("silinen paragraf tokenını komşu paragraf eklemesiyle replacement yapmaz", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "Çalışanın adını belirtmesi değerlendirilir." },
        { text: "Şirket dilediği çalışmalarda Çalışanın adını belirtebilir." },
      ], "base"),
      makeBlocks([
        { text: "Şirket dilediği çalışmalarda Çalışanın adını kredi olarak belirtebilir." },
      ], "revised"),
    );
    const text = summaries(result);

    expect(result.changes.some((change) => change.kind === "removed" && change.baseText?.includes("değerlendirilir"))).toBe(true);
    expect(result.changes.some((change) => change.kind === "added" && change.revisedText?.includes("kredi olarak"))).toBe(true);
    expect(text).not.toMatch(/değerlendirilir[^+−]*→[^+−]*(?:kredi|belirtebilir)/u);
  });

  it("Madde 12 gerçek mikro farklarını korur ve forbidden replacement üretmez", () => {
    const deleted = "Çalışanın Şirket bünyesinde ürettiği tüm Çalışma Ürünleri ve bunlara bağlı hizmetler Şirkete aittir; Çalışanın bunları üçüncü kişiye sunması ağır ihlal olarak değerlendirilir.";
    const oldName = "Çalışanın ad belirtilmesi hakkını talep etmeyeceği kabul edilir; Şirket dilediği çalışmalarda Çalışanın adını belirtebilir.";
    const newName = "Çalışanın ad belirtilmesi hakkını ısrarla talep etmeyeceği kabul edilir; Şirket dilediği çalışmalarda Çalışanın adını kredi olarak belirtebilir.";
    const result = compareDocuments(
      makeBlocks([{ text: "MADDE 12 – FİKRÎ HAKLAR" }, { text: deleted }, { text: oldName }], "base"),
      makeBlocks([{ text: "MADDE 12 – FİKRÎ HAKLAR" }, { text: newName }], "revised"),
    );
    const text = summaries(result);

    expect(result.changes.some((change) => change.kind === "removed" && change.baseText?.includes("ağır ihlal"))).toBe(true);
    expect(text).toContain("ısrarla");
    expect(text).toContain("kredi olarak");
    expect(text).not.toContain("ısrarla belirtebilir");
    expect(text).not.toMatch(/değerlendirilir[^+−]*→[^+−]*ısrarla/u);
    expect(text).not.toMatch(/değerlendirilir[^+−]*→[^+−]*belirtebilir/u);
  });

  it("EK ve alt başlıklarını global index yerine structural locator yapar", () => {
    const blocks = makeBlocks([
      { text: "EK-2" },
      { text: "1. Teslim Edilen Ekipman" },
      { text: "Ekipman listesi güncellenmiştir." },
      { text: "2. Ekipmanın Mülkiyeti" },
      { text: "Mülkiyet İşverene aittir." },
      { text: "4. Veri ve Erişim Güvenliği" },
      { text: "Erişim sınırlandırılmıştır." },
      { text: "7. Denetim Hakkı" },
      { text: "Denetim ölçülü yapılır." },
      { text: "EK-4" },
      { text: "1. Ticari Sır ve Gizli Bilgi Tanımı" },
      { text: "Gizli bilgi kapsamı açıklanmıştır." },
      { text: "EK-5" },
      { text: "5. Görüntü ve Ses Kullanım İzni" },
      { text: "İzin koşulları açıklanmıştır." },
      { text: "7. Kıyafet ve Temsil" },
      { text: "Temsil kuralları açıklanmıştır." },
      { text: "EK-6" },
      { text: "E. Gizlilik ve Rekabet Yükümlülüklerinin Devamı" },
      { text: "Yükümlülükler devam eder." },
    ]);

    expect(blocks[2].location).toBe("EK-2 / 1. Teslim Edilen Ekipman");
    expect(blocks[4].location).toBe("EK-2 / 2. Ekipmanın Mülkiyeti");
    expect(blocks[6].location).toBe("EK-2 / 4. Veri ve Erişim Güvenliği");
    expect(blocks[8].location).toBe("EK-2 / 7. Denetim Hakkı");
    expect(blocks[11].location).toBe("EK-4 / 1. Ticari Sır ve Gizli Bilgi Tanımı");
    expect(blocks[14].location).toBe("EK-5 / 5. Görüntü ve Ses Kullanım İzni");
    expect(blocks[16].location).toBe("EK-5 / 7. Kıyafet ve Temsil");
    expect(blocks[19].location).toBe("EK-6 / E. Gizlilik ve Rekabet Yükümlülüklerinin Devamı");
  });

  it("EK tablosunda nötr/heading context kullanır ve gerçek label farkını korur", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "EK-5" },
        { text: "İMZA BÖLÜMÜ" },
        { text: "AYZON DIGITAL (Şahıs İşletmesi) İşletme Sahibi | ……………………", kind: "table" },
      ], "base"),
      makeBlocks([
        { text: "EK-5" },
        { text: "İMZA BÖLÜMÜ" },
        { text: "AYZON DIGITAL Yetkili | ……………", kind: "table" },
      ], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].location).toBe("EK-5 / İmza Bölümü");
    expect(summaries(result)).toContain("Yetkili");
    expect(summaries(result)).not.toMatch(/(?:248|297)\. satır/u);
  });

  it("tek source table row değişikliğini iki kart olarak emit etmez", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "AYZON DIGITAL (Şahıs İşletmesi) İşletme Sahibi | İmza", kind: "table" }], "base"),
      makeBlocks([{ text: "AYZON DIGITAL Yetkili | İmza", kind: "table" }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(new Set(result.changes.flatMap((change) => change.rowIndices)).size).toBe(1);
  });
});
