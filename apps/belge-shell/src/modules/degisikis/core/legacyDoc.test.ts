import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DOMParser } from "@xmldom/xmldom";
import { strToU8, zipSync } from "fflate";
import mammoth from "mammoth";
import { describe, expect, it } from "vitest";
import { compareDocuments } from "./compare";
import { detectDocumentExtension, htmlEntriesFromRoot, prepareDocxFinalView, withoutLegacyPaginationArtifacts } from "./extractors";
import { makeBlocks } from "./normalize";
import type { DocumentBlock } from "./types";

const OLE_SIGNATURE = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

function escapeXml(text: string): string {
  return text.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
}

function paragraph(text: string): string {
  return `<w:p><w:r><w:t xml:space="preserve">${escapeXml(text)}</w:t></w:r></w:p>`;
}

function table(rows: string[][]): string {
  return `<w:tbl>${rows.map((row) => `<w:tr>${row.map((cell) => `<w:tc>${paragraph(cell)}</w:tc>`).join("")}</w:tr>`).join("")}</w:tbl>`;
}

function docx(parts: string[]): Uint8Array {
  const contentTypes = `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`;
  const relationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
  const document = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${parts.join("")}</w:body></w:document>`;
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

interface LegacyFixture {
  binaryDoc: Uint8Array;
  originalDocx: Uint8Array;
  convertedDocx: Uint8Array;
}

function legacyFixture(parts: string[]): LegacyFixture {
  const directory = mkdtempSync(join(tmpdir(), "degisikis-legacy-doc-test-"));
  const originalPath = join(directory, "source.docx");
  const binaryPath = join(directory, "source.doc");
  const convertedPath = join(directory, "converted.docx");
  const originalDocx = docx(parts);
  try {
    writeFileSync(originalPath, originalDocx);
    execFileSync("/usr/bin/textutil", ["-convert", "doc", "-output", binaryPath, "--", originalPath]);
    const binaryDoc = new Uint8Array(readFileSync(binaryPath));
    expect(Array.from(binaryDoc.slice(0, 8))).toEqual(OLE_SIGNATURE);
    execFileSync("/usr/bin/textutil", ["-convert", "docx", "-output", convertedPath, "--", binaryPath]);
    return { binaryDoc, originalDocx, convertedDocx: new Uint8Array(readFileSync(convertedPath)) };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

async function blocks(bytes: Uint8Array, prefix: string): Promise<DocumentBlock[]> {
  const prepared = prepareDocxFinalView(exactArrayBuffer(bytes));
  const html = (await mammoth.convertToHtml({ buffer: Buffer.from(prepared) })).value;
  const root = new DOMParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;
  return makeBlocks(withoutLegacyPaginationArtifacts(htmlEntriesFromRoot(root)), prefix);
}

async function compareLegacy(baseParts: string[], revisedParts: string[]) {
  const base = legacyFixture(baseParts);
  const revised = legacyFixture(revisedParts);
  return compareDocuments(await blocks(base.convertedDocx, "base"), await blocks(revised.convertedDocx, "revised"));
}

describe.runIf(process.platform === "darwin")("macOS yerel eski Word DOC ingestion", () => {
  it("gerçek binary DOC page field artefaktını karşılaştırma modeline taşımaz", async () => {
    const fixture = legacyFixture([
      paragraph("PAGE Yazılım Hizmetleri Ltd. Şti."),
      `<w:p><w:fldSimple w:instr=" PAGE "><w:r><w:t>PAGE</w:t></w:r></w:fldSimple></w:p>`,
      `<w:p><w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>PAGE 2</w:t></w:r></w:fldSimple></w:p>`,
    ]);
    const converted = await blocks(fixture.convertedDocx, "doc");

    expect(converted.map((block) => block.text)).toEqual(["PAGE Yazılım Hizmetleri Ltd. Şti."]);
  });

  it("aynı iki binary DOC için 0 değişiklik üretir", async () => {
    const fixture = legacyFixture([paragraph("Taraflar aynı hükmü kabul eder.")]);
    const result = compareDocuments(await blocks(fixture.convertedDocx, "base"), await blocks(fixture.convertedDocx, "revised"));
    expect(result.changes).toHaveLength(0);
  });

  it("30 gün → 60 gün replacement'ını bulur", async () => {
    const result = await compareLegacy([paragraph("Bildirim süresi 30 gündür.")], [paragraph("Bildirim süresi 60 gündür.")]);
    expect(result.changes[0].summary).toContain("“30” → “60”");
  });

  it("para replacement'ını güvenilir biçimde bulur", async () => {
    const result = await compareLegacy([paragraph("Bedel 25.000,00 TL'dir.")], [paragraph("Bedel 32.500,00 TL'dir.")]);
    expect(result.changes[0].summary).toContain("“25.000,00 TL” → “32.500,00 TL”");
  });

  it("yalnız eklenen cümleyi gösterir", async () => {
    const result = await compareLegacy(
      [paragraph("Birinci cümle. Son cümle.")],
      [paragraph("Birinci cümle. Yeni hüküm eklenmiştir. Son cümle.")],
    );
    expect(result.changes[0].kind).toBe("added");
    expect(result.changes[0].summary).toEqual(["+ “Yeni hüküm eklenmiştir.”"]);
  });

  it("yalnız silinen cümleyi gösterir", async () => {
    const result = await compareLegacy(
      [paragraph("Birinci cümle. Kaldırılacak hüküm. Son cümle.")],
      [paragraph("Birinci cümle. Son cümle.")],
    );
    expect(result.changes[0].kind).toBe("removed");
    expect(result.changes[0].summary).toEqual(["− “Kaldırılacak hüküm.”"]);
  });

  it("aynı metnin DOC ↔ DOCX karşılaştırmasında 0 değişiklik üretir", async () => {
    const fixture = legacyFixture([paragraph("Aynı biçimden bağımsız metin."), paragraph("İkinci paragraf korunur.")]);
    const result = compareDocuments(await blocks(fixture.convertedDocx, "doc"), await blocks(fixture.originalDocx, "docx"));
    expect(result.changes).toHaveLength(0);
  });

  it("DOC ↔ DOCX arasında yalnız gerçek değişikliği bulur", async () => {
    const legacy = legacyFixture([paragraph("Teslim tarihi 01/09/2026 olacaktır.")]);
    const modern = docx([paragraph("Teslim tarihi 15/09/2026 olacaktır.")]);
    const result = compareDocuments(await blocks(legacy.convertedDocx, "doc"), await blocks(modern, "docx"));
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary).toContain("“01/09/2026” → “15/09/2026”");
  });

  it("DOC dönüşümünden sonra paragraf split/merge davranışını korur", async () => {
    const result = await compareLegacy(
      [paragraph("Birinci cümle. İkinci cümle. Üçüncü cümle.")],
      [paragraph("Birinci cümle."), paragraph("İkinci cümle. Üçüncü cümle.")],
    );
    expect(result.changes).toHaveLength(0);
  });

  it("binary DOC tablosundaki hücre değişikliğini bulur", async () => {
    const result = await compareLegacy(
      [table([["Yükümlülük", "30 gün"]])],
      [table([["Yükümlülük", "60 gün"]])],
    );
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary).toContain("“30” → “60”");
  });

  it("Türkçe karakterleri ve ASCII format identifier normalizasyonunu korur", async () => {
    const text = "ç ğ ı İ ö ş ü — İŞ İLİŞKİSİ";
    const fixture = legacyFixture([paragraph(text)]);
    const convertedBlocks = await blocks(fixture.convertedDocx, "doc");
    expect(convertedBlocks[0].text).toContain(text);
    expect(detectDocumentExtension("ÖRNEK.DOC", "application/msword", exactArrayBuffer(fixture.binaryDoc))).toBe("doc");
    expect(detectDocumentExtension("ÖRNEK.DOCX", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", exactArrayBuffer(fixture.originalDocx))).toBe("docx");
  });

  it("yanlış uzantılı veya OLE imzası olmayan DOC dosyasını fail-closed reddeder", () => {
    const fakeDoc = exactArrayBuffer(new Uint8Array([0x50, 0x4b, 0x03, 0x04, 1, 2, 3]));
    expect(() => detectDocumentExtension("yanlış.DOC", "application/msword", fakeDoc)).toThrow(
      "Bu Word belgesi okunamadı. Dosya bozuk veya desteklenmeyen bir eski Word biçiminde olabilir.",
    );
  });
});
