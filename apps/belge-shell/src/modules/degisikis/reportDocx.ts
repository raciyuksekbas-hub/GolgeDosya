import { strToU8, zipSync } from "fflate";
import type { ComparisonChange } from "./viewModels/comparisonViewModel";
import { formatTimestamp, type ReportInput } from "./report";

/**
 * Word raporu doğrudan OOXML olarak üretilir ve mevcut `fflate` bağımlılığıyla
 * paketlenir. Harici dönüştürücü, Word/LibreOffice kurulumu veya ağ servisi
 * kullanılmaz; çıktı tamamen yerel olarak oluşur.
 *
 * Belge sade stillerle kurulur (Başlık, Başlık 2, Başlık 3, Normal) ki kullanıcı
 * Word'de açtığında normal bir belge gibi düzenleyebilsin.
 */

const KIND_LABEL: Record<ComparisonChange["kind"], string> = {
  added: "Eklenen",
  removed: "Silinen",
  modified: "Değiştirilen",
};

const KIND_COLOR: Record<ComparisonChange["kind"], string> = {
  added: "2A7F8E",
  removed: "E06A5C",
  modified: "5A646E",
};

const INK = "111417";
const MUTED = "6B7379";
const LINE = "D4D7DB";
const WASH_REMOVED = "FDF0EE";
const WASH_ADDED = "ECF5F7";
/** A4 genişliği eksi kenar boşlukları (twip). */
const CONTENT_WIDTH = 9638;

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface RunOptions { bold?: boolean; color?: string; size?: number; mono?: boolean; italic?: boolean }

function run(text: string, options: RunOptions = {}): string {
  const properties = [
    options.mono ? '<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/>' : "",
    options.bold ? "<w:b/>" : "",
    options.italic ? "<w:i/>" : "",
    options.color ? `<w:color w:val="${options.color}"/>` : "",
    options.size ? `<w:sz w:val="${options.size * 2}"/><w:szCs w:val="${options.size * 2}"/>` : "",
  ].join("");
  // Satır sonları Word'de gerçek satır sonu olur; metin bütünlüğü korunur.
  const body = escapeXml(text)
    .split("\n")
    .map((line) => `<w:t xml:space="preserve">${line}</w:t>`)
    .join("<w:br/>");
  return `<w:r>${properties ? `<w:rPr>${properties}</w:rPr>` : ""}${body}</w:r>`;
}

interface ParagraphOptions { style?: string; spaceBefore?: number; spaceAfter?: number; indent?: number; keepNext?: boolean }

function paragraph(runs: string, options: ParagraphOptions = {}): string {
  const properties = [
    options.style ? `<w:pStyle w:val="${options.style}"/>` : "",
    options.keepNext ? "<w:keepNext/>" : "",
    options.indent ? `<w:ind w:left="${options.indent}"/>` : "",
    options.spaceBefore !== undefined || options.spaceAfter !== undefined
      ? `<w:spacing${options.spaceBefore !== undefined ? ` w:before="${options.spaceBefore}"` : ""}${options.spaceAfter !== undefined ? ` w:after="${options.spaceAfter}"` : ""}/>`
      : "",
  ].join("");
  return `<w:p>${properties ? `<w:pPr>${properties}</w:pPr>` : ""}${runs}</w:p>`;
}

function cell(width: number, content: string, fill?: string): string {
  return `<w:tc><w:tcPr><w:tcW w:w="${width}" w:type="dxa"/>${fill ? `<w:shd w:val="clear" w:color="auto" w:fill="${fill}"/>` : ""}</w:tcPr>${content}</w:tc>`;
}

function table(widths: number[], rows: string[]): string {
  const borders = ["top", "left", "bottom", "right", "insideH", "insideV"]
    .map((edge) => `<w:${edge} w:val="single" w:sz="4" w:space="0" w:color="${LINE}"/>`)
    .join("");
  // w:tblPr içindeki öğe sırası şemayla sabittir: tblW → tblBorders → tblLayout
  // → tblCellMar. Sıra bozulursa okuyucular bildirilen genişlikleri yok sayar ve
  // sütunları içeriğe göre daraltır.
  return `<w:tbl><w:tblPr><w:tblW w:w="${widths.reduce((sum, value) => sum + value, 0)}" w:type="dxa"/>` +
    `<w:tblBorders>${borders}</w:tblBorders>` +
    '<w:tblLayout w:type="fixed"/>' +
    `<w:tblCellMar><w:top w:w="80" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="80" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar>` +
    `</w:tblPr><w:tblGrid>${widths.map((width) => `<w:gridCol w:w="${width}"/>`).join("")}</w:tblGrid>` +
    rows.join("") + "</w:tbl>";
}

function summaryTable(input: ReportInput): string {
  const widths = [Math.round(CONTENT_WIDTH * 0.46), Math.round(CONTENT_WIDTH * 0.27), CONTENT_WIDTH - Math.round(CONTENT_WIDTH * 0.46) - Math.round(CONTENT_WIDTH * 0.27)];
  const header = `<w:tr><w:trPr><w:tblHeader/></w:trPr>` +
    [["Tür", ""], ["Adet", ""], ["Oran", ""]].map(([label], index) =>
      cell(widths[index], paragraph(run(label, { bold: true, color: MUTED, size: 9 }), { spaceAfter: 0 }), "F3F4F2"),
    ).join("") + "</w:tr>";
  const { summary } = input;
  const rows: Array<[string, number, string, string | undefined]> = [
    ["Toplam fark", summary.total, "%100", INK],
    ["Ekleme", summary.added, `%${summary.addedPct}`, KIND_COLOR.added],
    ["Silinen", summary.removed, `%${summary.removedPct}`, KIND_COLOR.removed],
    ["Değiştirilen", summary.modified, `%${summary.modifiedPct}`, KIND_COLOR.modified],
  ];
  const body = rows.map(([label, count, ratio, color]) =>
    `<w:tr>${cell(widths[0], paragraph(run(label, { color, bold: label === "Toplam fark" }), { spaceAfter: 0 }))}` +
    `${cell(widths[1], paragraph(run(String(count), { bold: true }), { spaceAfter: 0 }))}` +
    `${cell(widths[2], paragraph(run(ratio, { color: MUTED }), { spaceAfter: 0 }))}</w:tr>`,
  );
  return table(widths, [header, ...body]);
}

function changeBlock(change: ComparisonChange): string {
  const half = Math.round(CONTENT_WIDTH / 2);
  const widths = [half, CONTENT_WIDTH - half];
  const heading = paragraph(
    run(`${change.displayIndex}  `, { bold: true, color: MUTED }) +
    run(KIND_LABEL[change.kind], { bold: true, color: KIND_COLOR[change.kind] }) +
    run(`  ·  ${change.sectionLabel}`, { color: MUTED }),
    { style: "Baslik3", keepNext: true, spaceBefore: 240, spaceAfter: 60 },
  );
  const structural = change.structural
    ? paragraph(run(`${change.structural.articleLabel} bütünüyle ${change.kind === "added" ? "eklendi" : "silindi"}.`, { color: MUTED, size: 9 }), { spaceAfter: 60 })
    : "";
  const summaryLines = change.summary
    .map((line) => paragraph(run(`•  ${line}`, { mono: true, size: 9 }), { indent: 200, spaceAfter: 20 }))
    .join("");
  const header = `<w:tr>` +
    cell(widths[0], paragraph(run("TEMEL SÜRÜM", { bold: true, color: MUTED, size: 8 }), { spaceAfter: 0 }), WASH_REMOVED) +
    cell(widths[1], paragraph(run("DEĞİŞİK SÜRÜM", { bold: true, color: MUTED, size: 8 }), { spaceAfter: 0 }), WASH_ADDED) +
    `</w:tr>`;
  const texts = `<w:tr>` +
    cell(widths[0], paragraph(run(change.leftText ?? "—", { size: 9.5 }), { spaceAfter: 0 }), WASH_REMOVED) +
    cell(widths[1], paragraph(run(change.rightText ?? "—", { size: 9.5 }), { spaceAfter: 0 }), WASH_ADDED) +
    `</w:tr>`;
  return heading + structural + summaryLines + table(widths, [header, texts]) + paragraph("", { spaceAfter: 0 });
}

function documentXml(input: ReportInput): string {
  const widths = [Math.round(CONTENT_WIDTH * 0.3), CONTENT_WIDTH - Math.round(CONTENT_WIDTH * 0.3)];
  const files = table(widths, [
    `<w:tr>${cell(widths[0], paragraph(run("TEMEL SÜRÜM", { bold: true, color: MUTED, size: 8 }), { spaceAfter: 0 }))}${cell(widths[1], paragraph(run(input.baseName, { bold: true }), { spaceAfter: 0 }))}</w:tr>`,
    `<w:tr>${cell(widths[0], paragraph(run("DEĞİŞİK SÜRÜM", { bold: true, color: MUTED, size: 8 }), { spaceAfter: 0 }))}${cell(widths[1], paragraph(run(input.revisedName, { bold: true }), { spaceAfter: 0 }))}</w:tr>`,
  ]);

  const body = [
    paragraph(run("GölgeDosya — Karşılaştırma Raporu"), { style: "Baslik1", spaceAfter: 40 }),
    paragraph(
      run("İki belge, bütün farklar.", { color: MUTED, size: 9.5 }) +
      run(`     ${formatTimestamp(input.generatedAt)}`, { color: MUTED, size: 9.5 }),
      { spaceAfter: 220 },
    ),
    files,
    paragraph("", { spaceAfter: 0 }),
    paragraph(run("Özet"), { style: "Baslik2", spaceBefore: 200, spaceAfter: 80 }),
    summaryTable(input),
    paragraph("", { spaceAfter: 0 }),
    paragraph(run("Fark Listesi"), { style: "Baslik2", spaceBefore: 240, spaceAfter: 40 }),
    input.changes.length
      ? input.changes.map(changeBlock).join("")
      : paragraph(run("Fark bulunamadı.", { color: MUTED })),
    paragraph(
      run("GölgeDosya ile yerel olarak üretilmiştir. Karşılaştırma sonuçları yardımcı niteliktedir; nihai kontrol kullanıcıya aittir.", { color: MUTED, size: 8.5 }),
      { spaceBefore: 320 },
    ),
  ].join("");

  return '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
    '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">' +
    `<w:body>${body}` +
    '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>' +
    '<w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/>' +
    "</w:sectPr></w:body></w:document>";
}

const STYLES_XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">' +
  '<w:docDefaults><w:rPrDefault><w:rPr>' +
  '<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/>' +
  `<w:color w:val="${INK}"/><w:sz w:val="21"/><w:szCs w:val="21"/>` +
  '</w:rPr></w:rPrDefault>' +
  '<w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault>' +
  "</w:docDefaults>" +
  '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>' +
  `<w:style w:type="paragraph" w:styleId="Baslik1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:outlineLvl w:val="0"/><w:spacing w:before="0" w:after="60"/></w:pPr><w:rPr><w:b/><w:sz w:val="34"/><w:szCs w:val="34"/><w:color w:val="${INK}"/></w:rPr></w:style>` +
  `<w:style w:type="paragraph" w:styleId="Baslik2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="24"/><w:szCs w:val="24"/><w:color w:val="1E2F3D"/></w:rPr></w:style>` +
  `<w:style w:type="paragraph" w:styleId="Baslik3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:sz w:val="21"/><w:szCs w:val="21"/></w:rPr></w:style>` +
  "</w:styles>";

const CONTENT_TYPES = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' +
  '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
  '<Default Extension="xml" ContentType="application/xml"/>' +
  '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
  '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>' +
  "</Types>";

const PACKAGE_RELS = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
  '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>' +
  "</Relationships>";

const DOCUMENT_RELS = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
  '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>' +
  "</Relationships>";

export function buildReportDocx(input: ReportInput): Uint8Array {
  return zipSync({
    "[Content_Types].xml": strToU8(CONTENT_TYPES),
    "_rels/.rels": strToU8(PACKAGE_RELS),
    "word/document.xml": strToU8(documentXml(input)),
    "word/styles.xml": strToU8(STYLES_XML),
    "word/_rels/document.xml.rels": strToU8(DOCUMENT_RELS),
  }, { level: 6 });
}
