export type StructuralPrefixKind = "article" | "paragraph";

export interface StructuralText {
  body: string;
  prefix?: {
    kind: StructuralPrefixKind;
    number: string;
    raw: string;
    label: string;
  };
}

const ARTICLE_PREFIX = /^\s*((?:geçici\s+)?madde|madd[eıi]|article)\s*[-–—.]?\s*([\dA-ZÇĞİÖŞÜ]+(?:[./-][\dA-ZÇĞİÖŞÜ]+)*)(?:\s*[-–—.:)]\s*|\s+|$)/iu;
const NUMBERED_PREFIX = /^\s*(\d+(?:[./]\d+)*)\s*([.)]|[-–—])\s*/u;
const MULTILEVEL_CLAUSE_PREFIX = /^\s*(\d+(?:[./]\d+)+)(?:\s*([.)])?\s+)(?=\p{L})/u;
const APPENDIX_HEADING = /^\s*EK\s*[-–—.]?\s*(\d+)\b(?:\s*[-–—.:]\s*(.*)|\s+(.*))?$/iu;
const APPENDIX_NUMBERED_SECTION = /^\s*(\d{1,2})[.)]\s*(\p{L}[\s\S]*)$/u;
const APPENDIX_LETTERED_SECTION = /^\s*([A-ZÇĞİÖŞÜ])[.)]\s*(\p{L}[\s\S]*)$/u;

function titleCaseIfUppercase(text: string): string {
  const clean = text.trim();
  if (clean !== clean.toLocaleUpperCase("tr-TR")) return clean;
  return clean.toLocaleLowerCase("tr-TR")
    .replace(/(^|\s)(\p{L})/gu, (_, space: string, letter: string) => `${space}${letter.toLocaleUpperCase("tr-TR")}`);
}

export function appendixHeading(text: string): { label: string; title?: string } | undefined {
  const match = text.match(APPENDIX_HEADING);
  if (!match) return undefined;
  const title = (match[2] ?? match[3] ?? "").trim();
  return { label: `EK-${match[1]}`, title: title || undefined };
}

export function appendixSection(text: string): string | undefined {
  const clean = text.trim();
  if (/^(?:T\.?\s*C\.?\s*Kimlik\s+No|Ad\s+Soyad|İmza(?:\s*\/\s*Kaşe)?|Tarih)\s*:/iu.test(clean)) return undefined;
  const numbered = text.match(APPENDIX_NUMBERED_SECTION);
  if (numbered) return `${numbered[1]}. ${titleCaseIfUppercase(numbered[2])}`;
  const lettered = text.match(APPENDIX_LETTERED_SECTION);
  if (lettered) return `${lettered[1].toLocaleUpperCase("tr-TR")}. ${titleCaseIfUppercase(lettered[2])}`;
  if (/^İMZA\s+BÖLÜMÜ$/iu.test(clean)) return "İmza Bölümü";
  return undefined;
}

function looksLikeCalendarDate(number: string): boolean {
  const parts = number.split(/[./]/u);
  if (parts.length !== 3) return false;
  const [day, month, year] = parts.map(Number);
  return day >= 1 && day <= 31
    && month >= 1 && month <= 12
    && (parts[2].length === 2 || parts[2].length === 4)
    && year >= 0;
}

function looksLikeClauseNumber(number: string): boolean {
  const parts = number.split(/[./]/u);
  return parts.length >= 2
    && parts.length <= 4
    && parts.every((part) => /^\d{1,2}$/u.test(part));
}

function looksLikeNumberedArticleTitle(body: string, delimiter: string): boolean {
  if (!/[-–—]/u.test(delimiter)) return false;
  const clean = body.replace(/[:.;]+$/u, "").trim();
  const letters = clean.match(/\p{L}/gu)?.length ?? 0;
  return clean.length > 0
    && clean.length <= 100
    && letters >= 2
    && clean === clean.toLocaleUpperCase("tr-TR");
}

export function splitStructuralPrefix(text: string): StructuralText {
  const article = text.match(ARTICLE_PREFIX);
  if (article) {
    const title = article[1].toLocaleLowerCase("tr-TR").startsWith("geçici") ? "Geçici Madde" : "Madde";
    return {
      body: text.slice(article[0].length).trimStart(),
      prefix: {
        kind: "article",
        number: article[2],
        raw: article[0].trim(),
        label: `${title} ${article[2]}`,
      },
    };
  }
  const clause = text.match(MULTILEVEL_CLAUSE_PREFIX);
  const calendarDatePrefix = clause ? looksLikeCalendarDate(clause[1]) : false;
  if (clause && !calendarDatePrefix && looksLikeClauseNumber(clause[1])) {
    return {
      body: text.slice(clause[0].length).trimStart(),
      prefix: {
        kind: "paragraph",
        number: clause[1],
        raw: clause[0].trim(),
        label: `${clause[1]}. paragraf`,
      },
    };
  }
  const numbered = clause && !looksLikeClauseNumber(clause[1]) ? null : calendarDatePrefix ? null : text.match(NUMBERED_PREFIX);
  if (numbered) {
    const body = text.slice(numbered[0].length).trimStart();
    const articleTitle = looksLikeNumberedArticleTitle(body, numbered[2]);
    return {
      body,
      prefix: {
        kind: articleTitle ? "article" : "paragraph",
        number: numbered[1],
        raw: numbered[0].trim(),
        label: articleTitle ? `Madde ${numbered[1]}` : `${numbered[1]}. paragraf`,
      },
    };
  }
  return { body: text };
}
