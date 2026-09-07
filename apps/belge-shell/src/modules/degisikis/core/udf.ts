import { normalizeTechnicalNoise } from "./normalize";
import type { BlockKind } from "./types";

export type UdfEntry = { text: string; kind?: BlockKind };

function tagName(element: Element): string {
  return (element.localName || element.tagName).toLocaleLowerCase("tr-TR").replace(/^.*:/, "");
}

function childElements(node: Element): Element[] {
  return Array.from(node.childNodes).filter((child): child is Element => child.nodeType === 1);
}

function descendants(node: Element, name?: string): Element[] {
  const result: Element[] = [];
  const visit = (element: Element) => {
    for (const child of childElements(element)) {
      if (!name || tagName(child) === name) result.push(child);
      visit(child);
    }
  };
  visit(node);
  return result;
}

function cleanUdfText(text: string): string {
  return text.replace(/[\u200B\uFEFF\uFFFC]/g, "").replace(/\r\n?/g, "\n").trim();
}

export function splitStructuralText(text: string): string[] {
  const clean = cleanUdfText(text);
  if (!clean) return [];
  let parts = clean.split(/\n+/).map(normalizeTechnicalNoise).filter(Boolean);

  // Some old UDF producers omit paragraph nodes and line separators. In that
  // narrow fallback, article starts are safer boundaries than one giant block.
  if (parts.length === 1 && parts[0].length > 400) {
    const articleStarts = parts[0].match(/(?:^|\s)(?:geçici\s+)?madde\s+[\d/A-ZÇĞİÖŞÜ.-]+/giu) ?? [];
    if (articleStarts.length > 1) {
      parts = parts[0]
        .split(/(?=(?:geçici\s+)?madde\s+[\d/A-ZÇĞİÖŞÜ.-]+(?:\s*[-–—.:)]|\s))/giu)
        .map(normalizeTechnicalNoise)
        .filter(Boolean);
    }
  }
  return parts;
}

function topLevelContent(root: Element): string {
  const content = childElements(root).find((child) => tagName(child) === "content" && !child.hasAttribute("startOffset"));
  // Offsets in UYAP files address the untouched Unicode code-point stream.
  // Trimming or normalizing before slicing would shift every paragraph.
  return content?.textContent ?? "";
}

function offsetText(node: Element, codePoints: string[]): string {
  const spans = [node, ...descendants(node)]
    .filter((element) => element.hasAttribute("startOffset") && element.hasAttribute("length"))
    .map((element) => ({
      start: Number.parseInt(element.getAttribute("startOffset") ?? "", 10),
      length: Number.parseInt(element.getAttribute("length") ?? "", 10),
    }))
    .filter((span) => Number.isFinite(span.start) && Number.isFinite(span.length) && span.length > 0)
    .sort((a, b) => a.start - b.start);
  if (!spans.length) return cleanUdfText(node.textContent ?? "");
  const start = spans[0].start;
  const end = Math.max(...spans.map((span) => span.start + span.length));
  return cleanUdfText(codePoints.slice(start, end).join(""));
}

function isListParagraph(element: Element): boolean {
  let current: Element | undefined = element;
  while (current) {
    if (["list", "item", "li"].includes(tagName(current))) return true;
    const attributes = Array.from(current.attributes);
    if (attributes.some((attribute) =>
      /(?:list|bullet|number|numara)/i.test(attribute.name) && !/^(?:false|0|none)?$/i.test(attribute.value),
    )) return true;
    current = current.parentNode?.nodeType === 1 ? current.parentNode as Element : undefined;
  }
  return false;
}

function renderParagraph(element: Element, codePoints: string[]): UdfEntry[] {
  const text = offsetText(element, codePoints);
  return splitStructuralText(text).map((part) => ({
    text: part,
    kind: isListParagraph(element) ? "list" : "paragraph",
  }));
}

function renderTable(table: Element, codePoints: string[]): UdfEntry[] {
  const rows = descendants(table, "row");
  const entries: UdfEntry[] = [];
  for (const row of rows) {
    const cells = descendants(row, "cell").filter((cell) => cell.parentNode === row);
    const cellTexts = cells.map((cell) => {
      const paragraphs = descendants(cell, "paragraph");
      const values = paragraphs.flatMap((paragraph) => renderParagraph(paragraph, codePoints).map((entry) => entry.text));
      const joined = values.reduce((text, value) => (
        /\d[.,]$/u.test(text) && /^\d/u.test(value) ? `${text}${value}` : `${text}${text ? " " : ""}${value}`
      ), "");
      return normalizeTechnicalNoise((values.length ? joined : offsetText(cell, codePoints)));
    }).filter(Boolean);
    if (cellTexts.length) entries.push({ text: cellTexts.join(" | "), kind: "table" });
  }
  return entries;
}

function renderStructure(element: Element, codePoints: string[]): UdfEntry[] {
  const tag = tagName(element);
  if (tag === "paragraph" || tag === "p" || tag === "para") return renderParagraph(element, codePoints);
  if (tag === "table") return renderTable(element, codePoints);
  if (tag === "row" || tag === "cell" || tag === "content") return [];
  return childElements(element).flatMap((child) => renderStructure(child, codePoints));
}

function genericXmlEntries(root: Element): UdfEntry[] {
  const entries: UdfEntry[] = [];
  const blockTags = new Set(["p", "paragraph", "para", "h1", "h2", "h3", "h4", "h5", "h6", "row", "tr", "item", "li"]);
  for (const element of [root, ...descendants(root)]) {
    const tag = tagName(element);
    if (!blockTags.has(tag)) continue;
    const parent = element.parentNode?.nodeType === 1 ? element.parentNode as Element : undefined;
    if (parent && blockTags.has(tagName(parent))) continue;
    if (tag === "tr" || tag === "row") {
      const cells = descendants(element).filter((child) => ["cell", "td", "th"].includes(tagName(child)));
      const values = cells.map((cell) => normalizeTechnicalNoise(cell.textContent ?? "")).filter(Boolean);
      if (values.length) entries.push({ text: values.join(" | "), kind: "table" });
      continue;
    }
    const text = normalizeTechnicalNoise(element.textContent ?? "");
    if (text) entries.push({ text, kind: /^h[1-6]$/.test(tag) ? "heading" : ["li", "item"].includes(tag) ? "list" : "paragraph" });
  }
  return entries;
}

export function udfXmlEntries(xml: string): UdfEntry[] {
  const document = new DOMParser().parseFromString(xml, "application/xml");
  if (document.getElementsByTagName("parsererror").length) return [];
  const root = document.documentElement;
  const content = topLevelContent(root);
  const elements = childElements(root).find((child) => tagName(child) === "elements");

  if (content && elements) {
    const codePoints = Array.from(content);
    const structured = childElements(elements).flatMap((child) => renderStructure(child, codePoints));
    if (structured.length) return structured;
  }

  const generic = genericXmlEntries(root);
  if (generic.length) return generic;
  return splitStructuralText(content || root.textContent || "").map((text) => ({ text }));
}
