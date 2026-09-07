import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DOMParser as NodeDomParser } from "@xmldom/xmldom";
import { compareDocuments } from "./compare";
import { makeBlocks } from "./normalize";
import { udfXmlEntries } from "./udf";

const browserDomParser = globalThis.DOMParser;

beforeAll(() => {
  globalThis.DOMParser = NodeDomParser as unknown as typeof DOMParser;
});

afterAll(() => {
  globalThis.DOMParser = browserDomParser;
});

describe("v0.3.1 hedefli structural accuracy", () => {
  const splitText = "Sağlayıcı günlük yedekleme yapar. Yedekler 30 gün saklanır.";
  const splitParts = ["Sağlayıcı günlük yedekleme yapar.", "Yedekler 30 gün saklanır."];

  it("aynı yerel maddede 1→2 paragraph split sıfır farktır", () => {
    const base = makeBlocks([{ text: "MADDE 3 – YEDEKLEME" }, { text: `3.5. ${splitText}` }], "base");
    const revised = makeBlocks([
      { text: "MADDE 3 – YEDEKLEME" },
      { text: `3.5. ${splitParts[0]}` },
      { text: splitParts[1] },
    ], "revised");
    expect(compareDocuments(base, revised).changes).toHaveLength(0);
  });

  it("aynı yerel maddede 2→1 paragraph merge sıfır farktır", () => {
    const base = makeBlocks([
      { text: "MADDE 3 – YEDEKLEME" },
      { text: `3.5. ${splitParts[0]}` },
      { text: splitParts[1] },
    ], "base");
    const revised = makeBlocks([{ text: "MADDE 3 – YEDEKLEME" }, { text: `3.5. ${splitText}` }], "revised");
    expect(compareDocuments(base, revised).changes).toHaveLength(0);
  });

  it("liste içeriğinin tek paragrafa dönüşmesi syntax farkı üretmez", () => {
    const items = [
      "Veri güvenliği tedbirlerinin alınması",
      "Yedekleme prosedürlerinin uygulanması",
      "Erişim kontrolü mekanizmalarının kurulması",
    ];
    const base = makeBlocks(items.map((text) => ({ text: `- ${text}`, kind: "list" as const })), "base");
    const revised = makeBlocks([{ text: `${items[0]}, ${items[1]} ve ${items[2]}.` }], "revised");
    expect(compareDocuments(base, revised).changes).toHaveLength(0);
  });

  it("aynı yükümlülükler intro paragrafına taşınınca eski liste deletion üretmez", () => {
    const base = makeBlocks([
      { text: "MADDE 8 – GİZLİLİK" },
      { text: "8.2. Sağlayıcı, gizlilik yükümlülükleri çerçevesinde şu güvenlik tedbirlerini almakla yükümlüdür:" },
      { text: "- Veri güvenliği tedbirlerinin alınması" },
      { text: "- Yedekleme prosedürlerinin uygulanması" },
      { text: "- Erişim kontrolü mekanizmalarının kurulması" },
    ], "base");
    const revised = makeBlocks([
      { text: "MADDE 8 – GİZLİLİK" },
      { text: "8.2. Sağlayıcı, gizlilik yükümlülükleri çerçevesinde veri güvenliği tedbirlerinin alınması, yedekleme prosedürlerinin uygulanması ve erişim kontrolü mekanizmalarının kurulması tedbirlerini almakla yükümlüdür." },
    ], "revised");
    const result = compareDocuments(base, revised);
    expect(result.changes.filter((change) => change.kind === "removed" && /Veri güvenliği|Yedekleme prosedürleri|Erişim kontrolü/u.test(change.baseText ?? ""))).toHaveLength(0);
  });

  it("aynı EK içinde dar penceredeki exact move add+delete üretmez", () => {
    const base = makeBlocks([
      { text: "EK-3" },
      { text: "2.1. Devir planı hazırlanır." },
      { text: "2.2. Eğitim takvimi bildirilir." },
      { text: "2.3. Eğitim oturumları video kaydına alınır." },
    ], "base");
    const revised = makeBlocks([
      { text: "EK-3" },
      { text: "2.1. Devir planı hazırlanır." },
      { text: "2.3. Eğitim oturumları video kaydına alınır." },
      { text: "2.2. Eğitim takvimi bildirilir." },
    ], "revised");
    expect(compareDocuments(base, revised).changes).toHaveLength(0);
  });

  it("araya numbered item eklenince yalnız yeni item görünür", () => {
    const base = makeBlocks(["1. A hizmeti", "2. B hizmeti", "3. C hizmeti"].map((text) => ({ text })), "base");
    const revised = makeBlocks(["1. A hizmeti", "2. X hizmeti", "3. B hizmeti", "4. C hizmeti"].map((text) => ({ text })), "revised");
    const result = compareDocuments(base, revised);
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0]).toMatchObject({ kind: "added", revisedText: "2. X hizmeti" });
    expect(result.changes.flatMap((change) => change.summary).join(" ")).not.toMatch(/[23]\s*→\s*[34]/u);
  });

  it("araya tablo satırı eklenince row number değişimleri görünmez", () => {
    const base = makeBlocks([
      { text: "1 | A hizmeti | 10.000 TL", kind: "table" },
      { text: "2 | B hizmeti | 20.000 TL", kind: "table" },
      { text: "3 | C hizmeti | 30.000 TL", kind: "table" },
    ], "base");
    const revised = makeBlocks([
      { text: "1 | A hizmeti | 10.000 TL", kind: "table" },
      { text: "2 | X hizmeti | 35.000 TL", kind: "table" },
      { text: "3 | B hizmeti | 20.000 TL", kind: "table" },
      { text: "4 | C hizmeti | 30.000 TL", kind: "table" },
    ], "revised");
    const result = compareDocuments(base, revised);
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].revisedText).toContain("35.000 TL");
    expect(result.changes.flatMap((change) => change.summary).join(" ")).not.toMatch(/[23]\s*→\s*[34]/u);
  });

  it("Amazon common-spine özetinde ayrı hunklardan sahte replacement kurmaz", () => {
    const short = "değişiminin sağlanarak, yeni Ürün’üb 31.08.2023 tarihine kadar Tüketici’ye gönderilmesi";
    const long = "öncelikle Xiaomi Mi 10 olarak ayıpsız bir benzeriyle değişiminin sağlanarak, yeni Ürün’ün 31.08.2023 tarihine kadar Tüketici’nin adresine kargo masrafları Amazon’a ait olmak üzere gönderilmesi";
    const result = compareDocuments(makeBlocks([{ text: short }], "base"), makeBlocks([{ text: long }], "revised"), { trace: true });
    const summary = result.changes.flatMap((change) => change.summary).join(" ");
    expect(summary).not.toContain("“Tüketici’ye” → “Ürün’ün”");
    expect(result.trace!.rows.flatMap((row) => row.tokenDiffHunks).every((hunk) => (
      !hunk.removed || !hunk.added || hunk.replacementReason === "same-atomic-pair"
    ))).toBe(true);
  });

  it("UDF aynı hücrede bölünmüş 35.000 TL değerini tek satırda birleştirir", () => {
    const content = "Mobil Uygulama Bakımı3 Ay35.000 TL";
    const xml = `<?xml version="1.0"?><template><content><![CDATA[${content}]]></content><elements><table><row><cell><paragraph><content startOffset="0" length="21" /></paragraph></cell><cell><paragraph><content startOffset="21" length="4" /></paragraph></cell><cell><paragraph><content startOffset="25" length="3" /></paragraph><paragraph><content startOffset="28" length="6" /></paragraph></cell></row></table></elements></template>`;
    expect(udfXmlEntries(xml)).toEqual([{ text: "Mobil Uygulama Bakımı | 3 Ay | 35.000 TL", kind: "table" }]);
  });

  it("aynı tutar değişikliğinin her source occurrence'ını ayrı provenance ile korur", () => {
    const base = makeBlocks(Array.from({ length: 3 }, (_, index) => ({ text: `${index + 1}. Ödenecek tutar 95.000 TL'dir.` })), "base");
    const revised = makeBlocks(Array.from({ length: 3 }, (_, index) => ({ text: `${index + 1}. Ödenecek tutar 93.000 TL'dir.` })), "revised");
    const result = compareDocuments(base, revised, { trace: true });
    expect(result.changes).toHaveLength(3);
    expect(new Set(result.changes.map((change) => change.provenance?.baseSourceBlockIds.join("+"))).size).toBe(3);
    expect(result.changes.flatMap((change) => change.summary).every((line) => {
      const match = line.match(/^“([^”]+)” → “([^”]+)”$/u);
      return !match || match[1] !== match[2];
    })).toBe(true);
  });
});
