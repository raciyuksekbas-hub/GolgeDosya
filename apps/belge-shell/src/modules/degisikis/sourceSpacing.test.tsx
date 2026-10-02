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

function compareParagraphs(base: string[], revised: string[]): ComparisonResult {
  return compareDocuments(
    makeBlocks(base.map((text) => ({ text })), "base"),
    makeBlocks(revised.map((text) => ({ text })), "revised"),
  );
}

// Yalnız boşluğu farklı iki yazım aynı sözcüklerdir; sözcük farkı onları eşit
// sayar ve panelde işaretlenecek bir şey yoktur. Parça metni kaynaktan
// kesildikten sonra özet tutar, süre ve yetkili mahkeme ifadelerini bu ham
// metinle karşılaştırıyordu: "18.000 TL" ile "18.000TL" farklı çıkıyor, satır
// "değişti" sayılıyor, rayda bir değişiklik görünüyor ama iki panelde de
// hiçbir işaret yoktu. Eşitlik sözcüklerle kurulur, gösterim kaynaktan gelir.
const SPACING_ONLY: Array<[string, string]> = [
  ["Kira bedeli 18.000 TL'dir.", "Kira bedeli 18.000TL'dir."],
  ["Toplam 1.250,50 TL ödenecektir.", "Toplam 1.250,50TL ödenecektir."],
  ["Bedel 500 USD olarak ödenir.", "Bedel 500USD olarak ödenir."],
  ["Fesih bildirimi 30 (otuz) gün önceden yapılır.", "Fesih bildirimi 30(otuz) gün önceden yapılır."],
  ["Uyuşmazlıklarda İstanbul Anadolu Mahkemeleri yetkilidir.", "Uyuşmazlıklarda İstanbul\tAnadolu Mahkemeleri yetkilidir."],
];

describe("yalnız boşluk farkı değişiklik sayılmaz", () => {
  it.each(SPACING_ONLY)("%s ↔ %s", (first, second) => {
    for (const [base, revised] of [[first, second], [second, first]]) {
      const comparison = compareParagraphs([base], [revised]);
      expect(comparison.rows.map((row) => row.kind)).toEqual(["unchanged"]);
      expect(comparison.rawChanges).toEqual([]);
      expect(comparison.changes).toEqual([]);
      expect(paneRows(comparison, "base")).toEqual([base]);
      expect(paneRows(comparison, "revised")).toEqual([revised]);
    }
  });

  const summaries = (comparison: ComparisonResult) => comparison.changes.map((change) => change.summary);

  it("gerçek değişikliğin yanındaki boşluk farkı özete girmez", () => {
    const base = ["Kira bedeli 18.000 TL olup 5 (beş) gün içinde ödenir."];
    const glued = compareParagraphs(base, ["Kira bedeli 18.000TL olup 7 (yedi) gün içinde ödenir."]);
    const spaced = compareParagraphs(base, ["Kira bedeli 18.000 TL olup 7 (yedi) gün içinde ödenir."]);
    expect(summaries(glued)).toEqual(summaries(spaced));
    expect(summaries(glued)[0][0]).toBe("“5 (beş) gün” → “7 (yedi) gün”");
    expect(summaries(glued).flat().filter((line) => line.includes("18.000"))).toEqual([]);
  });

  // Süre ifadeleri iki tarafta sayılıp sırayla eşleştirilir. Önündeki sözcüğe
  // bitişik "Kiracı30 (otuz) gün" ham metinde süre sayılmıyordu; değişik
  // tarafta tek süre kalıyor ve özet, yerinde duran 30 günü yeni eklenen
  // 15 iş gününe "değişti" diye bağlıyordu.
  it("sözcüğe bitişik yazılmış süre başka bir süreyle eşleştirilmez", () => {
    const base = ["Kiracı 30 (otuz) gün içinde tahliye eder."];
    const glued = compareParagraphs(base, ["Kiracı30 (otuz) gün içinde, en geç 15 (onbeş) iş günü sonra tahliye eder."]);
    const spaced = compareParagraphs(base, ["Kiracı 30 (otuz) gün içinde, en geç 15 (onbeş) iş günü sonra tahliye eder."]);
    expect(summaries(glued)).toEqual(summaries(spaced));
    expect(summaries(glued).flat().filter((line) => line.startsWith("“30 (otuz) gün” →"))).toEqual([]);
  });

  // Tutar özeti iki yoldan çıkar: tutar ifadesi belgedeki yazımla ("18.500TL"),
  // sayı adayı sözcüklerden ("18.500 TL"). İkisi aynı değişikliktir.
  it("aynı tutar değişikliği özette bir kez, belgedeki yazımıyla geçer", () => {
    const comparison = compareParagraphs(["Kira bedeli 18.000 TL'dir."], ["Kira bedeli 18.500TL'ydi."]);
    expect(comparison.changes.flatMap((change) => change.summary)).toEqual(["“18.000 TL” → “18.500TL”"]);
  });

  // Birden çok parçalı değişiklikte kart özeti satırın genel özetiyle her
  // parçanın kendi özetini birleştirir. Biri sözcüklerden, öteki kaynaktan
  // yazıldığı için aynı ekleme iki kez çıkıyordu: "md. 119/1 - e" ve
  // "md.119/1-e".
  it("çok parçalı değişiklikte aynı ekleme özette bir kez geçer", () => {
    const comparison = compareParagraphs(
      ["Davalı ödeme yapmadı ve dava açıldı."],
      ["Davalı HMK md.119/1-e uyarınca ödeme yapmadı ve dava bugün açıldı."],
    );
    expect(comparison.changes).toHaveLength(1);
    expect(comparison.changes[0].summary.filter((line) => line.includes("119/1"))).toHaveLength(1);
    expect(comparison.changes[0].summary.filter((line) => line.includes("bugün"))).toHaveLength(1);
  });

  // Sahada görülen bileşik satır: iki paragraf tek paragrafa birleşmiş, tutar
  // değişmiş. Eskiden cümle bölücü "17.500"ün noktasında da bölüyor, panel
  // "aylık 17 .   500 TL olup% 18" gösteriyor, özet "17" → "18" diyordu.
  it("iki paragraf birleşince tutar bölünmez; panel kaynağı, özet tutarı gösterir", () => {
    const base = [
      "Davalı ABC A.Ş.'nin kira borcu aylık 17.500 TL olup %18 KDV hariçtir.",
      "Fazlaya ilişkin haklarımız (HMK md. 119/1-e) saklıdır.",
    ];
    const revised = "Davalı ABC A.Ş.'nin kira borcu aylık 18.500 TL olup %18 KDV hariçtir. Fazlaya ilişkin haklarımız (HMK md. 119/1-e) saklıdır.";
    const comparison = compareParagraphs(base, [revised]);
    expect(comparison.rows.map((row) => row.kind)).toEqual(["modified"]);
    expect(comparison.rows[0].composite).toBe(true);
    expect(paneRows(comparison, "base")).toEqual([base.join(" ")]);
    expect(paneRows(comparison, "revised")).toEqual([revised]);
    expect(comparison.changes.flatMap((change) => change.summary)).toEqual(["“17.500” → “18.500”"]);
    expect(paneHtml(comparison, "base")).toContain('<del class="fragment removed">17.500 </del>');
    expect(paneHtml(comparison, "revised")).toContain('<ins class="fragment added">18.500 </ins>');
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
