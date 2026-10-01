// Karşılaştır — panelde görünen metin belgedeki metnin AYNISI olmalı.
//
// Yeniden üretim: "Bedel 18.000." → "Bedel 18.500." değişikliğinde Değişik
// Sürüm paneli "Bedel 18.500 ." gösteriyordu; "T.C." "T. C.", "A.Ş.'nin"
// "A. Ş. ' nin", "%20" "% 20", "md.119/1-f" "md. 119/1 - f" oluyordu. Sebep
// iki katmanlıydı: motor bir parçanın sözcüklerini kaynağa bakmadan tahminle
// birleştiriyordu, panel de HER parçanın arasına boşluk koyuyordu. Hukuk
// metninde tutar, tarih, madde atfı ve kısaltma belgede nasıl yazılmışsa öyle
// okunmalı; bu testler panelin metnini kaynakla birebir karşılaştırır.
import { renderToStaticMarkup } from "react-dom/server";
import { strToU8, zipSync } from "fflate";
import { DOMParser as NodeDomParser } from "@xmldom/xmldom";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DocumentPane } from "./DocumentPane";
import { compareDocuments } from "./core/compare";
import mammoth from "mammoth";
import { extractDocument, htmlEntriesFromRoot, prepareDocxFinalView } from "./core/extractors";
import { makeBlocks } from "./core/normalize";
import { wordDiff } from "./core/wordDiff";
import type { ComparisonResult, DiffFragment } from "./core/types";

// Panelin sözleşmesi: parçalar arasında yalnız kaynakta boşluk varsa boşluk.
function joined(fragments: DiffFragment[]): string {
  return fragments.map((fragment, index) => `${index > 0 && fragment.spaceBefore !== false ? " " : ""}${fragment.text}`).join("");
}

function decode(html: string): string {
  return html
    .replace(/<[^>]+>/gu, "")
    .replace(/&#x27;/gu, "'")
    .replace(/&quot;/gu, "\"")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&amp;/gu, "&");
}

function paneHtml(comparison: ComparisonResult, side: "base" | "revised"): string {
  return renderToStaticMarkup(
    <DocumentPane
      side={side}
      doc={{ name: "s.docx", blocks: [] } as never}
      rows={comparison.rows}
      changes={[]}
      selectedRows={[]}
      paneRef={{ current: null }}
      rowRefs={{ current: new Map() }}
      onScroll={() => undefined}
    />,
  );
}

/** Paneldeki her satırın okunan metni (satır numarası hariç). */
function paneRows(comparison: ComparisonResult, side: "base" | "revised"): string[] {
  return Array.from(paneHtml(comparison, side).matchAll(/<div class="row-body">(.*?)<\/div>/gsu), (match) => decode(match[1]));
}

// Gerçek belgelerde görülen biçimler: tutar + nokta, kısaltma, ek, yüzde,
// atıf, parantez, tırnak, virgül, bitişik para birimi.
const PAIRS: Array<[string, string]> = [
  ["Bedel 18.000.", "Bedel 18.500."],
  ["Bedel 18.000TL'dir.", "Bedel 18.500TL'dir."],
  ["Toplam: 18.000,00-TL", "Toplam: 18.500,00-TL"],
  ["T.C. İstanbul 3. Asliye Hukuk Mahkemesi'ne", "T.C. Ankara 3. Asliye Hukuk Mahkemesi'ne"],
  ["Davalı ABC A.Ş.'nin borcu 01.10.2026 tarihinde doğmuştur.", "Davalı ABC A.Ş.'nin borcu 15.10.2026 tarihinde doğmuştur."],
  ["Madde 3.1 uyarınca %18 KDV uygulanır.", "Madde 3.1 uyarınca %20 KDV uygulanır."],
  ["HMK md.119/1-e hükmü (bkz. ek-1) saklıdır.", "HMK md.119/1-f hükmü (bkz. ek-2) saklıdır."],
  ["Tutar 18.500 TL olarak ödenir.", "Tutar 18.500 TL olarak, peşin ödenir."],
  ["“Kiracı” sözcüğü (A) kişiyi ifade eder.", "“Kiraya veren” sözcüğü (B) kişiyi ifade eder."],
  ["Masraflar (harç, vb.) davalıya aittir.", "Masraflar (harç, vekalet ücreti vb.) davalıya aittir."],
];

describe("sözcük farkı kaynağın boşluklarını taşır", () => {
  it.each(PAIRS)("%s → %s", (base, revised) => {
    const diff = wordDiff(base, revised);
    expect(joined(diff.base)).toBe(base);
    expect(joined(diff.revised)).toBe(revised);
  });

  it("değişen kısım tahminle değil kaynaktan alınır", () => {
    const diff = wordDiff("Bedel 18.000.", "Bedel 18.500.");
    expect(diff.revised).toEqual([
      { kind: "equal", text: "Bedel" },
      { kind: "added", text: "18.500" },
      { kind: "equal", text: ".", spaceBefore: false },
    ]);
    expect(diff.hunks).toEqual([{ removed: "18.000", added: "18.500" }]);
    expect(wordDiff("T.C. İstanbul", "T.C. Ankara").revised[0]).toEqual({ kind: "equal", text: "T.C." });
  });
});

describe("panel belgedeki metni gösterir", () => {
  it.each(PAIRS)("%s → %s", (base, revised) => {
    const comparison = compareDocuments(makeBlocks([{ text: base }], "base"), makeBlocks([{ text: revised }], "revised"));
    expect(comparison.rows[0].kind).toBe("modified");
    expect(paneRows(comparison, "base")).toEqual([base]);
    expect(paneRows(comparison, "revised")).toEqual([revised]);
  });

  it("işaret yalnız değişen sayıyı sarar; sonraki nokta bitişik kalır", () => {
    const comparison = compareDocuments(makeBlocks([{ text: "Bedel 18.000." }], "base"), makeBlocks([{ text: "Bedel 18.500." }], "revised"));
    expect(paneHtml(comparison, "revised")).toContain('<ins class="fragment added">18.500</ins><span class="fragment equal">.</span>');
    expect(paneHtml(comparison, "base")).toContain('<del class="fragment removed">18.000</del><span class="fragment equal">.</span>');
  });

  it("numaralı paragrafın öneki ile gövdesi arasındaki boşluk kaynağa uyar", () => {
    const comparison = compareDocuments(
      makeBlocks([{ text: "1.Kira bedeli 18.000 TL'dir." }], "base"),
      makeBlocks([{ text: "1.Kira bedeli 18.500 TL'dir." }], "revised"),
    );
    expect(paneRows(comparison, "revised")).toEqual(["1.Kira bedeli 18.500 TL'dir."]);
  });
});

describe("gerçek DOCX ve UDF çifti", () => {
  const browserDomParser = globalThis.DOMParser;
  beforeAll(() => {
    globalThis.DOMParser = NodeDomParser as unknown as typeof DOMParser;
  });
  afterAll(() => {
    globalThis.DOMParser = browserDomParser;
  });

  const escape = (text: string) => text.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");

  // Word bir sayıyı sık sık iki "run"a böler; çıkarıcı bunları birleştirir.
  // Uygulamanın DOCX yolu: son görünüm → mammoth → HTML girdileri → blok.
  // (mammoth'un Node yapısı `arrayBuffer` almadığı için `extractDocument`
  // yerine aynı adımlar `buffer` ile çağrılır; extractors.test.ts de böyle.)
  async function docxBlocks(paragraphs: string[][]) {
    const body = paragraphs
      .map((runs) => `<w:p>${runs.map((run) => `<w:r><w:t xml:space="preserve">${escape(run)}</w:t></w:r>`).join("")}</w:p>`)
      .join("");
    const document = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`;
    const zip = zipSync({
      "[Content_Types].xml": strToU8(`<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`),
      "_rels/.rels": strToU8(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`),
      "word/document.xml": strToU8(document),
      "word/_rels/document.xml.rels": strToU8(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>`),
    });
    const prepared = prepareDocxFinalView(zip.buffer.slice(zip.byteOffset, zip.byteOffset + zip.byteLength) as ArrayBuffer);
    const html = (await mammoth.convertToHtml({ buffer: Buffer.from(prepared) }, { includeDefaultStyleMap: true, ignoreEmptyParagraphs: true })).value;
    const root = new NodeDomParser().parseFromString(`<body>${html}</body>`, "text/xml").documentElement;
    return makeBlocks(htmlEntriesFromRoot(root as unknown as Node), "docx");
  }

  // UYAP UDF: tek içerik dizisi + paragraf ofsetleri, zip içinde content.xml.
  function udfFile(paragraphs: string[]): File {
    const content = paragraphs.join("\n");
    let offset = 0;
    const elements = paragraphs.map((paragraph) => {
      const length = Array.from(paragraph).length;
      const xml = `<paragraph><content startOffset="${offset}" length="${length}" /></paragraph>`;
      offset += length + 1;
      return xml;
    }).join("");
    const xml = `<?xml version="1.0" encoding="UTF-8"?><template format_id="1.8"><content><![CDATA[${content}]]></content><elements>${elements}</elements></template>`;
    return new File([zipSync({ "content.xml": strToU8(xml) })], "degisik.udf");
  }

  it("çıkarılan metin panelde bozulmadan okunur", async () => {
    const base = await docxBlocks([
      ["T.C. İSTANBUL 3. ASLİYE HUKUK MAHKEMESİ'NE"],
      ["Davalı ABC A.Ş.'nin kira borcu aylık 18.", "000 TL olup %18 KDV hariçtir."],
      ["Fazlaya ilişkin haklarımız (HMK md.119/1-e) saklıdır."],
    ]);
    const revised = await extractDocument(udfFile([
      "T.C. İSTANBUL 3. ASLİYE HUKUK MAHKEMESİ'NE",
      "Davalı ABC A.Ş.'nin kira borcu aylık 18.500 TL olup %20 KDV hariçtir.",
      "Fazlaya ilişkin haklarımız (HMK md.119/1-f) saklıdır.",
    ]));
    expect(revised.extension).toBe("udf");
    expect(base[1].text).toBe("Davalı ABC A.Ş.'nin kira borcu aylık 18.000 TL olup %18 KDV hariçtir.");

    const comparison = compareDocuments(base, revised.blocks);
    // İki paragraf tek karşılaştırma satırında (bileşik satır) eşleşir; satır
    // metni kaynak paragrafların tek boşlukla birleşimidir.
    expect(comparison.rows.map((row) => row.kind)).toEqual(["unchanged", "modified"]);
    expect(comparison.rows[1].composite).toBe(true);
    expect(paneRows(comparison, "base")).toEqual(comparison.rows.map((row) => row.base!.text));
    expect(paneRows(comparison, "revised")).toEqual(comparison.rows.map((row) => row.revised!.text));
    expect(paneRows(comparison, "revised")[1]).toBe(
      "Davalı ABC A.Ş.'nin kira borcu aylık 18.500 TL olup %20 KDV hariçtir. Fazlaya ilişkin haklarımız (HMK md.119/1-f) saklıdır.",
    );
    expect(paneRows(comparison, "base")[1]).toBe(
      "Davalı ABC A.Ş.'nin kira borcu aylık 18.000 TL olup %18 KDV hariçtir. Fazlaya ilişkin haklarımız (HMK md.119/1-e) saklıdır.",
    );
    // Bileşik satır cümlelere bölünür; bölme "18.000"in noktasında
    // yapılırsa özet tutarı "000" → "500" diye yazar.
    expect(comparison.changes.flatMap((change) => change.summary)).toContain("“18.000” → “18.500”");
    expect(paneHtml(comparison, "revised")).toContain('<ins class="fragment added">18.500');
  });
});
