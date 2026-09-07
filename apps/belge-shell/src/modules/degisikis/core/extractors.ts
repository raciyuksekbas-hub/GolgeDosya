import mammoth from "mammoth";
import { invoke } from "@tauri-apps/api/core";
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.mjs";
import pdfWorker from "pdfjs-dist/legacy/build/pdf.worker.min.mjs?url";
import { strFromU8, strToU8, unzipSync, zipSync } from "fflate";
import { makeBlocks, normalizeTechnicalNoise } from "./normalize";
import type { BlockKind, LocalDocument } from "./types";
import { udfXmlEntries } from "./udf";

pdfjs.GlobalWorkerOptions.workerSrc = pdfWorker;

export class ExtractionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ExtractionError";
  }
}

export type Entry = {
  text: string;
  kind?: BlockKind;
  page?: number;
  sourceListOrdinal?: string;
  listLevel?: number;
};

type DocumentExtension = LocalDocument["extension"];

const OLE_SIGNATURE = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1] as const;
const ZIP_SIGNATURES = [[0x50, 0x4b, 0x03, 0x04], [0x50, 0x4b, 0x05, 0x06], [0x50, 0x4b, 0x07, 0x08]] as const;

function startsWithBytes(bytes: Uint8Array, signature: readonly number[]): boolean {
  return signature.every((value, index) => bytes[index] === value);
}

export function detectDocumentExtension(fileName: string, mimeType: string, buffer: ArrayBuffer): DocumentExtension {
  // File-format identifiers are ASCII protocol values; locale-sensitive casing is unsafe here.
  const extension = fileName.split(".").pop()?.toLowerCase();
  const mime = mimeType.toLowerCase();
  const bytes = new Uint8Array(buffer, 0, Math.min(buffer.byteLength, 16));
  if (extension === "doc") {
    if (!startsWithBytes(bytes, OLE_SIGNATURE) || mime.includes("openxmlformats")) {
      throw new ExtractionError("Bu Word belgesi okunamadı. Dosya bozuk veya desteklenmeyen bir eski Word biçiminde olabilir.");
    }
    return "doc";
  }
  if (extension === "docx") {
    if (!ZIP_SIGNATURES.some((signature) => startsWithBytes(bytes, signature)) || mime === "application/msword") {
      throw new ExtractionError("Bu DOCX dosyası okunamadı. Dosyanın uzantısını ve bütünlüğünü kontrol edin.");
    }
    return "docx";
  }
  if (extension === "pdf") return "pdf";
  if (extension === "udf") return "udf";
  throw new ExtractionError("Desteklenmeyen dosya türü. PDF, DOC, DOCX veya UDF seçin.");
}

function htmlTagName(element: Element): string {
  // HTML tag names are ASCII keywords. Turkish locale casing turns LI into "lı".
  return element.tagName.toLowerCase();
}

function elementChildren(node: Node): Element[] {
  return Array.from(node.childNodes).filter((child): child is Element => child.nodeType === 1);
}

function textWithoutNestedLists(node: Node): string {
  return Array.from(node.childNodes).map((child) => {
    if (child.nodeType === 3) return child.nodeValue ?? "";
    if (child.nodeType !== 1) return "";
    const tag = htmlTagName(child as Element);
    return tag === "ul" || tag === "ol" ? "" : textWithoutNestedLists(child);
  }).join("");
}

export function htmlEntriesFromRoot(root: Node): Entry[] {
  const entries: Entry[] = [];
  const positiveIntegerAttribute = (element: Element, name: string): number | undefined => {
    const value = element.getAttribute(name);
    if (!value || !/^\d+$/u.test(value)) return undefined;
    const number = Number(value);
    return Number.isSafeInteger(number) && number > 0 ? number : undefined;
  };
  const visit = (element: Element, listLevel = 0) => {
    const tag = htmlTagName(element);
    if (tag === "table") {
      for (const row of Array.from(element.getElementsByTagName("tr"))) {
        const cells = elementChildren(row)
          .filter((cell) => ["th", "td"].includes(htmlTagName(cell)))
          .map((cell) => normalizeTechnicalNoise(cell.textContent ?? ""))
          .filter(Boolean);
        if (cells.length) entries.push({ text: cells.join(" | "), kind: "table" });
      }
      return;
    }
    if (tag === "ul" || tag === "ol") {
      const ordered = tag === "ol";
      let nextOrdinal = ordered ? positiveIntegerAttribute(element, "start") ?? 1 : undefined;
      for (const item of elementChildren(element).filter((child) => htmlTagName(child) === "li")) {
        const sourceListOrdinal = ordered
          ? positiveIntegerAttribute(item, "value") ?? nextOrdinal
          : undefined;
        const text = normalizeTechnicalNoise(textWithoutNestedLists(item));
        if (text) entries.push({
          text,
          kind: "list",
          sourceListOrdinal: sourceListOrdinal?.toString(),
          listLevel,
        });
        elementChildren(item)
          .filter((child) => ["ul", "ol"].includes(htmlTagName(child)))
          .forEach((child) => visit(child, listLevel + 1));
        if (sourceListOrdinal !== undefined) nextOrdinal = sourceListOrdinal + 1;
      }
      return;
    }
    if (/^(?:p|h[1-6])$/.test(tag)) {
      const text = normalizeTechnicalNoise(element.textContent ?? "");
      if (text) entries.push({ text, kind: /^h[1-6]$/.test(tag) ? "heading" : "paragraph" });
      return;
    }
    elementChildren(element).forEach(visit);
  };
  elementChildren(root).forEach(visit);
  return entries;
}

function htmlEntries(html: string): Entry[] {
  const document = new DOMParser().parseFromString(html, "text/html");
  return htmlEntriesFromRoot(document.body);
}

function exactArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function decodeXmlText(value: string): string {
  return value
    .replace(/<[^>]+>/gu, "")
    .replace(/&quot;/gu, "\"")
    .replace(/&apos;/gu, "'")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&amp;/gu, "&");
}

function isPaginationFieldInstruction(value: string): boolean {
  const instruction = decodeXmlText(value).trim().toUpperCase();
  return /^(?:PAGE|NUMPAGES|SECTIONPAGES)(?:\s|\\|$)/u.test(instruction);
}

function removePaginationFields(xml: string): string {
  const withoutSimpleFields = xml.replace(
    /<w:fldSimple\b([^>]*)>[\s\S]*?<\/w:fldSimple>/gu,
    (field, attributes: string) => {
      const instruction = attributes.match(/\bw:instr\s*=\s*(["'])([\s\S]*?)\1/u)?.[2] ?? "";
      return isPaginationFieldInstruction(instruction) ? "" : field;
    },
  );
  return withoutSimpleFields.replace(
    /<w:r\b[^>]*>(?:(?!<\/w:r>)[\s\S])*?<w:fldChar\b[^>]*\bw:fldCharType\s*=\s*(["'])begin\1[^>]*\/?\s*>(?:(?!<\/w:r>)[\s\S])*?<\/w:r>[\s\S]*?<w:r\b[^>]*>(?:(?!<\/w:r>)[\s\S])*?<w:fldChar\b[^>]*\bw:fldCharType\s*=\s*(["'])end\2[^>]*\/?\s*>(?:(?!<\/w:r>)[\s\S])*?<\/w:r>/gu,
    (field) => {
      const instruction = Array.from(field.matchAll(/<w:instrText\b[^>]*>([\s\S]*?)<\/w:instrText>/gu), (match) => match[1]).join(" ");
      return isPaginationFieldInstruction(instruction) ? "" : field;
    },
  );
}

function removePageFurnitureTextboxes(xml: string): string {
  const isPageFurniture = (shape: string) => (
    /<wp:anchor\b[^>]*\bbehindDoc\s*=\s*(["'])1\1/gu.test(shape)
    && /<wp:position[HV]\b[^>]*\brelativeFrom\s*=\s*(["'])page\1/gu.test(shape)
    && /<w:txbxContent\b/gu.test(shape)
  );
  const withoutAlternateContent = xml.replace(
    /<mc:AlternateContent\b[\s\S]*?<\/mc:AlternateContent>/gu,
    (shape) => isPageFurniture(shape) ? "" : shape,
  );
  return withoutAlternateContent.replace(
    /<w:drawing\b[\s\S]*?<\/w:drawing>/gu,
    (shape) => isPageFurniture(shape) ? "" : shape,
  );
}

export function prepareDocxFinalView(buffer: ArrayBuffer): ArrayBuffer {
  const archive = unzipSync(new Uint8Array(buffer));
  let changed = false;
  for (const [name, bytes] of Object.entries(archive)) {
    if (!/^word\/.*\.xml$/iu.test(name)) continue;
    const xml = strFromU8(bytes);
    const finalViewXml = removePaginationFields(name === "word/document.xml" ? removePageFurnitureTextboxes(xml) : xml)
      .replace(/<(\/?)w:moveTo\b/gu, "<$1w:ins")
      .replace(/<(\/?)w:moveFrom\b/gu, "<$1w:del");
    if (finalViewXml === xml) continue;
    archive[name] = strToU8(finalViewXml);
    changed = true;
  }
  return changed ? exactArrayBuffer(zipSync(archive)) : buffer;
}

async function extractDocx(buffer: ArrayBuffer): Promise<{ entries: Entry[]; warnings: string[] }> {
  const finalViewBuffer = prepareDocxFinalView(buffer);
  const result = await mammoth.convertToHtml(
    { arrayBuffer: finalViewBuffer },
    { includeDefaultStyleMap: true, ignoreEmptyParagraphs: true },
  );
  const entries = htmlEntries(result.value);
  if (!entries.length) throw new ExtractionError("Bu DOCX dosyasında karşılaştırılabilir metin bulunamadı.");
  return { entries, warnings: result.messages.filter((message) => message.type === "warning").map((message) => message.message) };
}

export type LegacyDocConverter = (buffer: ArrayBuffer) => Promise<ArrayBuffer>;

export function withoutLegacyPaginationArtifacts(entries: Entry[]): Entry[] {
  let contentEnd = entries.length;
  while (contentEnd > 0 && /^PAGE(?:\s+\d+)?$/u.test(entries[contentEnd - 1].text)) contentEnd--;
  return entries.slice(0, contentEnd);
}

export function legacyDocErrorMessage(code: string): string {
  if (code.includes("DOC_WINDOWS_WORD_REQUIRED")) {
    return "DOC belgesi açılamadı. Windows'ta eski .doc belgelerinin dönüştürülmesi için Microsoft Word gereklidir. Belgeyi .docx olarak kaydedip yeniden deneyebilirsiniz.";
  }
  if (code.includes("DOC_PASSWORD")) {
    return "Parola korumalı .doc belgeleri desteklenmiyor.";
  }
  if (code.includes("DOC_INVALID")) {
    return "Belge geçerli bir Word .doc dosyası olarak okunamadı.";
  }
  return "Eski Word belgesi yerel olarak dönüştürülemedi.";
}

export async function convertLegacyDocLocally(buffer: ArrayBuffer): Promise<ArrayBuffer> {
  try {
    // Komut adı birleşik kabukta modül önekli; imza ve davranış aynı.
    const converted = await invoke<number[]>("degisikis_convert_legacy_doc", {
      contents: Array.from(new Uint8Array(buffer)),
    });
    return exactArrayBuffer(Uint8Array.from(converted));
  } catch (error) {
    const code = typeof error === "string" ? error : error instanceof Error ? error.message : "";
    throw new ExtractionError(legacyDocErrorMessage(code));
  }
}

export async function extractLegacyDoc(
  buffer: ArrayBuffer,
  converter: LegacyDocConverter = convertLegacyDocLocally,
): Promise<{ entries: Entry[]; warnings: string[] }> {
  const converted = await converter(buffer);
  const result = await extractDocx(converted);
  return {
    entries: withoutLegacyPaginationArtifacts(result.entries),
    warnings: ["Eski Word biçimi yerel olarak dönüştürülerek okundu.", ...result.warnings],
  };
}

export interface PdfLine { text: string; y: number; page: number; startX: number; endX: number }

export function pageLines(items: Array<{ str?: string; transform?: number[]; width?: number }>, page: number): PdfLine[] {
  const rows = new Map<number, Array<{ text: string; x: number; endX: number }>>();
  for (const item of items) {
    const text = item.str?.trim();
    if (!text) continue;
    const x = item.transform?.[4] ?? 0;
    const y = Math.round((item.transform?.[5] ?? 0) * 2) / 2;
    const nearby = Array.from(rows.keys()).find((key) => Math.abs(key - y) <= 1.5);
    const key = nearby ?? y;
    const row = rows.get(key) ?? [];
    row.push({ text, x, endX: x + (item.width ?? 0) });
    rows.set(key, row);
  }
  return Array.from(rows, ([y, pieces]) => {
    const ordered = [...pieces].sort((a, b) => a.x - b.x);
    return {
      y,
      page,
      text: ordered.map((piece) => piece.text).join(" "),
      startX: ordered[0].x,
      endX: ordered.reduce((widest, piece) => Math.max(widest, piece.endX), ordered[0].endX),
    };
  }).sort((a, b) => b.y - a.y);
}

/**
 * The document's dominant line pitch: the most frequent gap between consecutive lines.
 * Wrapped lines of one paragraph sit exactly one pitch apart, so it is the *extra* leading
 * above that pitch which separates two visual blocks. A fixed threshold cannot express
 * this — a blank line spans about 24 units at 10pt but about 34 at 14pt, so a single
 * constant either misses blank lines in compact documents or splits wrapped paragraphs
 * in generously spaced ones. Ties fall back to the smallest gap, which is the pitch
 * whenever block separations are never tighter than ordinary line spacing.
 */
function dominantLinePitch(pages: PdfLine[][]): number | undefined {
  const counts = new Map<number, number>();
  for (const lines of pages) {
    for (let index = 1; index < lines.length; index++) {
      const gap = Math.round((lines[index - 1].y - lines[index].y) * 2) / 2;
      if (gap > 0.5) counts.set(gap, (counts.get(gap) ?? 0) + 1);
    }
  }
  let pitch: number | undefined;
  let best = 0;
  for (const [gap, count] of counts) {
    if (count > best || (count === best && pitch !== undefined && gap < pitch)) {
      pitch = gap;
      best = count;
    }
  }
  return pitch;
}

function repeatedMargins(pages: PdfLine[][]): Set<string> {
  const counts = new Map<string, number>();
  for (const lines of pages) {
    const candidates = [...lines.slice(0, 2), ...lines.slice(-2)];
    for (const line of new Set(candidates.map((candidate) => normalizeTechnicalNoise(candidate.text)))) {
      if (line && !/^\d{1,4}$/.test(line)) counts.set(line, (counts.get(line) ?? 0) + 1);
    }
  }
  const threshold = Math.max(2, Math.ceil(pages.length * 0.6));
  return new Set(Array.from(counts).filter(([, count]) => count >= threshold).map(([text]) => text));
}

/** Extra leading, as a multiple of the dominant pitch, that separates two visual blocks. */
const PARAGRAPH_LEADING = 1.4;
/** Fallback separation when a document is too short to establish a pitch. */
const FALLBACK_LEADING = 24;
/**
 * How much of the text column a line must fill to read as wrapped rather than finished.
 * A wrapped line breaks because the next word did not fit, so it reaches close to the right
 * margin; a line ending far short of it has completed its block. Measured on rendered legal
 * PDFs the two populations sit far apart — continuation lines fill upwards of 98% of the
 * column, block-final and signature lines under 35% — so the floor sits between them.
 */
const WRAPPED_LINE_FILL = 0.6;

export function pdfEntries(pages: PdfLine[][]): Entry[] {
  const repeated = repeatedMargins(pages);
  const pitch = dominantLinePitch(pages);
  const entries: Entry[] = [];
  for (const page of pages) {
    const lines = page.filter((line) => {
      const clean = normalizeTechnicalNoise(line.text);
      return clean && !repeated.has(clean) && !/^[-–—]?\s*\d{1,4}\s*[-–—]?$/.test(clean);
    });
    const rightEdge = lines.reduce((edge, line) => Math.max(edge, line.endX), -Infinity);
    const leftEdge = lines.reduce((edge, line) => Math.min(edge, line.startX), Infinity);
    const columnWidth = rightEdge - leftEdge;
    let buffer = "";
    let previous: PdfLine | undefined;
    const flush = () => {
      if (buffer.trim()) entries.push({ text: buffer, page: page[0]?.page });
      buffer = "";
    };
    for (const line of lines) {
      const text = line.text.trim();
      const gap = previous !== undefined ? previous.y - line.y : 0;
      const largeGap = previous !== undefined
        && (pitch !== undefined ? gap > pitch * PARAGRAPH_LEADING : gap > FALLBACK_LEADING);
      // A line that stopped well short of the right margin did not wrap into this one.
      const previousLineFinished = previous !== undefined && columnWidth > 0
        && (previous.endX - leftEdge) / columnWidth < WRAPPED_LINE_FILL;
      const newStructuralBlock = /^(?:(?:geçici\s+)?madde\s+[\d/A-ZÇĞİÖŞÜ.-]+|[IVXLCDM]+[.)-]|\d+(?:\.\d+)*[.)])\s+/iu.test(text);
      if ((largeGap || previousLineFinished || newStructuralBlock) && buffer) flush();
      if (buffer.endsWith("-") && /^\p{Ll}/u.test(text)) buffer = buffer.slice(0, -1) + text;
      else buffer += `${buffer ? " " : ""}${text}`;
      if (/[.!?;:]$/.test(text) && buffer.length > 180) flush();
      previous = line;
    }
    flush();
  }
  return entries;
}

async function extractPdf(buffer: ArrayBuffer): Promise<{ entries: Entry[]; warnings: string[] }> {
  const task = pdfjs.getDocument({ data: new Uint8Array(buffer), useWorkerFetch: false });
  const pdf = await task.promise;
  const pages: PdfLine[][] = [];
  for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber++) {
    const page = await pdf.getPage(pageNumber);
    const content = await page.getTextContent();
    pages.push(pageLines(content.items as Array<{ str?: string; transform?: number[]; width?: number }>, pageNumber));
  }
  const entries = pdfEntries(pages);
  if (!entries.length || entries.reduce((sum, entry) => sum + entry.text.length, 0) < 8) {
    throw new ExtractionError("Bu PDF'de karşılaştırılabilir metin bulunamadı. Görüntü tabanlı/taranmış PDF'ler henüz desteklenmemektedir.");
  }
  return { entries, warnings: [] };
}

async function extractUdf(buffer: ArrayBuffer): Promise<{ entries: Entry[]; warnings: string[] }> {
  const bytes = new Uint8Array(buffer);
  let candidates: string[] = [];
  try {
    const archive = unzipSync(bytes);
    candidates = Object.entries(archive)
      .filter(([name]) => /(?:content|document|body|richtext|udf).*\.(?:xml|html?)$/i.test(name) || /\.(?:xml|html?)$/i.test(name))
      .sort(([a], [b]) => Number(/content/i.test(b)) - Number(/content/i.test(a)))
      .map(([, value]) => strFromU8(value));
  } catch {
    candidates = [strFromU8(bytes)];
  }
  for (const candidate of candidates) {
    const entries = /<html/i.test(candidate) ? htmlEntries(candidate) : udfXmlEntries(candidate);
    if (entries.length) return { entries, warnings: [] };
  }
  throw new ExtractionError("Bu UDF dosyasında karşılaştırılabilir metin bulunamadı.");
}

export async function extractDocument(file: File): Promise<LocalDocument> {
  const buffer = await file.arrayBuffer();
  const extension = detectDocumentExtension(file.name, file.type, buffer);
  let result: { entries: Entry[]; warnings: string[] };
  try {
    if (extension === "doc") result = await extractLegacyDoc(buffer);
    else if (extension === "docx") result = await extractDocx(buffer);
    else if (extension === "pdf") result = await extractPdf(buffer);
    else result = await extractUdf(buffer);
  } catch (error) {
    if (error instanceof ExtractionError) throw error;
    throw new ExtractionError(`${file.name} okunamadı. Dosyanın bozuk olmadığını kontrol edin.`);
  }
  return {
    name: file.name,
    extension,
    size: file.size,
    blocks: makeBlocks(result.entries, extension),
    warnings: result.warnings,
  };
}
