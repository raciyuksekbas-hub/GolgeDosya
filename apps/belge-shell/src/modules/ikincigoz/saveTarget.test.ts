// Denetle — düzeltilmiş kopyanın kaydetme penceresi.
//
// Pencere öneri almıyordu (`defaultPath: undefined`): ad yok, klasör yok,
// filtre yok ("kayıt klasörleri arasında birlik yok").
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { correctedName, correctedSaveOptions } from "./saveTarget";

const here = dirname(fileURLToPath(import.meta.url));
const review = readFileSync(resolve(here, "ReviewWorkspace.tsx"), "utf8");

describe("Denetle'nin kaydetme penceresi", () => {
  it("motorun önerdiği ad, belgenin klasörü ve belgenin türü", () => {
    expect(correctedName("/Users/avukat/Dava/dava dilekçesi.docx")).toBe("dava dilekçesi - Düzeltilmiş.docx");
    expect(correctedName("C:\\Dava\\İŞ SÖZLEŞMESİ.udf")).toBe("İŞ SÖZLEŞMESİ - Düzeltilmiş.udf");
    expect(correctedSaveOptions("/Users/avukat/Dava/dava.docx", "/Users/avukat/Dava")).toEqual({
      title: "Düzeltilmiş kopyayı kaydet",
      defaultPath: "/Users/avukat/Dava/dava - Düzeltilmiş.docx",
      filters: [{ name: "Word belgesi", extensions: ["docx"] }],
    });
    // Öneri yoksa (geçici klasör) yalnız ad; tür filtresi yine verilir.
    expect(correctedSaveOptions("C:\\Dava\\karar.UDF", null)).toMatchObject({
      defaultPath: "karar - Düzeltilmiş.UDF",
      filters: [{ name: "UYAP belgesi", extensions: ["udf"] }],
    });
  });

  it("pencere bu seçeneklerle ve belgenin klasöründe açılır", () => {
    expect(review).toContain("await save(correctedSaveOptions(path, await startFolder(path)))");
    expect(review).not.toContain("defaultPath: undefined");
  });
});
