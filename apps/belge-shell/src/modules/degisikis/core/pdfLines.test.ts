import { describe, expect, it } from "vitest";
import { pageLines, pdfEntries } from "./extractors";
import { makeBlocks } from "./normalize";
import { compareDocuments } from "./compare";

const CHAR = 5;      // average glyph advance of the synthetic 10pt face
const PITCH = 12;    // baseline-to-baseline distance of the synthetic page
const LEFT = 72;     // left text margin
const COLUMN = 451;  // width of the text column

type TextItem = { str: string; transform: number[]; width: number };

const runAt = (text: string, x: number, y: number): TextItem => ({
  str: text,
  transform: [1, 0, 0, 1, x, y],
  width: text.length * CHAR,
});

/** A left-aligned line that wraps, i.e. one that runs out to the right margin. */
const wrappedLine = (text: string, y: number): TextItem => ({
  str: text,
  transform: [1, 0, 0, 1, LEFT, y],
  width: COLUMN,
});

const entriesFor = (pages: TextItem[][]): string[] =>
  pdfEntries(pages.map((items, index) => pageLines(items, index + 1))).map((entry) => entry.text);

/** Body copy long enough for the dominant pitch to be established, as in a real page. */
const body = (top: number): TextItem[] => [
  wrappedLine("Taraflar arasındaki uyuşmazlık hakkında 6325 sayılı Kanun uyarınca", top),
  wrappedLine("arabuluculuk süreci yürütülmüş, taraflar bir araya gelerek görüşmüş", top - PITCH),
  wrappedLine("ve aşağıdaki hususlarda anlaşmaya varmışlardır. İşbu tutanak taraf", top - PITCH * 2),
  wrappedLine("sayısınca nüsha olarak düzenlenmiş ve taraflarca imza altına alınmış", top - PITCH * 3),
  runAt("olup bir örneği arabulucuda kalacaktır.", LEFT, top - PITCH * 4),
];

/**
 * The user-reported layout: the mediator's name is separated from the title below it by a
 * blank line, and the title runs straight into the next signatory's line.
 */
const signature = (surname: string): TextItem[] => [
  ...body(700),
  runAt(`AV. Yunus Emre ${surname}`, LEFT, 600),
  runAt("ARABULUCU", LEFT, 600 - PITCH * 2),
  runAt("ARB. AV. EDA YILMAZ", LEFT, 600 - PITCH * 3),
];

describe("PDF görsel satır sınırları", () => {
  it("aynı baseline üzerindeki ayrı text run'ları tek görsel satır yapar", () => {
    const lines = pageLines([
      runAt("AV. Yunus", 250, 600),
      runAt("Emre", 300, 600),
      runAt("EROL", 330, 600),
      runAt("ARABULUCU", 275, 588),
    ], 1);

    expect(lines.map((line) => line.text)).toEqual(["AV. Yunus Emre EROL", "ARABULUCU"]);
  });

  it("sonraki baseline'ı azalan y sırasıyla ayrı görsel satır tutar", () => {
    const [first, second] = pageLines([
      runAt("EROL", 330, 600),
      runAt("AV. Yunus", 250, 600),
      runAt("ARABULUCU", 275, 588),
    ], 1);

    expect(first.text).toBe("AV. Yunus EROL");
    expect(first.y).toBeGreaterThan(second.y);
    expect(second.text).toBe("ARABULUCU");
  });

  it("boş satırla ayrılan imza satırını gövdeyle veya unvanla birleştirmez", () => {
    // The blank line spans exactly 2 x PITCH = 24 units. The previous fixed "> 24"
    // threshold could not separate it, so the name absorbed the title below it.
    const entries = entriesFor([signature("EROL")]);

    expect(entries).toContain("AV. Yunus Emre EROL");
    expect(entries.some((entry) => /EROL\s+ARABULUCU/u.test(entry))).toBe(false);
  });

  it("EROL → EROLL farkını tek maddi değişiklik olarak korur ve yanlış pozitif üretmez", () => {
    const blocksFor = (surname: string, prefix: string) =>
      makeBlocks(pdfEntries([pageLines(signature(surname), 1)]), prefix);
    const result = compareDocuments(blocksFor("EROL", "base"), blocksFor("EROLL", "revised"));

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].baseText).toContain("EROL");
    expect(result.changes[0].revisedText).toContain("EROLL");
    expect(result.rows.filter((row) => row.kind !== "unchanged")).toHaveLength(1);
  });

  it("üç fiziksel satıra sarmalanan paragrafı tek bloğa toplar", () => {
    const entries = entriesFor([[
      wrappedLine("Taraflar, işbu protokolün imzalanmasından itibaren doğacak her türlü", 700),
      wrappedLine("uyuşmazlığın çözümünde İstanbul mahkemelerinin ve icra dairelerinin", 700 - PITCH),
      runAt("yetkili olduğunu kabul ederler.", LEFT, 700 - PITCH * 2),
    ]]);

    expect(entries).toEqual([
      "Taraflar, işbu protokolün imzalanmasından itibaren doğacak her türlü"
      + " uyuşmazlığın çözümünde İstanbul mahkemelerinin ve icra dairelerinin"
      + " yetkili olduğunu kabul ederler.",
    ]);
  });

  it("geniş satır aralıklı belgede sarmalanan paragrafı satır satır bölmez", () => {
    // Pitch 28 exceeds the old fixed threshold, which split every wrapped line of a
    // large-type document into its own block.
    const wide = 28;
    const entries = entriesFor([[
      wrappedLine("Taraflar, işbu protokolün imzalanmasından itibaren doğacak her türlü", 700),
      wrappedLine("uyuşmazlığın çözümünde İstanbul mahkemelerinin ve icra dairelerinin", 700 - wide),
      runAt("yetkili olduğunu kabul ederler.", LEFT, 700 - wide * 2),
    ]]);

    expect(entries).toHaveLength(1);
  });

  it("sarmalanan paragrafı ayrı kartlara bölmez", () => {
    const wrapped = (): TextItem[] => [
      wrappedLine("Taraflar, işbu protokolün imzalanmasından itibaren doğacak her türlü", 700),
      wrappedLine("uyuşmazlığın çözümünde İstanbul mahkemelerinin ve icra dairelerinin", 700 - PITCH),
      runAt("yetkili olduğunu kabul ederler.", LEFT, 700 - PITCH * 2),
    ];
    const result = compareDocuments(
      makeBlocks(pdfEntries([pageLines(wrapped(), 1)]), "base"),
      makeBlocks(pdfEntries([pageLines(wrapped(), 1)]), "revised"),
    );

    expect(result.changes).toHaveLength(0);
  });

  it("sayfa sınırında devam eden paragrafta add/delete üretmez", () => {
    const spread = (): TextItem[][] => [
      [
        wrappedLine("Taraflar, işbu protokolün imzalanmasından itibaren doğacak her türlü", 700),
        wrappedLine("uyuşmazlığın çözümünde İstanbul mahkemelerinin yetkili olduğunu ve", 700 - PITCH),
      ],
      [
        wrappedLine("bu yetkinin münhasır olmadığını, ayrıca tarafların dava açmadan önce", 700),
        runAt("arabuluculuğa başvuracağını kabul ederler.", LEFT, 700 - PITCH),
      ],
    ];
    const toBlocks = (prefix: string) =>
      makeBlocks(pdfEntries(spread().map((items, index) => pageLines(items, index + 1))), prefix);
    const result = compareDocuments(toBlocks("base"), toBlocks("revised"));

    expect(result.changes).toHaveLength(0);
    expect(result.rows.every((row) => row.kind === "unchanged")).toBe(true);
  });

  it("tekrarlayan üstbilgi ve sayfa numarası bastırmasını korur", () => {
    const page = (line: string): TextItem[] => [
      runAt("ARABULUCULUK MERKEZİ · GİZLİ", LEFT, 780),
      wrappedLine(line, 700),
      runAt("3", 295, 60),
    ];
    const entries = entriesFor([
      page("Birinci sayfadaki esas hüküm metni burada yer almaktadır ve devam eder."),
      page("İkinci sayfadaki esas hüküm metni burada yer almaktadır ve devam eder."),
      page("Üçüncü sayfadaki esas hüküm metni burada yer almaktadır ve devam eder."),
    ]);

    expect(entries.some((entry) => entry.includes("ARABULUCULUK MERKEZİ"))).toBe(false);
    expect(entries.some((entry) => /(?:^|\s)3(?:\s|$)/u.test(entry))).toBe(false);
    expect(entries).toHaveLength(3);
  });

  it("ardışık imza satırlarını da ayrı görsel satır olarak korur", () => {
    const entries = entriesFor([signature("EROL")]);

    expect(entries).toContain("ARABULUCU");
    expect(entries).toContain("ARB. AV. EDA YILMAZ");
  });

  it("imza bloğunun tamamını ayrı satırlar hâlinde verir", () => {
    expect(entriesFor([signature("EROL")]).slice(-3)).toEqual([
      "AV. Yunus Emre EROL",
      "ARABULUCU",
      "ARB. AV. EDA YILMAZ",
    ]);
  });
});
