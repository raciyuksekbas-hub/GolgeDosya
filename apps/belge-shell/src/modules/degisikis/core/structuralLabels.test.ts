import { DOMParser } from "@xmldom/xmldom";
import { strToU8, zipSync } from "fflate";
import mammoth from "mammoth";
import { describe, expect, it } from "vitest";
import { copyChangesText } from "../uiLabels";
import { compareDocuments } from "./compare";
import { htmlEntriesFromRoot } from "./extractors";
import { displayLocation, makeBlocks } from "./normalize";

function syntheticArtDocx(items: string[]): Uint8Array {
  const paragraph = (text: string, numbered = false) => {
    const escaped = text.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
    const numbering = numbered
      ? '<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>'
      : "";
    return `<w:p>${numbering}<w:r><w:t>${escaped}</w:t></w:r></w:p>`;
  };
  const body = [
    ...Array.from({ length: 8 }, (_, index) => paragraph(`Başlangıç bölümü ${index + 1}.`)),
    paragraph("MADDE 3 – DEVİR PROTOKOLÜ HÜKÜMLERİ"),
    ...items.map((item) => paragraph(item, true)),
  ].join("");
  const document = `<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`;
  const numbering = `<?xml version="1.0" encoding="UTF-8"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>`;
  const contentTypes = `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>`;
  const relationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
  const documentRelationships = `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>`;
  return zipSync({
    "[Content_Types].xml": strToU8(contentTypes),
    "_rels/.rels": strToU8(relationships),
    "word/document.xml": strToU8(document),
    "word/numbering.xml": strToU8(numbering),
    "word/_rels/document.xml.rels": strToU8(documentRelationships),
  });
}

async function artEntries(items: string[]) {
  const html = (await mammoth.convertToHtml({ buffer: Buffer.from(syntheticArtDocx(items)) })).value;
  const root = new DOMParser().parseFromString(
    `<body>${html}</body>`,
    "text/xml",
  ).documentElement;
  return htmlEntriesFromRoot(root);
}

describe("kaynak belge numaralandırmasının yapısal etiketleri", () => {
  it("ART benzeri DOCX listesinde global satır sırası yerine Madde 3 bentlerini gösterir", async () => {
    const unchanged = "Sözleşme süresi tarafların mutabakatıyla yeniden belirlenir.";
    const base = makeBlocks(await artEntries([
      "HOL'e ait tüm hak ve yükümlülükler ART'a devredilir.",
      "HOL'ün doğmuş borcu bulunmadığı kabul edilir ve HOL ibra edilir.",
      "ART Esas Sözleşme hükümlerini tek taraflı tadil edebilir.",
      "Damga vergisi dahil masraflar yarı yarıya ödenir.",
      unchanged,
    ]), "base");
    const revised = makeBlocks(await artEntries([
      "HOL'e ait tüm hak, yükümlülük ve teminatlar ART'a devredilir.",
      "Şirket'in HOL'ü ibrası sınırlıdır ve taraflar müteselsilen sorumludur.",
      "Esas Sözleşme ancak uygun kabul, tadil veya ek protokol ile değiştirilebilir.",
      "Damga vergisi ve diğer masraflar taraflarca yarı yarıya ödenir.",
      unchanged,
    ]), "revised");
    const result = compareDocuments(base, revised);
    const changedLocations = new Set(result.changes.map((change) => change.location));

    expect(base.slice(-5).map((block) => [block.sourceListOrdinal, block.location])).toEqual([
      ["1", "Madde 3/1"],
      ["2", "Madde 3/2"],
      ["3", "Madde 3/3"],
      ["4", "Madde 3/4"],
      ["5", "Madde 3/5"],
    ]);
    expect(changedLocations).toEqual(new Set(["Madde 3/1", "Madde 3/2", "Madde 3/3", "Madde 3/4"]));
    expect(result.rows.find((row) => row.base?.location === "Madde 3/5")?.kind).toBe("unchanged");
    expect(result.changes.some((change) => /(?:9|10|11|12|13|14)\. liste öğesi/u.test(change.location))).toBe(false);

    const copiedLines = copyChangesText(result.changes).split("\n");
    expect(copiedLines).toHaveLength(result.changes.length);
    expect(new Set(copiedLines).size).toBe(copiedLines.length);
  });

  it("yalnız kaynakta bulunan ordered-list numarasını gösterir, bullet için sayı uydurmaz", () => {
    const root = new DOMParser().parseFromString(
      "<body><ol start=\"7\"><li>Numaralı içerik</li></ol><ul><li>İşaretli içerik</li></ul></body>",
      "text/xml",
    ).documentElement;
    const blocks = makeBlocks(htmlEntriesFromRoot(root));

    expect(blocks[0]).toMatchObject({ sourceListOrdinal: "7", location: "7. numaralı bölüm" });
    expect(blocks[1].sourceListOrdinal).toBeUndefined();
    expect(displayLocation(blocks[1], 27)).toBe("Liste öğesi");
  });
});
