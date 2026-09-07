import { describe, expect, it } from "vitest";
import { strToU8, zipSync } from "fflate";
import mammoth from "mammoth";
import { DOMParser } from "@xmldom/xmldom";
import { compareDocuments } from "./compare";
import { htmlEntriesFromRoot, prepareDocxFinalView, withoutLegacyPaginationArtifacts } from "./extractors";
import { makeBlocks } from "./normalize";

function documentXml(paragraphs: string[]): string {
  const escaped = paragraphs.map((text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"));
  return `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${escaped.map((text) => `<w:p><w:r><w:t>${text}</w:t></w:r></w:p>`).join("")}</w:body></w:document>`;
}

function docx(document: string): Uint8Array {
  const contentTypes = `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`;
  const relationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
  return zipSync({
    "[Content_Types].xml": strToU8(contentTypes),
    "_rels/.rels": strToU8(relationships),
    "word/document.xml": strToU8(document),
    "word/_rels/document.xml.rels": strToU8(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>`),
  });
}

function exactArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function numberedParagraph(content: string): string {
  return `<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>${content}</w:p>`;
}

function numberedDocx(paragraphs: string[]): Uint8Array {
  const document = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>${paragraphs.map(numberedParagraph).join("")}<w:sectPr><w:headerReference w:type="default" r:id="rId2"/></w:sectPr></w:body></w:document>`;
  const numbering = `<?xml version="1.0" encoding="UTF-8"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>`;
  const header = `<?xml version="1.0" encoding="UTF-8"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>ÖRNEK ŞİRKET · Sayfa üstbilgisi</w:t></w:r></w:p></w:hdr>`;
  const contentTypes = `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>`;
  const relationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
  const documentRelationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>`;
  return zipSync({
    "[Content_Types].xml": strToU8(contentTypes),
    "_rels/.rels": strToU8(relationships),
    "word/document.xml": strToU8(document),
    "word/numbering.xml": strToU8(numbering),
    "word/header1.xml": strToU8(header),
    "word/_rels/document.xml.rels": strToU8(documentRelationships),
  });
}

function htmlParagraphs(html: string): Array<{ text: string }> {
  return Array.from(html.matchAll(/<p>(.*?)<\/p>/gu), (match) => ({
    text: match[1].replace(/<[^>]+>/gu, "").replace(/&amp;/gu, "&").replace(/&lt;/gu, "<").replace(/&gt;/gu, ">"),
  }));
}

describe("DOCX ayrıştırıcısı", () => {
  it("sayfaya bağlı arka-plan metin kutusunu çıkarır, aynı şirketin gerçek gövde kullanımını korur", async () => {
    const furniture = (line: string) => `<w:p><w:r><w:drawing><wp:anchor behindDoc="1"><wp:positionH relativeFrom="page"/><wps:txbx><w:txbxContent><w:p><w:r><w:t>${line}</w:t></w:r></w:p></w:txbxContent></wps:txbx></wp:anchor></w:drawing></w:r></w:p>`;
    const xml = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body>
      ${furniture("Rönesans Holding A.Ş.")}
      ${furniture("Adres ve Vergi No: 7350642670")}
      ${furniture("Rönesans Holding A.Ş.")}
      <w:p><w:r><w:t>ALICI Rönesans Holding A.Ş.'dir.</w:t></w:r></w:p>
    </w:body></w:document>`;
    const prepared = prepareDocxFinalView(exactArrayBuffer(docx(xml)));
    const html = (await mammoth.convertToHtml({ buffer: Buffer.from(prepared) })).value;
    const root = new DOMParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;

    expect(htmlEntriesFromRoot(root)).toEqual([{ text: "ALICI Rönesans Holding A.Ş.'dir.", kind: "paragraph" }]);
  });

  it("Word pagination field'larını çıkarır, gerçek PAGE metnini korur", async () => {
    const fields = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
      <w:p><w:r><w:t>PAGE Yazılım Hizmetleri Ltd. Şti.</w:t></w:r></w:p>
      <w:p><w:fldSimple w:instr=" PAGE "><w:r><w:t>PAGE</w:t></w:r></w:fldSimple></w:p>
      <w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>PAGE 2</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
      <w:p><w:fldSimple w:instr=" SECTIONPAGES "><w:r><w:t>3</w:t></w:r></w:fldSimple></w:p>
    </w:body></w:document>`;
    const prepared = prepareDocxFinalView(exactArrayBuffer(docx(fields)));
    const html = (await mammoth.convertToHtml({ buffer: Buffer.from(prepared) })).value;
    const root = new DOMParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;

    expect(htmlEntriesFromRoot(root)).toEqual([{ text: "PAGE Yazılım Hizmetleri Ltd. Şti.", kind: "paragraph" }]);
    expect(withoutLegacyPaginationArtifacts([
      { text: "PAGE Yazılım Hizmetleri Ltd. Şti." },
      { text: "PAGE" },
      { text: "PAGE 2" },
    ])).toEqual([{ text: "PAGE Yazılım Hizmetleri Ltd. Şti." }]);
  });

  it("Türkçe locale altında büyük LI etiketli numaralı hükümleri düşürmez", () => {
    const root = new DOMParser().parseFromString(
      "<body><ol><li>Madde 3<ol><li>3.1 Bedel 364.970,00-USD</li><li>3.2 Süre 3 (Üç) iş günüdür.</li></ol></li></ol></body>",
      "text/xml",
    ).documentElement;

    expect(htmlEntriesFromRoot(root)).toEqual([
      { text: "Madde 3", kind: "list", sourceListOrdinal: "1", listLevel: 0 },
      { text: "3.1 Bedel 364.970,00-USD", kind: "list", sourceListOrdinal: "1", listLevel: 1 },
      { text: "3.2 Süre 3 (Üç) iş günüdür.", kind: "list", sourceListOrdinal: "2", listLevel: 1 },
    ]);
  });

  it("Track Changes içeriğini kabul edilmiş nihai Word görünümüyle çıkarır", async () => {
    const revisionXml = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t xml:space="preserve">Korunan </w:t></w:r><w:del w:id="1"><w:r><w:delText xml:space="preserve">silinen </w:delText></w:r></w:del><w:ins w:id="2"><w:r><w:t xml:space="preserve">eklenen </w:t></w:r></w:ins><w:moveFrom w:id="3"><w:r><w:t xml:space="preserve">eski konum </w:t></w:r></w:moveFrom><w:moveTo w:id="4"><w:r><w:t xml:space="preserve">yeni konum </w:t></w:r></w:moveTo><w:r><w:t>metin.</w:t></w:r></w:p></w:body></w:document>`;
    const prepared = prepareDocxFinalView(exactArrayBuffer(docx(revisionXml)));
    const result = await mammoth.convertToHtml({ buffer: Buffer.from(prepared) });
    expect(result.value).toBe("<p>Korunan eklenen yeni konum metin.</p>");
  });

  it("yoğun revision içeren numaralı sentetik DOCX gövdelerini ve kritik farkları korur", async () => {
    const baseArchive = numberedDocx([
      `<w:r><w:t xml:space="preserve">3.1 Bedel </w:t></w:r><w:ins w:id="1"><w:r><w:t>343.570,00 USD</w:t></w:r></w:ins><w:r><w:t> olarak ödenir.</w:t></w:r>`,
      `<w:r><w:t xml:space="preserve">3.2 Ödeme </w:t></w:r><w:ins w:id="2"><w:r><w:t>60 (Altmış) takvim günü</w:t></w:r></w:ins><w:r><w:t> içinde yapılır.</w:t></w:r>`,
      `<w:r><w:t>3.3 Ortak ve uzun hüküm aynen korunur.</w:t></w:r>`,
    ]);
    const revisedArchive = numberedDocx([
      `<w:r><w:t xml:space="preserve">3.1 Bedel </w:t></w:r><w:del w:id="3"><w:r><w:delText>343.570,00 USD</w:delText></w:r></w:del><w:ins w:id="4"><w:r><w:t>364.970,00-USD</w:t></w:r></w:ins><w:r><w:t> olarak ödenir.</w:t></w:r>`,
      `<w:r><w:t xml:space="preserve">3.2 Ödeme </w:t></w:r><w:del w:id="5"><w:r><w:delText>60 (Altmış) takvim günü</w:delText></w:r></w:del><w:ins w:id="6"><w:r><w:t>3 (Üç) iş günü</w:t></w:r></w:ins><w:r><w:t xml:space="preserve"> içinde yapılır ve </w:t></w:r><w:ins w:id="7"><w:r><w:t>aylık %5 gecikme faizi uygulanır.</w:t></w:r></w:ins>`,
      `<w:r><w:t>3.3 Ortak ve uzun hüküm aynen korunur.</w:t></w:r>`,
      `<w:ins w:id="8"><w:r><w:t>3.4 Yeni ve tamamen anonim hüküm eklenmiştir.</w:t></w:r></w:ins>`,
    ]);
    const toEntries = async (archive: Uint8Array) => {
      const prepared = prepareDocxFinalView(exactArrayBuffer(archive));
      const html = (await mammoth.convertToHtml({ buffer: Buffer.from(prepared) })).value;
      const root = new DOMParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;
      return htmlEntriesFromRoot(root);
    };
    const baseEntries = await toEntries(baseArchive);
    const revisedEntries = await toEntries(revisedArchive);
    const result = compareDocuments(makeBlocks(baseEntries, "base"), makeBlocks(revisedEntries, "revised"));

    expect(baseEntries.some((entry) => entry.text.includes("343.570,00 USD"))).toBe(true);
    expect(revisedEntries.some((entry) => entry.text.includes("364.970,00-USD"))).toBe(true);
    expect(revisedEntries.some((entry) => entry.text.includes("3 (Üç) iş günü"))).toBe(true);
    expect(revisedEntries.some((entry) => entry.text.includes("aylık %5"))).toBe(true);
    expect(revisedEntries.some((entry) => entry.text.includes("Sayfa üstbilgisi"))).toBe(false);
    expect(result.changes.some((change) => change.summary.includes("“60 (Altmış) takvim günü” → “3 (Üç) iş günü”"))).toBe(true);
    expect(result.rows.filter((row) => row.kind !== "unchanged")).toHaveLength(3);
  });

  it("Merve ihtarname DOCX split örüntüsünü üç maddi fark olarak karşılaştırır", async () => {
    const addedSentence = "Bu durumun müvekkilin, 14 Kasım tarihi itibarıyla şirketin genel durumuna ilişkin ve şirketin ilgili mevzuat ve yönetmeliklere aykırı faaliyetlerini açık biçimde ortaya koyduğu raporunu yönetim kuruluna sunduktan sonra, mali işler departmanı tarafından çalıştığı birimin kapatılacağı yönünde bir baskıyla karşı karşıya kalması akabinde yaşanması düşündürücüdür.";
    const removedOvertime = "nihayet fazla mesai yapmasına karşın bu mesailerinin karşılığı da ödenmemiştir.";
    const commonA = "Şirket nezdinde uzun yıllardır çalışan müvekkilin görevini özenle yerine getirdiği, tüm sorumluluklarını zamanında tamamladığı ve yönetim süreçlerine eksiksiz katkı sunduğu sabittir.";
    const commonBStart = "Buna rağmen çalışma koşulları ağırlaştırılmış ve ücretleri geç ödenmiştir;";
    const commonBEnd = "İş ilişkisi boyunca müvekkil tüm yazılı bildirimleri zamanında yapmış ve yöneticilerini bilgilendirmiştir.";
    const base = await mammoth.convertToHtml({ buffer: Buffer.from(docx(documentXml([
      `${commonA} ${commonBStart} ${removedOvertime} ${commonBEnd}`,
      "Müvekkilin ücret, fazla mesai ücretlerini, yıllık izin ve sair işçilik alacaklarını talep ederiz.",
    ]))) });
    const revised = await mammoth.convertToHtml({ buffer: Buffer.from(docx(documentXml([
      `${commonA} ${addedSentence}`,
      `${commonBStart} ${commonBEnd}`,
      "Müvekkilin ücret, yıllık izin ve sair işçilik alacaklarını talep ederiz.",
    ]))) });
    const result = compareDocuments(makeBlocks(htmlParagraphs(base.value), "base"), makeBlocks(htmlParagraphs(revised.value), "revised"));

    expect(result.changes.map((change) => [change.kind, change.baseText, change.revisedText])).toEqual([
      ["added", undefined, addedSentence],
      ["removed", removedOvertime, undefined],
      ["removed", "fazla mesai ücretlerini,", undefined],
    ]);
  });
});
