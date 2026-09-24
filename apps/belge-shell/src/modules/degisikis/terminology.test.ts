// Karşılaştır terim tutarlılığı (saha maddesi 31).
//
// Saha: kelime ya da tek harf değişikliğinde bile "Değiştirilen paragraf"
// yazıyordu. Bu bir KONUM yedeğiydi ama tür gibi okunuyordu ve her türe
// uygulanıyordu: eklenen paragraf "Eklenen · Değiştirilen paragraf" diye iki
// zıt tür taşıyordu. Gerçek motor üzerinde ölçülür.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";
import { buildComparisonViewModel } from "./viewModels/comparisonViewModel";
import { KIND_LABEL } from "./uiLabels";

const here = dirname(fileURLToPath(import.meta.url));
const compare = (base: string[], revised: string[]) => {
  const result = compareDocuments(
    makeBlocks(base.map((text) => ({ text })), "base"),
    makeBlocks(revised.map((text) => ({ text })), "revised"),
  );
  return { result, model: buildComparisonViewModel(result) };
};

describe("konum türü söylemez; tür etiketiyle çelişmez", () => {
  it("tek harflik düzeltme: tür Değiştirilen, konum 'Değişiklik içeren paragraf'", () => {
    const { model } = compare(["Sözleşme feshedilmiştir."], ["Sözleşme feshedilmistir."]);
    expect(model.changes).toHaveLength(1);
    expect(model.changes[0].kind).toBe("modified");
    expect(model.changes[0].sectionLabel).toBe("Değişiklik içeren paragraf");
  });

  it("eklenen ve silinen numarasız paragraf 'değiştirilen' diye adlandırılmaz", () => {
    const added = compare(["Birinci paragraf."], ["Birinci paragraf.", "Yepyeni bir paragraf eklendi."]).model;
    const removed = compare(["Birinci paragraf.", "Bu paragraf çıkarıldı."], ["Birinci paragraf."]).model;
    for (const change of [...added.changes, ...removed.changes]) {
      expect(change.sectionLabel).not.toMatch(/değiştiril/i);
      // Listede okunan birleşik satır iki farklı tür sözcüğü taşımaz.
      const line = `${KIND_LABEL[change.kind]} · ${change.sectionLabel}`;
      const kinds = ["Eklenen", "Silinen", "Değiştirilen"].filter((k) => line.includes(k));
      expect(kinds).toEqual([KIND_LABEL[change.kind]]);
    }
  });
});

describe("tek kaynak", () => {
  it("tür adları tek yerde tanımlı; liste, ray ve raporlar onu kullanır", () => {
    for (const file of ["ChangeInspector.tsx", "ChangeRail.tsx", "report.ts", "reportDocx.ts"]) {
      const src = readFileSync(resolve(here, file), "utf8");
      expect(src, file).not.toMatch(/const KIND_LABEL\b/);
      expect(src, file).toContain('import { KIND_LABEL } from "./uiLabels"');
    }
  });

  it("eski ifade kullanıcı metinlerinde kalmadı", () => {
    for (const file of ["core/normalize.ts", "ChangeInspector.tsx", "ChangeRail.tsx", "report.ts", "reportDocx.ts", "uiLabels.ts"]) {
      const code = readFileSync(resolve(here, file), "utf8")
        .split("\n")
        .filter((l) => !l.trim().startsWith("*") && !l.trim().startsWith("//"))
        .join("\n");
      expect(code, file).not.toMatch(/Değiştirilen paragraf|değiştirilen bölüm|Değiştirilen bölüm/);
    }
  });
});
