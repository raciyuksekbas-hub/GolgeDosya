/**
 * Word otomatik numaralandırmasının GÖRÜNÜR sonucunu hesaplar.
 *
 * Madde numarası paragrafın metninde YOKTUR: Word onu `numbering.xml` +
 * paragrafın `w:numPr` bağından çizim anında üretir. Bu yüzden
 * "3.2. Sorumluluk" maddesi "4. Sorumluluk"a dönüştüğünde iki tarafın
 * çıkarılan metni birebir aynı ("Sorumluluk") oluyor ve karşılaştırma hiçbir
 * fark üretmiyordu. Hukuk belgesinde bu kabul edilemez: numara değişince
 * bütün çapraz atıflar ("madde 3.2'de belirtilen") yanlış maddeyi gösterir.
 *
 * Karşılaştırılan şey Word'ün İÇ KİMLİĞİ (`numId`) değil, kullanıcının
 * gördüğü SONUÇTUR. İki belgede aynı görünen numara farklı `numId`'lerden
 * geliyorsa fark üretilmez.
 */

/** Tek bir numaralandırma düzeyinin tanımı. */
interface LevelDef {
  format: string;
  /** `%1.%2.` gibi şablon. */
  text: string;
  start: number;
}

interface Numbering {
  /** `numId` → düzey tanımları. */
  byNumId: Map<string, Map<number, LevelDef>>;
}

const attr = (xml: string, name: string): string | undefined =>
  new RegExp(`${name}="([^"]*)"`, "u").exec(xml)?.[1];

/** `numbering.xml`'i çöz: soyut tanımlar ve onlara bağlı somut numaralar. */
export function parseNumbering(xml: string | undefined): Numbering {
  const byNumId = new Map<string, Map<number, LevelDef>>();
  if (!xml) return { byNumId };

  const abstracts = new Map<string, Map<number, LevelDef>>();
  for (const block of xml.match(/<w:abstractNum\b[\s\S]*?<\/w:abstractNum>/gu) ?? []) {
    const id = attr(block, "w:abstractNumId");
    if (!id) continue;
    const levels = new Map<number, LevelDef>();
    for (const lvl of block.match(/<w:lvl\b[\s\S]*?<\/w:lvl>/gu) ?? []) {
      const ilvl = Number(attr(lvl, "w:ilvl") ?? "0");
      levels.set(ilvl, {
        format: /<w:numFmt[^>]*w:val="([^"]*)"/u.exec(lvl)?.[1] ?? "decimal",
        text: /<w:lvlText[^>]*w:val="([^"]*)"/u.exec(lvl)?.[1] ?? "%1.",
        start: Number(/<w:start[^>]*w:val="(\d+)"/u.exec(lvl)?.[1] ?? "1"),
      });
    }
    abstracts.set(id, levels);
  }

  for (const block of xml.match(/<w:num\b[\s\S]*?<\/w:num>/gu) ?? []) {
    const numId = attr(block, "w:numId");
    const abstractId = /<w:abstractNumId[^>]*w:val="([^"]*)"/u.exec(block)?.[1];
    if (!numId || !abstractId) continue;
    const base = abstracts.get(abstractId);
    if (!base) continue;
    // `startOverride` somut numaraya aittir; soyut tanım paylaşılabilir.
    const levels = new Map(base);
    for (const over of block.match(/<w:lvlOverride\b[\s\S]*?<\/w:lvlOverride>/gu) ?? []) {
      const ilvl = Number(attr(over, "w:ilvl") ?? "0");
      const start = /<w:startOverride[^>]*w:val="(\d+)"/u.exec(over)?.[1];
      const def = levels.get(ilvl);
      if (def && start) levels.set(ilvl, { ...def, start: Number(start) });
    }
    byNumId.set(numId, levels);
  }
  return { byNumId };
}

const ROMAN: [number, string][] = [
  [1000, "m"], [900, "cm"], [500, "d"], [400, "cd"], [100, "c"], [90, "xc"],
  [50, "l"], [40, "xl"], [10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"],
];

function roman(value: number): string {
  let left = value;
  let out = "";
  for (const [size, sign] of ROMAN) {
    while (left >= size) {
      out += sign;
      left -= size;
    }
  }
  return out;
}

/** Sayacı düzeyin biçimine göre yazıya çevirir. */
export function formatCounter(value: number, format: string): string {
  switch (format) {
    case "lowerLetter":
      return String.fromCharCode(96 + ((value - 1) % 26) + 1);
    case "upperLetter":
      return String.fromCharCode(64 + ((value - 1) % 26) + 1);
    case "lowerRoman":
      return roman(value);
    case "upperRoman":
      return roman(value).toUpperCase();
    case "decimalZero":
      return value < 10 ? `0${value}` : String(value);
    default:
      return String(value);
  }
}

/** Numarasız biçimler: madde numarası taşımazlar. */
const UNNUMBERED = new Set(["bullet", "none"]);

/**
 * Belgedeki numaralandırılmış paragrafların GÖRÜNÜR etiketlerini, belge
 * sırasına göre üretir.
 *
 * Sayaçlar `numId` başına tutulur; bir düzey artınca ondan DERİN düzeyler
 * sıfırlanır — Word'ün davranışı budur.
 */
export function visibleLabels(documentXml: string, numbering: Numbering): (string | undefined)[] {
  const counters = new Map<string, number[]>();
  const labels: (string | undefined)[] = [];

  for (const para of documentXml.match(/<w:p\b[\s\S]*?<\/w:p>/gu) ?? []) {
    const numPr = /<w:numPr\b[\s\S]*?<\/w:numPr>/u.exec(para)?.[0];
    if (!numPr) {
      labels.push(undefined);
      continue;
    }
    const numId = /<w:numId[^>]*w:val="([^"]*)"/u.exec(numPr)?.[1];
    const ilvl = Number(/<w:ilvl[^>]*w:val="(\d+)"/u.exec(numPr)?.[1] ?? "0");
    const levels = numId ? numbering.byNumId.get(numId) : undefined;
    const def = levels?.get(ilvl);
    if (!numId || !def || UNNUMBERED.has(def.format)) {
      labels.push(undefined);
      continue;
    }

    const state = counters.get(numId) ?? [];
    while (state.length <= ilvl) state.push(0);
    if (state[ilvl] === 0) {
      const start = levels?.get(ilvl)?.start ?? 1;
      state[ilvl] = start;
    } else {
      state[ilvl] += 1;
    }
    // Derin düzeyler sıfırlanır: 4. maddeden sonra 4.1 yeniden başlar.
    for (let deeper = ilvl + 1; deeper < state.length; deeper += 1) state[deeper] = 0;
    counters.set(numId, state);

    // `%1.%2.` şablonunu doldur.
    const label = def.text.replace(/%(\d)/gu, (_, digit: string) => {
      const index = Number(digit) - 1;
      const levelFormat = levels?.get(index)?.format ?? "decimal";
      return formatCounter(state[index] || 1, levelFormat);
    });
    labels.push(label.trim() || undefined);
  }
  return labels;
}

/**
 * Görünür etiketi paragrafın METNİNE yazar.
 *
 * Etiket gerçek metne dönüşünce hem karşılaştırmaya girer hem de mevcut
 * yapısal önek makinesi (`splitStructuralPrefix`) onu zaten tanıdığı biçimde
 * görür. Ayrı bir karşılaştırma yolu açmaya gerek kalmaz.
 *
 * Paragraf zaten o etiketle başlıyorsa (kullanıcı numarayı elle yazmışsa)
 * ikinci kez yazılmaz.
 */
export function injectVisibleNumbering(documentXml: string, numberingXml: string | undefined): string {
  const numbering = parseNumbering(numberingXml);
  if (numbering.byNumId.size === 0) return documentXml;
  const labels = visibleLabels(documentXml, numbering);
  let index = -1;
  return documentXml.replace(/<w:p\b[\s\S]*?<\/w:p>/gu, (para) => {
    index += 1;
    const label = labels[index];
    if (!label) return para;
    const text = (para.match(/<w:t[^>]*>([^<]*)<\/w:t>/gu) ?? [])
      .map((m) => m.replace(/<[^>]+>/gu, ""))
      .join("")
      .trimStart();
    if (text.startsWith(label)) return para;
    const run = `<w:r><w:t xml:space="preserve">${label} </w:t></w:r>`;
    // İlk çalıştırılabilir içeriğin önüne: `w:pPr` varsa ondan sonra.
    const pPrEnd = para.indexOf("</w:pPr>");
    if (pPrEnd >= 0) {
      const at = pPrEnd + "</w:pPr>".length;
      return para.slice(0, at) + run + para.slice(at);
    }
    const open = para.indexOf(">");
    return para.slice(0, open + 1) + run + para.slice(open + 1);
  });
}
