// Word OTOMATİK NUMARALANDIRMA değişikliği karşılaştırmada görünür mü?
//
// SAHA/ÇEKİŞMELİ BULGU: madde numarası Word'ün numbering modelinden gelir ve
// paragrafın metninde YOKTUR. "3.2. Sorumluluk" maddesi "4. Sorumluluk"a
// dönüştüğünde iki tarafın çıkarılan metni birebir aynı ("Sorumluluk") olur
// ve karşılaştırma HİÇBİR fark üretmez. Hukuk belgesinde bu kabul edilemez:
// madde numarası değişince bütün çapraz atıflar ("madde 3.2'de belirtilen")
// yanlış maddeyi gösterir.
import { strToU8, zipSync } from "fflate";
import { beforeAll, describe, expect, it, vi } from "vitest";

// Tarayıcıya ait iki şey Node koşumunda yok: `DOMParser` ve mammoth'un
// tarayıcı girdisi (`arrayBuffer`). Ürün tarayıcıda çalışır; burada yalnız
// koşum farkı köprülenir, ürün yolu değişmez.
vi.mock("mammoth", async () => {
  const real = (await vi.importActual("mammoth/lib/index.js")) as {
    convertToHtml: (o: unknown, e?: unknown) => unknown;
  };
  return {
    default: {
      convertToHtml: (opts: { arrayBuffer?: ArrayBuffer }, extra?: unknown) =>
        real.convertToHtml({ buffer: Buffer.from(opts.arrayBuffer as ArrayBuffer) }, extra),
    },
  };
});

beforeAll(async () => {
  const { DOMParser: XmlDom } = await import("@xmldom/xmldom");
  class Shim {
    parseFromString(html: string) {
      const cleaned = html
        .replace(/&nbsp;/g, "\u00a0")
        .replace(/<(br|img|hr)([^>]*?)\/?>/g, "<$1$2/>");
      const doc = new XmlDom().parseFromString(`<body>${cleaned}</body>`, "text/xml") as unknown as {
        documentElement: unknown;
      };
      return { body: doc.documentElement };
    }
  }
  (globalThis as { DOMParser?: unknown }).DOMParser = Shim;
});
import { prepareDocxFinalView } from "./core/extractors";

const NS = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';

/** Tek bir soyut numaralandırma düzeyi. */
function level(ilvl: number, fmt: string, text: string, start = 1) {
  return `<w:lvl w:ilvl="${ilvl}"><w:start w:val="${start}"/><w:numFmt w:val="${fmt}"/>`
    + `<w:lvlText w:val="${text}"/></w:lvl>`;
}

/** `numbering.xml`: bir soyut tanım + ona bağlı somut numaralar. */
export function numberingXml(abstractId: number, levels: string[], nums: [number, number][]) {
  const abstract = `<w:abstractNum w:abstractNumId="${abstractId}">${levels.join("")}</w:abstractNum>`;
  const concrete = nums
    .map(([numId, abs]) => `<w:num w:numId="${numId}"><w:abstractNumId w:val="${abs}"/></w:num>`)
    .join("");
  return `<?xml version="1.0"?><w:numbering ${NS}>${abstract}${concrete}</w:numbering>`;
}

/** Numaralandırmaya BAĞLI paragraf — numara metinde yazmaz. */
export function numbered(numId: number, ilvl: number, text: string) {
  return `<w:p><w:pPr><w:pStyle w:val="ListParagraph"/>`
    + `<w:numPr><w:ilvl w:val="${ilvl}"/><w:numId w:val="${numId}"/></w:numPr></w:pPr>`
    + `<w:r><w:t>${text}</w:t></w:r></w:p>`;
}

export function plain(text: string) {
  return `<w:p><w:r><w:t>${text}</w:t></w:r></w:p>`;
}

export function docx(body: string, numbering?: string): ArrayBuffer {
  const files: Record<string, Uint8Array> = {
    "[Content_Types].xml": strToU8(
      `<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">`
      + `<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>`
      + `<Default Extension="xml" ContentType="application/xml"/>`
      + `<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>`
      + `</Types>`,
    ),
    "_rels/.rels": strToU8(
      `<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">`
      + `<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>`
      + `</Relationships>`,
    ),
    "word/document.xml": strToU8(
      `<?xml version="1.0"?><w:document ${NS}><w:body>${body}</w:body></w:document>`,
    ),
  };
  if (numbering) files["word/numbering.xml"] = strToU8(numbering);
  const zipped = zipSync(files);
  return zipped.buffer.slice(zipped.byteOffset, zipped.byteOffset + zipped.byteLength) as ArrayBuffer;
}

/** Hazırlanan DOCX'in `document.xml`'inde görünen metin. */
function preparedText(buffer: ArrayBuffer): string {
  const { unzipSync, strFromU8 } = require("fflate") as typeof import("fflate");
  const out = prepareDocxFinalView(buffer);
  const xml = strFromU8(unzipSync(new Uint8Array(out))["word/document.xml"]);
  return (xml.match(/<w:t[^>]*>([^<]*)<\/w:t>/gu) ?? [])
    .map((m) => m.replace(/<[^>]+>/gu, ""))
    .join(" ");
}

/** Bir DOCX'ten karşılaştırma bloklarını üretir. */
async function blocks(body: string, numbering?: string) {
  const { extractDocument } = await import("./core/extractors");
  const buffer = docx(body, numbering);
  const file = new File([new Uint8Array(buffer)], "s.docx", {
    type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  });
  return (await extractDocument(file)).blocks;
}

async function diffCount(a: { body: string; numbering?: string }, b: { body: string; numbering?: string }) {
  const { compareDocuments } = await import("./core/compare");
  const { buildComparisonViewModel } = await import("./viewModels/comparisonViewModel");
  const model = buildComparisonViewModel(
    compareDocuments(await blocks(a.body, a.numbering), await blocks(b.body, b.numbering)),
  );
  return { total: model.summary.total, json: JSON.stringify(model.changes) };
}

const DECIMAL = (numIds: [number, number][] = [[4, 0]]) =>
  numberingXml(0, [level(0, "decimal", "%1."), level(1, "decimal", "%1.%2.")], numIds);
const LETTER = numberingXml(1, [level(0, "lowerLetter", "%1)")], [[7, 1]]);
const ROMAN = numberingXml(2, [level(0, "lowerRoman", "(%1)")], [[8, 2]]);

describe("görünür numara metne girer", () => {
  it("numaralı paragraf etiketini taşır", async () => {
    const text = preparedText(docx(numbered(4, 1, "Sorumluluk"), DECIMAL()));
    expect(text).toContain("1.1.");
  });

  it("numarasız paragraf etiket almaz", async () => {
    expect(preparedText(docx(plain("Giriş"), DECIMAL()))).toBe("Giriş");
  });

  it("kullanıcı numarayı elle yazmışsa ikinci kez yazılmaz", async () => {
    const body = numbered(4, 0, "1. Sorumluluk");
    expect(preparedText(docx(body, DECIMAL()))).toBe("1. Sorumluluk");
  });
});

describe("§6 — kabul senaryoları", () => {
  it("A) 3.2. -> 4.  fark VAR", async () => {
    // Temel: 3 üst madde, sonra 3.2 alt madde. Değişik: 4. üst madde.
    const base = numbered(4, 0, "Bir") + numbered(4, 0, "İki") + numbered(4, 0, "Üç")
      + numbered(4, 1, "Sorumluluk");
    const revised = numbered(4, 0, "Bir") + numbered(4, 0, "İki") + numbered(4, 0, "Üç")
      + numbered(4, 0, "Sorumluluk");
    const { total, json } = await diffCount({ body: base, numbering: DECIMAL() }, { body: revised, numbering: DECIMAL() });
    expect(total).toBeGreaterThan(0);
    expect(json).toContain("3.1.");
    expect(json).toContain("4.");
  });

  it("B) aynı görünür numara, farklı iç numId  fark YOK", async () => {
    // Karşılaştırılan şey Word'ün iç kimliği değil, kullanıcının gördüğüdür.
    const a = { body: numbered(4, 0, "Sorumluluk"), numbering: DECIMAL([[4, 0]]) };
    const b = { body: numbered(9, 0, "Sorumluluk"), numbering: DECIMAL([[9, 0]]) };
    expect((await diffCount(a, b)).total).toBe(0);
  });

  it("C) numara aynı, metin değişmiş  yalnız içerik farkı", async () => {
    // Çevresinde çıpa olsun: tek bloklu belgede eşleştirici zaten
    // "ekle + sil" üretir, bu numaralandırmayla ilgili değildir.
    const around = (clause: string) =>
      plain("TARAFLAR") + numbered(4, 0, clause) + numbered(4, 0, "Yürürlük") + plain("İMZALAR");
    const { json } = await diffCount(
      { body: around("Sorumluluk"), numbering: DECIMAL() },
      { body: around("Sorumluluk ve Tazminat"), numbering: DECIMAL() },
    );
    // Fark İÇERİKTEDİR; numara iki tarafta da "1." kalır.
    expect(json).toContain("Tazminat");
    expect(json).not.toContain("2. Sorumluluk");
  });

  it("D) numara değişmiş, metin aynı  numaralandırma farkı", async () => {
    const a = numbered(4, 0, "Bir") + numbered(4, 0, "Sorumluluk");
    const b = numbered(4, 0, "Sorumluluk");
    expect((await diffCount({ body: a, numbering: DECIMAL() }, { body: b, numbering: DECIMAL() })).total)
      .toBeGreaterThan(0);
  });

  it("E) iç içe liste kayması kaybolmaz", async () => {
    const nest = (top: number) => {
      let out = "";
      for (let i = 0; i < top; i += 1) out += numbered(4, 0, `Madde ${i}`);
      return out + numbered(4, 1, "Alt bir") + numbered(4, 1, "Alt iki");
    };
    const { total, json } = await diffCount(
      { body: nest(4), numbering: DECIMAL() },
      { body: nest(5), numbering: DECIMAL() },
    );
    expect(total).toBeGreaterThan(0);
    expect(json).toContain("4.1.");
    expect(json).toContain("5.1.");
  });

  it("F) a) b) c) ve (i) (ii) varyantları", async () => {
    const letters = preparedText(docx(numbered(7, 0, "Bir") + numbered(7, 0, "İki"), LETTER));
    expect(letters).toContain("a)");
    expect(letters).toContain("b)");
    const romans = preparedText(docx(numbered(8, 0, "Bir") + numbered(8, 0, "İki"), ROMAN));
    expect(romans).toContain("(i)");
    expect(romans).toContain("(ii)");
  });
});

describe("başka listeleri bozmuyor", () => {
  it("madde işaretli (bullet) liste numara almaz", async () => {
    const bullets = numberingXml(3, [level(0, "bullet", "\u2022")], [[11, 3]]);
    expect(preparedText(docx(numbered(11, 0, "Bir"), bullets))).toBe("Bir");
  });

  it("numbering.xml yoksa belge değişmez", async () => {
    expect(preparedText(docx(numbered(4, 0, "Sorumluluk")))).toBe("Sorumluluk");
  });

  it("derin düzey üst düzey artınca sıfırlanır", async () => {
    const body = numbered(4, 0, "A") + numbered(4, 1, "A1") + numbered(4, 0, "B") + numbered(4, 1, "B1");
    const text = preparedText(docx(body, DECIMAL()));
    expect(text).toContain("1.1.");
    expect(text).toContain("2.1.");
    expect(text).not.toContain("1.2.");
  });
});
