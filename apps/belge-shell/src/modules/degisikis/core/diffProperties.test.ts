import { describe, expect, it } from "vitest";
import { compareDocuments } from "./compare";
import { makeBlocks } from "./normalize";

function seeded(seed: number) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

describe("diff property ve provenance regression'ları", () => {
  it("identity ve yalnız teknik boşluk değişimi fark üretmez", () => {
    const paragraphs = [
      { text: "MADDE 6 – ÜCRET VE YAN HAKLAR" },
      { text: "Ücret her ayın beşinci iş gününde banka hesabına ödenir." },
      { text: "Çalışanın yıllık izin hakları saklıdır." },
    ];
    expect(compareDocuments(makeBlocks(paragraphs, "base"), makeBlocks(paragraphs, "revised")).changes).toHaveLength(0);
    expect(compareDocuments(
      makeBlocks([{ text: "Ücret   her ay\tbanka hesabına ödenir." }], "base"),
      makeBlocks([{ text: " Ücret her ay banka hesabına ödenir. " }], "revised"),
    ).changes).toHaveLength(0);
  });

  it("salt paragraf split/merge işlemini içerik değişikliği saymaz", () => {
    const base = makeBlocks([
      { text: "MADDE 4 – ÇALIŞMA DÜZENİ" },
      { text: "Çalışma haftada beş gündür. Fazla çalışma yazılı onaya tabidir." },
    ], "base");
    const revised = makeBlocks([
      { text: "MADDE 4 – ÇALIŞMA DÜZENİ" },
      { text: "Çalışma haftada beş gündür." },
      { text: "Fazla çalışma yazılı onaya tabidir." },
    ], "revised");
    expect(compareDocuments(base, revised).changes).toHaveLength(0);
    expect(compareDocuments(revised, base).changes).toHaveLength(0);
  });

  it("prefix extension ve yön tersleme substantive değişikliği korur", () => {
    const base = makeBlocks([{ text: "İstanbul Mahkemeleri ve İcra Daireleri yetkilidir." }], "base");
    const revised = makeBlocks([{ text: "Münhasıran İstanbul Anadolu Mahkemeleri ve İcra Daireleri yetkilidir." }], "revised");
    const forward = compareDocuments(base, revised);
    const reverse = compareDocuments(revised, base);
    expect(forward.changes).toHaveLength(1);
    expect(reverse.changes).toHaveLength(1);
    expect(forward.changes[0].kind).toBe("added");
    expect(reverse.changes[0].kind).toBe("removed");
    expect(forward.changes.flatMap((change) => change.summary).join(" ")).toContain("Münhasıran");
    expect(reverse.changes.flatMap((change) => change.summary).join(" ")).toContain("Münhasıran");
  });

  it("trace hunks her zaman bildirilen atomik source metninden gelir", () => {
    const base = makeBlocks([
      { text: "MADDE 12 – FİKRÎ HAKLAR" },
      { text: "Çalışanın adını belirtmesi değerlendirilir." },
      { text: "Şirket dilediği çalışmalarda Çalışanın adını belirtebilir." },
    ], "base");
    const revised = makeBlocks([
      { text: "MADDE 12 – FİKRÎ HAKLAR" },
      { text: "Şirket dilediği çalışmalarda Çalışanın adını kredi olarak belirtebilir." },
    ], "revised");
    const result = compareDocuments(base, revised, { trace: true });
    expect(result.trace).toBeDefined();
    for (const row of result.trace!.rows) {
      const oldById = new Map(row.oldSourceUnits.map((unit) => [unit.id, unit.text]));
      const nextById = new Map(row.newSourceUnits.map((unit) => [unit.id, unit.text]));
      for (const hunk of row.tokenDiffHunks) {
        if (hunk.removed) {
          expect(hunk.oldSourceBlockIds.some((id) => oldById.get(id)?.includes(hunk.removed))).toBe(true);
        }
        if (hunk.added) {
          expect(hunk.newSourceBlockIds.some((id) => nextById.get(id)?.includes(hunk.added))).toBe(true);
        }
        if (hunk.removed && hunk.added) {
          expect(hunk.replacementReason).toBe("same-atomic-pair");
          expect(hunk.oldSourceBlockIds.length).toBeGreaterThan(0);
          expect(hunk.newSourceBlockIds.length).toBeGreaterThan(0);
        }
      }
    }
  });

  it("deterministic randomized structural varyantlarda truth ve symmetry korunur", () => {
    const random = seeded(0xD36151C);
    const nouns = ["ücret", "izin", "gizlilik", "ekipman", "yetki", "rekabet"];
    for (let iteration = 0; iteration < 80; iteration += 1) {
      const noun = nouns[Math.floor(random() * nouns.length)];
      const oldValue = 1 + Math.floor(random() * 90);
      let newValue = 1 + Math.floor(random() * 90);
      if (newValue === oldValue) newValue += 1;
      const headingNumber = 1 + Math.floor(random() * 20);
      const oldParagraph = `${noun} bakımından süre ${oldValue} gündür. Tarafların diğer hakları saklıdır.`;
      const newParagraph = `${noun} bakımından süre ${newValue} gündür. Tarafların diğer hakları saklıdır.`;
      const base = makeBlocks([
        { text: `MADDE ${headingNumber} – ${noun.toLocaleUpperCase("tr-TR")}` },
        { text: oldParagraph },
      ], "base");
      const revised = makeBlocks([
        { text: `Madde-${headingNumber + 3}: ${noun}` },
        ...(random() > 0.5
          ? [{ text: newParagraph }]
          : [{ text: `${noun} bakımından süre ${newValue} gündür.` }, { text: "Tarafların diğer hakları saklıdır." }]),
      ], "revised");
      const forward = compareDocuments(base, revised, { trace: true });
      const reverse = compareDocuments(revised, base, { trace: true });
      expect(forward.changes.length).toBeGreaterThan(0);
      expect(reverse.changes.length).toBeGreaterThan(0);
      expect(Math.abs(forward.changes.length - reverse.changes.length)).toBeLessThanOrEqual(1);
      const text = forward.changes.flatMap((change) => change.summary).join(" ");
      expect(text).toContain(String(oldValue));
      expect(text).toContain(String(newValue));
      for (const row of forward.trace!.rows) {
        for (const hunk of row.tokenDiffHunks.filter((item) => item.removed && item.added)) {
          expect(hunk.replacementReason).toBe("same-atomic-pair");
        }
      }
    }
  });
});
