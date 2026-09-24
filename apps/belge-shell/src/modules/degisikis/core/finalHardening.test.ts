import { DOMParser } from "@xmldom/xmldom";
import { strToU8, zipSync } from "fflate";
import mammoth from "mammoth";
import { describe, expect, it } from "vitest";
import { compareDocuments } from "./compare";
import { htmlEntriesFromRoot } from "./extractors";
import { makeBlocks } from "./normalize";

function hardeningDocx(paragraphs: Array<{ text: string; numbered?: boolean }>): Uint8Array {
  const xmlParagraphs = paragraphs.map(({ text, numbered }) => {
    const escaped = text.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
    const numPr = numbered
      ? '<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>'
      : "";
    return `<w:p>${numPr}<w:r><w:t>${escaped}</w:t></w:r></w:p>`;
  }).join("");
  const document = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${xmlParagraphs}</w:body></w:document>`;
  const numbering = `<?xml version="1.0" encoding="UTF-8"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>`;
  const contentTypes = `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>`;
  const rootRelationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
  const documentRelationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>`;
  return zipSync({
    "[Content_Types].xml": strToU8(contentTypes),
    "_rels/.rels": strToU8(rootRelationships),
    "word/document.xml": strToU8(document),
    "word/numbering.xml": strToU8(numbering),
    "word/_rels/document.xml.rels": strToU8(documentRelationships),
  });
}

async function entriesFromDocx(paragraphs: Array<{ text: string; numbered?: boolean }>) {
  const html = (await mammoth.convertToHtml({ buffer: Buffer.from(hardeningDocx(paragraphs)) })).value;
  const root = new DOMParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;
  return htmlEntriesFromRoot(root);
}

describe("v0.3.0 öncesi son regression hardening", () => {
  it("sentetik KODAR DOCX'te long → short yalnız devam silinmesi üretir", async () => {
    const common = "Depozito bedeli 2 (iki) kira bedeli ‘dir.";
    const continuation = Array.from({ length: 12 }, (_, index) => `Nakit depozitonun ${index + 1}. güvence koşulu eksiksiz uygulanır.`).join(" ");
    const base = makeBlocks(await entriesFromDocx([
      { text: "MADDE 5 – MALİ YÜKÜMLÜLÜKLER" },
      { text: `${common} ${continuation}`, numbered: true },
    ]), "base");
    const revised = makeBlocks(await entriesFromDocx([
      { text: "MADDE 5 – MALİ YÜKÜMLÜLÜKLER" },
      { text: common, numbered: true },
    ]), "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0]).toMatchObject({ kind: "removed", location: "Madde 5/1" });
    expect(result.changes[0].baseText).not.toContain(common);
  });

  it("sentetik KODAR DOCX'te tek bullet paragraf ↔ üç Word list item sıfır farktır", async () => {
    const parts = ["Deprem ve doğal afetler", "Genel salgın hastalık", "Sivil ayaklanmalar"];
    const base = makeBlocks(await entriesFromDocx([
      { text: "MADDE 11 – MÜCBİR SEBEP" },
      { text: `● ${parts.join(" ● ")}` },
    ]), "base");
    const revised = makeBlocks(await entriesFromDocx([
      { text: "MADDE 11 – MÜCBİR SEBEP" },
      ...parts.map((text) => ({ text, numbered: true })),
    ]), "revised");

    expect(compareDocuments(base, revised).changes).toHaveLength(0);
  });

  it("aynı bentte long → short ortak prefix'i add/delete yapmaz", () => {
    const common = "Depozito bedeli 2 (iki) kira bedeli ‘dir.";
    const continuation = Array.from({ length: 12 }, (_, index) => (
      `Kiracı tarafından Kiraya Veren'e ödenen nakit depozitonun ${index + 1}. güvence koşulu sözleşme boyunca eksiksiz uygulanır.`
    )).join(" ");
    const result = compareDocuments(
      makeBlocks([
        { text: "MADDE 5 – MALİ YÜKÜMLÜLÜKLER" },
        { text: `${common} ${continuation}`, kind: "list", sourceListOrdinal: "2" },
      ], "base"),
      makeBlocks([
        { text: "MADDE 5 – MALİ YÜKÜMLÜLÜKLER" },
        { text: common, kind: "list", sourceListOrdinal: "2" },
      ], "revised"),
    );

    expect(result.changes.some((change) => change.kind === "added" && change.revisedText?.includes(common))).toBe(false);
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0]).toMatchObject({ kind: "removed", location: "Madde 5/2" });
    expect(result.changes[0].baseText).not.toContain(common);
    expect(result.changes[0].baseText).toContain("güvence koşulu");
  });

  it("aynı içeriğin 1 ↔ 3 adjacent liste bölünmesini fark saymaz", () => {
    const single = "● Deprem ve doğal afetler ● Genel salgın hastalık ● Sivil ayaklanmalar";
    const split = ["Deprem ve doğal afetler", "Genel salgın hastalık", "Sivil ayaklanmalar"];
    const result = compareDocuments(
      makeBlocks([
        { text: "MADDE 11 – MÜCBİR SEBEP" },
        { text: single, kind: "list", sourceListOrdinal: "1" },
      ], "base"),
      makeBlocks([
        { text: "MADDE 11 – MÜCBİR SEBEP" },
        ...split.map((text, index) => ({ text, kind: "list" as const, sourceListOrdinal: `${index + 1}` })),
      ], "revised"),
    );

    expect(result.changes).toHaveLength(0);
  });

  it("aynı içeriğin 3 ↔ 1 ters birleşmesini de fark saymaz", () => {
    const split = ["Deprem ve doğal afetler", "Genel salgın hastalık", "Sivil ayaklanmalar"];
    const single = "● Deprem ve doğal afetler ● Genel salgın hastalık ● Sivil ayaklanmalar";
    const result = compareDocuments(
      makeBlocks(split.map((text, index) => ({ text, kind: "list", sourceListOrdinal: `${index + 1}` })), "base"),
      makeBlocks([{ text: single, kind: "list", sourceListOrdinal: "1" }], "revised"),
    );

    expect(result.changes).toHaveLength(0);
  });

  it("explicit clause numarasını global/local paragraf sayısından önce gösterir", () => {
    const blocks = makeBlocks([
      { text: "MADDE 6 – YÜKÜMLÜLÜKLER" },
      { text: "6.2 Şirket gerekli bütün tedbirleri alır." },
      { text: "6.13. Yeni fiyat politikası taraflarca uygulanır." },
      { text: "6.15 Ruhsat ve izinler zamanında alınır." },
    ]);

    expect(blocks.slice(1).map((block) => block.location)).toEqual(["Madde 6.2", "Madde 6.13", "Madde 6.15"]);
  });

  it("explicit alt bent altında Word list ordinal'ini parent/child olarak gösterir", () => {
    const blocks = makeBlocks([
      { text: "MADDE 5 – MALİ YÜKÜMLÜLÜKLER" },
      { text: "5.1. Genel hükümler" },
      { text: "Birinci güvence", kind: "list", sourceListOrdinal: "1" },
      { text: "İkinci güvence", kind: "list", sourceListOrdinal: "2" },
    ]);

    expect(blocks.at(-1)?.location).toBe("Madde 5.1/2");
  });

  it("tek seviyeli explicit maddeyi korur, tarih ve tutarı clause sanmaz", () => {
    const blocks = makeBlocks([
      { text: "1. Birinci hüküm uygulanır." },
      { text: "31.12.2025 tarihinde ödeme yapılır." },
      { text: "342,00 TL tahsil edilir." },
      { text: "23.400 TL tahsil edilir." },
      { text: "192.168.1.1 adresi kullanılmaz." },
      { text: "2 (iki) yıl geçerlidir." },
    ]);

    expect(blocks[0]).toMatchObject({ sourceClauseNumber: "1", location: "Madde 1" });
    expect(blocks.slice(1).every((block) => block.sourceClauseNumber === undefined)).toBe(true);
  });

  it("explicit numarası olmayan paragraph için pseudo-source ordinal üretmez", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "İlk metin korunur." }, { text: "Eski adsız paragraf." }], "base"),
      makeBlocks([{ text: "İlk metin korunur." }, { text: "Yeni adsız paragraf." }], "revised"),
    );

    expect(result.changes[0].location).toBe("Değişiklik içeren paragraf");
  });

  it("Word run kaynaklı sentence whitespace farkı için boş card üretmez", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Taraf yükümlülüğünü yerine getirmeyi taahhüt etmiştir.Gizli bilgileri korur." }], "base"),
      makeBlocks([{ text: "Taraf yükümlülüğünü yerine getirmeyi taahhüt etmiştir. Gizli bilgileri korur." }], "revised"),
    );

    expect(result.changes).toHaveLength(0);
  });

  it("13 Madde → 13 ana madde ve 7 → 8 segmentlerinin ikisini de muhasebeleştirir", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Sözleşme 01.01.2025 tarihinde 13 Madde, 7 Sayfa olarak düzenlenmiştir." }], "base"),
      makeBlocks([{ text: "Sözleşme 02.02.2026 tarihinde 13 ana madde, 8 Sayfa olarak düzenlenmiştir." }], "revised"),
    );
    const change = result.changes[0];

    expect(change.summary.join(" ")).toContain("ana");
    expect(change.summary.join(" ")).toContain("7");
    expect(change.summary.join(" ")).toContain("8");
    expect(change.presentation).toBeDefined();
    expect(new Set([
      ...change.presentation!.visibleSegmentIds,
      ...change.presentation!.collapsedSegmentIds,
    ]).size).toBe(change.presentation!.segmentCount);
  });

  it("Çukurova clause etiketlerini ve eklenen 6.13/6.15 hükümlerini tam korur", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "6.2 Projenin adı Çukurova Towers olarak uygulanır." },
        { text: "8.4 Taraf gizli bilgileri korumayı taahhüt etmiştir.Gizli bilgiler açıklanamaz." },
      ], "base"),
      makeBlocks([
        { text: "6.2 Projenin adı Çukurova Tower olarak uygulanır." },
        { text: "6.13 Yeni fiyat politikası, satış bedelleri ve ödeme planı tarafların yazılı onayıyla eksiksiz uygulanır." },
        { text: "6.15 Ruhsat, izin ve resmî kurum onayları yükümlü tarafça süresinde ve eksiksiz alınır." },
        { text: "8.4 Taraf gizli bilgileri korumayı taahhüt etmiştir. Gizli bilgiler açıklanamaz." },
      ], "revised"),
    );

    expect(result.changes.some((change) => change.location === "Madde 6.2" && change.summary.join(" ").includes("Tower"))).toBe(true);
    expect(result.changes).toContainEqual(expect.objectContaining({
      kind: "added",
      location: "Madde 6.13",
      revisedText: expect.stringContaining("Yeni fiyat politikası"),
    }));
    expect(result.changes).toContainEqual(expect.objectContaining({
      kind: "added",
      location: "Madde 6.15",
      revisedText: expect.stringContaining("Ruhsat, izin"),
    }));
    expect(result.changes.some((change) => change.location === "Madde 8.4")).toBe(false);
  });

  it("KODAR kira parent/list etiketlerinde global ordinal üretmez", () => {
    const clauses = ["5.1", "5.2", "5.3", "5.4", "7.1", "7.2", "7.3"];
    const entries = clauses.flatMap((clause) => [
      { text: `${clause}. Genel hükümler` },
      { text: "Altındaki numaralı hüküm", kind: "list" as const, sourceListOrdinal: clause === "7.1" ? "23" : "2" },
    ]);
    const locations = makeBlocks(entries).map((block) => block.location);

    expect(locations).toContain("Madde 5.1/2");
    expect(locations).toContain("Madde 5.2/2");
    expect(locations).toContain("Madde 7.1/23");
    expect(locations).toContain("Madde 7.3/2");
    expect(locations.some((location) => /\b(?:65|104|180|194)\. (?:liste öğesi|paragraf)/u.test(location ?? ""))).toBe(false);
  });

  it("KODAR danışmanlık explicit clause değişikliklerini kendi numarasıyla korur", () => {
    const clauseNumbers = ["3.1.3", "3.1.5", "3.2.2", "4.1", "4.3", "6.1", "6.2", "6.3", "6.4", "8.2", "8.4"];
    const base = makeBlocks(clauseNumbers.map((number) => ({ text: `${number} Danışman yükümlülüğü eski haliyle uygulanır.` })), "base");
    const revised = makeBlocks(clauseNumbers.map((number) => ({ text: `${number} Danışman yükümlülüğü doğrudan ve yeni haliyle uygulanır.` })), "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(clauseNumbers.length);
    expect(result.changes.map((change) => change.location)).toEqual(clauseNumbers.map((number) => `Madde ${number}`));
    expect(result.changes.every((change) => change.summary.join(" ").includes("doğrudan"))).toBe(true);
  });

  it("non-contiguous mikro eklemeleri sentetik tek cümleye birleştirmez", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Haklar korunur, bilgiler açıklanır, eser tescil edilir ve kayıt yayımlanır." }], "base"),
      makeBlocks([{ text: "Fikrî Haklar korunur, gizli bilgiler açıklanır, eser ayrıca tescil edilir ve kayıt kamuya yayımlanır." }], "revised"),
    );
    const summary = result.changes[0].summary;

    expect(summary.some((line) => line.includes("Fikrî gizli ayrıca kamuya"))).toBe(false);
    expect(result.changes[0].presentation?.segmentCount).toBeGreaterThanOrEqual(4);
    expect(summary.some((line) => /değişiklik daha$/u.test(line))).toBe(true);
  });

  it("gerçek kelime birleşmesi ve liste içeriği eklenmesini bastırmaz", () => {
    const joinedWords = compareDocuments(
      makeBlocks([{ text: "Bu bir şey değildir." }], "base"),
      makeBlocks([{ text: "Bu birşey değildir." }], "revised"),
    );
    const changedList = compareDocuments(
      makeBlocks([{ text: "Genel salgın hastalık", kind: "list", sourceListOrdinal: "1" }], "base"),
      makeBlocks([{ text: "Genel salgın hastalık ve savaş", kind: "list", sourceListOrdinal: "1" }], "revised"),
    );

    expect(joinedWords.changes.length).toBeGreaterThan(0);
    expect(changedList.changes.some((change) => change.summary.join(" ").includes("savaş"))).toBe(true);
  });
});
