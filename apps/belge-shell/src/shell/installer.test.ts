// Windows kurulumu Türkçe (saha maddesi 4): "Kurulumda Türkçe ifadeler olabilir."
//
// NSIS dil yapılandırması hiç yoktu; Tauri'nin varsayılanı yalnız İngilizce
// (makensis: "1 language table"). Kurulum, işletim sisteminin dili listede
// varsa onu, yoksa İLK dili kullanır: Türkçe Windows'ta Türkçe, başka her yerde
// güvenli İngilizce. Derlenen kurulumdaki gerçek dil tablosu sayısını Windows
// CI ölçer (Sürüm metadata adımı).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const conf = JSON.parse(readFileSync(resolve(here, "../../src-tauri/tauri.conf.json"), "utf8"));

describe("NSIS kurulum dili", () => {
  it("Türkçe dahil, İngilizce yedek (ilk dil)", () => {
    const nsis = conf.bundle.windows.nsis;
    expect(nsis.languages).toEqual(["English", "Turkish"]);
    // Dil seçim penceresi yok: dil işletim sisteminden gelir.
    expect(nsis.displayLanguageSelector).toBe(false);
  });
});
