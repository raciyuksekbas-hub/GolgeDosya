import { unzipSync, strFromU8 } from "fflate";
import mammoth from "mammoth";
import { describe, expect, it } from "vitest";
import { buildReportDocx } from "./reportDocx";
import { buildReportBytes, reportFileName, type ReportInput } from "./report";
import { summarize, type ComparisonChange } from "./viewModels/comparisonViewModel";

const changes: ComparisonChange[] = [
  {
    id: "a", index: 1, displayIndex: "01", kind: "modified", sectionLabel: "Madde 3.1",
    summary: ['"100.000 TL" → "118.000 TL"', '− "hariç"'], rowIndices: [8],
    leftText: "3.1. Hizmet bedeli KDV hariç 100.000 TL'dir.",
    rightText: "3.1. Hizmet bedeli KDV dahil 118.000 TL'dir.",
  },
  {
    id: "b", index: 2, displayIndex: "02", kind: "added", sectionLabel: "Madde 3.3",
    summary: ["Bölüm bütünüyle eklendi."], rowIndices: [10],
    rightText: "3.3. Hizmet bedeli <sabit & değişmez> kalır.",
    structural: { articleLabel: "Madde 3.3", contentCount: 1 },
  },
  {
    id: "c", index: 3, displayIndex: "03", kind: "removed", sectionLabel: "Madde 6.3",
    summary: ["Bölüm bütünüyle silindi."], rowIndices: [19],
    leftText: "6.3. Yüklenici personelinden sorumludur.",
  },
];

const input: ReportInput = {
  baseName: "Hizmet Sözleşmesi_v3.docx",
  revisedName: "Hizmet Sözleşmesi_v4.docx",
  generatedAt: new Date(2026, 7, 21, 15, 23),
  summary: summarize(changes),
  changes,
};

describe("DOCX raporu", () => {
  it("boş olmayan bir ZIP paketi üretir ve Word parçalarını içerir", () => {
    const bytes = buildReportDocx(input);
    expect(bytes.byteLength).toBeGreaterThan(1000);
    // PK zip imzası
    expect([bytes[0], bytes[1]]).toEqual([0x50, 0x4b]);
    const files = Object.keys(unzipSync(bytes));
    expect(files).toContain("[Content_Types].xml");
    expect(files).toContain("_rels/.rels");
    expect(files).toContain("word/document.xml");
    expect(files).toContain("word/styles.xml");
    expect(files).toContain("word/_rels/document.xml.rels");
  });

  it("belge gövdesinde belge adları, özet ve bütün farklar bulunur", () => {
    const xml = strFromU8(unzipSync(buildReportDocx(input))["word/document.xml"]);
    expect(xml).toContain("Hizmet Sözleşmesi_v3.docx");
    expect(xml).toContain("Hizmet Sözleşmesi_v4.docx");
    expect(xml).toContain("21.08.2026 15:23");
    expect(xml).toContain("Toplam fark");
    for (const change of changes) {
      expect(xml).toContain(change.displayIndex);
      expect(xml).toContain(change.sectionLabel);
    }
    expect(xml).toContain(`%${input.summary.addedPct}`);
  });

  it("belge metnini XML'e kaçırarak yazar", () => {
    const xml = strFromU8(unzipSync(buildReportDocx(input))["word/document.xml"]);
    expect(xml).toContain("&lt;sabit &amp; değişmez&gt;");
    expect(xml).not.toContain("<sabit");
  });

  it("gerçek bir Word belgesi olarak açılır ve içeriği okunur", async () => {
    const bytes = buildReportDocx(input);
    // Node bağlamında mammoth `buffer` bekler; tarayıcıda `arrayBuffer` kullanılır.
    const { value, messages } = await mammoth.convertToHtml({ buffer: Buffer.from(bytes) });
    expect(messages.filter((message) => message.type === "error")).toHaveLength(0);
    expect(value).toContain("Değişikİş — Karşılaştırma Raporu");
    expect(value).toContain("Hizmet Sözleşmesi_v3.docx");
    expect(value).toContain("Madde 3.1");
    expect(value).toContain("Madde 6.3");
    expect(value).toContain("TEMEL SÜRÜM");
    expect(value).toContain("DEĞİŞİK SÜRÜM");
    expect(value).toContain("Hizmet bedeli KDV dahil 118.000 TL");
    // Başlıklar gerçek başlık stilleriyle gelir; belge düzenlenebilir yapıdadır.
    expect(value).toMatch(/<h1>|<h2>/);
  });

  it("biçim seçimi DOCX için bayt üretir, metin biçimleri bozulmaz", async () => {
    const docx = await buildReportBytes(input, "docx");
    expect([docx[0], docx[1]]).toEqual([0x50, 0x4b]);
    const html = new TextDecoder().decode(await buildReportBytes(input, "html"));
    expect(html.startsWith("<!doctype html>")).toBe(true);
    const markdown = new TextDecoder().decode(await buildReportBytes(input, "markdown"));
    expect(markdown.startsWith("# Değişikİş")).toBe(true);
    const json = JSON.parse(new TextDecoder().decode(await buildReportBytes(input, "json")));
    expect(json.summary.total).toBe(3);
  });

  it("dosya adı uzantısı doğrudur ve güvenli karakterlerden oluşur", () => {
    const name = reportFileName(input, "docx");
    expect(name).toBe("DegisikIs-Rapor-20260821-1523.docx");
    expect(name).toMatch(/^[A-Za-z0-9._-]+$/);
  });
});
