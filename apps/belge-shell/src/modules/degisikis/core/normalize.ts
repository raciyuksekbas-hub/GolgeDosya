import type { BlockKind, DocumentBlock } from "./types";
import { appendixHeading, appendixSection, splitStructuralPrefix } from "./structure";

const HEADING_RE = /^(?:[IVXLCDM]+[.)-]|[A-ZÇĞİÖŞÜ\s]{5,}|\d+(?:\.\d+)*[.)])\s+/u;

function canonicalizeFormPlaceholders(input: string): string {
  return input.replace(/(?:[._…]\s*){2,}/gu, (run, offset: number, full: string) => {
    const marks = Array.from(run).filter((character) => character === "." || character === "_" || character === "…");
    const effectiveLength = marks.reduce((sum, character) => sum + (character === "…" ? 3 : 1), 0);
    if (effectiveLength < 6) return run;
    const before = full.slice(0, offset).trimEnd().at(-1);
    const after = full.slice(offset + run.length).trimStart().at(0);
    const onlyPlaceholder = !full.replace(run, "").trim();
    const labelledField = before === ":" || before === "|" || before === "\t";
    const isolatedLongRun = effectiveLength >= 10 && before !== "." && after !== ".";
    return onlyPlaceholder || labelledField || isolatedLongRun ? "………" : run;
  });
}

export function normalizeTechnicalNoise(input: string): string {
  return canonicalizeFormPlaceholders(input)
    .replace(/\u00ad/g, "")
    .replace(/\u00a0/g, " ")
    .replace(/([\p{L}])[-‐]\s*\n\s*([\p{Ll}])/gu, "$1$2")
    .replace(/[\t ]*\n[\t ]*/g, " ")
    .replace(/[ \t]{2,}/g, " ")
    .replace(/(\p{Ll}[.!?])\s*(?=\p{Lu})/gu, "$1 ")
    .trim();
}

export function comparisonForm(input: string): string {
  return normalizeTechnicalNoise(input).normalize("NFC");
}

export function looseMatchForm(input: string): string {
  return comparisonForm(input)
    .toLocaleLowerCase("tr-TR")
    .replace(/[^\p{L}\p{N}%₺€$]+/gu, " ")
    .trim();
}

export function classifyBlock(text: string, hinted?: BlockKind): { kind: BlockKind; label?: string } {
  const clean = normalizeTechnicalNoise(text);
  const structural = splitStructuralPrefix(clean);
  if (structural.prefix?.kind === "article") return { kind: "article", label: structural.prefix.label };
  if (hinted === "table") return { kind: "table" };
  if (structural.prefix?.kind === "paragraph" && hinted !== "heading") return { kind: hinted ?? "paragraph" };
  if (hinted === "heading" || (clean.length < 140 && HEADING_RE.test(clean))) return { kind: "heading" };
  return { kind: hinted ?? "paragraph" };
}

export function makeBlocks(
  entries: Array<{
    text: string;
    kind?: BlockKind;
    page?: number;
    sourceListOrdinal?: string;
    listLevel?: number;
  }>,
  prefix = "block",
): DocumentBlock[] {
  // Tek başına duran 1–4 haneli sayı PDF'te sayfa numarasıdır; DOCX/UDF'te
  // İÇERİKTİR. Bu ayrım yapılmadan filtre her biçime uygulanıyordu ve
  // sözleşmenin tablo hücrelerindeki tutar ("1500"), yıl ("2026") ve madde
  // numarası ("12") karşılaştırma veri kümesinden SESSİZCE düşüyordu:
  // bedel 1500'den 1800'e çekilse Karşılaştır hiçbir fark göstermiyordu.
  // Yanlış negatif, yanlış pozitiften tehlikelidir (§46); bastırma yalnız
  // sayfa numarasının gerçekten yapı artığı olduğu yerde kalır.
  const suppressStandaloneNumbers = prefix === "pdf";
  let currentArticle: string | undefined;
  let currentClause: string | undefined;
  let currentAppendix: string | undefined;
  let currentAppendixSection: string | undefined;
  return entries
    .map((entry) => ({ ...entry, text: normalizeTechnicalNoise(entry.text) }))
    .filter(
      (entry) =>
        entry.text.length > 0
        && !(suppressStandaloneNumbers && /^\d{1,4}$/.test(entry.text)),
    )
    .map((entry, order) => {
      const appendix = appendixHeading(entry.text);
      const section = currentAppendix && !appendix ? appendixSection(entry.text) : undefined;
      const classified = classifyBlock(entry.text, entry.kind);
      const structural = splitStructuralPrefix(entry.text);
      const sourceListOrdinal = entry.sourceListOrdinal
        ?? (classified.kind === "list" && structural.prefix?.kind === "paragraph"
          ? structural.prefix.number
          : undefined);
      const sourceClauseNumber = structural.prefix?.kind === "paragraph"
        && (
          structural.prefix.number.includes(".")
          || structural.prefix.number.includes("/")
          || classified.kind !== "list"
          || !sourceListOrdinal
        )
        ? structural.prefix.number
        : undefined;
      if (appendix) {
        currentAppendix = appendix.label;
        currentAppendixSection = appendix.title ? `${appendix.label} / ${appendix.title}` : undefined;
        currentArticle = undefined;
        currentClause = undefined;
      } else if (section && currentAppendix) {
        currentAppendixSection = `${currentAppendix} / ${section}`;
        currentArticle = undefined;
        currentClause = undefined;
      } else if (classified.kind === "article") {
        currentArticle = classified.label;
        currentClause = undefined;
        currentAppendix = undefined;
        currentAppendixSection = undefined;
      } else if (classified.kind === "heading") {
        currentArticle = undefined;
        currentClause = undefined;
      }
      let location = appendix
        ? appendix.label
        : section
          ? currentAppendixSection
          : currentAppendix
            ? currentAppendixSection ?? `${currentAppendix} içindeki değiştirilen bölüm`
            : sourceClauseNumber
              ? `Madde ${sourceClauseNumber}`
              : classified.label;
      if (sourceClauseNumber && !currentAppendix) currentClause = location;
      if (!location && (currentArticle || currentClause) && ["paragraph", "list", "table"].includes(classified.kind)) {
        location = classified.kind === "table"
          ? `${currentArticle ?? currentClause} / Tablo`
          : classified.kind === "list" && sourceListOrdinal
            ? `${currentClause ?? currentArticle}/${sourceListOrdinal}`
          : `${currentArticle ?? currentClause} / Paragraf`;
      }
      if (!location && classified.kind === "list" && sourceListOrdinal) {
        location = `${sourceListOrdinal}. numaralı bölüm`;
      }
      const id = `${prefix}-${order}`;
      const block = {
        id,
        text: entry.text,
        kind: appendix ? "heading" : classified.kind,
        label: appendix ? appendix.label : section ? currentAppendixSection : classified.label,
        order,
        page: entry.page,
        location,
        sourceListOrdinal,
        listLevel: entry.listLevel,
        sourceClauseNumber,
        appendixLabel: currentAppendix,
        appendixSection: currentAppendixSection,
        sourceBlockIds: [id],
        sourceTexts: [entry.text],
      };
      return {
        ...block,
        sourceUnits: [{ id, text: entry.text, kind: block.kind, order, location }],
      };
    });
}

export function displayLocation(block: DocumentBlock | undefined, _fallbackIndex: number): string {
  if (!block) return "Değiştirilen bölüm";
  if (block.location) return block.location;
  if (block.label) return block.label;
  if (block.kind === "table") return "Tablo satırı";
  if (block.kind === "list") return block.sourceListOrdinal
    ? `${block.sourceListOrdinal}. numaralı bölüm`
    : "Liste öğesi";
  if (block.kind === "heading") return "Başlık";
  return "Değiştirilen paragraf";
}
