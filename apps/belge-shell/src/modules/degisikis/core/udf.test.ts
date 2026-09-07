import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DOMParser as NodeDomParser } from "@xmldom/xmldom";
import { compareDocuments } from "./compare";
import { makeBlocks } from "./normalize";
import { splitStructuralText, udfXmlEntries } from "./udf";

const browserDomParser = globalThis.DOMParser;

beforeAll(() => {
  globalThis.DOMParser = NodeDomParser as unknown as typeof DOMParser;
});

afterAll(() => {
  globalThis.DOMParser = browserDomParser;
});

function uyapXml(paragraphs: string[]): string {
  const content = paragraphs.join("\n");
  let offset = 0;
  const elements = paragraphs.map((paragraph) => {
    const length = Array.from(paragraph).length;
    const xml = `<paragraph><content startOffset="${offset}" length="${length}" /></paragraph>`;
    offset += length + 1;
    return xml;
  }).join("");
  return `<?xml version="1.0" encoding="UTF-8"?><template><content><![CDATA[${content}]]></content><elements>${elements}</elements></template>`;
}

describe("UYAP UDF ayrıştırıcısı", () => {
  it("offset tabanlı UDF içeriğini ayrı paragraflara böler", () => {
    const entries = udfXmlEntries(uyapXml([
      "KİRA SÖZLEŞMESİ",
      "Madde 1 Taraflar aşağıdaki koşulları kabul eder.",
      "Madde 2 Kira bedeli aylık 20.000 TL'dir.",
      "Madde 4 Bildirimler yazılı yapılır.",
    ]));

    expect(entries.map((entry) => entry.text)).toEqual([
      "KİRA SÖZLEŞMESİ",
      "Madde 1 Taraflar aşağıdaki koşulları kabul eder.",
      "Madde 2 Kira bedeli aylık 20.000 TL'dir.",
      "Madde 4 Bildirimler yazılı yapılır.",
    ]);
  });

  it("iki UDF'deki anlamlı farkları tek dev değişiklik yerine ayrı eşleştirir", () => {
    const base = makeBlocks(udfXmlEntries(uyapXml([
      "KİRA SÖZLEŞMESİ",
      "Madde 1 Taraflar aşağıdaki koşulları kabul eder.",
      "Madde 2 Kira bedeli aylık 20.000 TL'dir.",
      "Madde 4 Bildirimler yazılı yapılır.",
    ])), "base");
    const revised = makeBlocks(udfXmlEntries(uyapXml([
      "KİRA SÖZLEŞMESİ",
      "Madde 1 Taraflar aşağıdaki koşulları kabul eder.",
      "Madde 2 Kira bedeli aylık net 25.000 TL'dir.",
      "Madde 3 Depozito iki aylık kira bedelidir.",
    ])), "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes.map((change) => [change.kind, change.location])).toEqual([
      ["modified", "Madde 2"],
      ["added", "Madde 3"],
      ["removed", "Madde 4"],
    ]);
    expect(result.changes[0].summary).toEqual([
      "“20.000 TL” → “25.000 TL”",
      "+ “net”",
    ]);
  });

  it("yapısız eski UDF metninde satırları ve madde başlangıçlarını ayırır", () => {
    expect(splitStructuralText("Başlık\nMadde 1 Birinci hüküm.\nMadde 2 İkinci hüküm.")).toHaveLength(3);
  });

  it("tablo hücrelerini satır yapısını kaybetmeden çıkarır", () => {
    const xml = `<?xml version="1.0"?><template><content><![CDATA[Ücret30.000 TL]]></content><elements><table><row><cell><paragraph><content startOffset="0" length="5" /></paragraph></cell><cell><paragraph><content startOffset="5" length="9" /></paragraph></cell></row></table></elements></template>`;
    expect(udfXmlEntries(xml)).toEqual([{ text: "Ücret | 30.000 TL", kind: "table" }]);
  });
});
