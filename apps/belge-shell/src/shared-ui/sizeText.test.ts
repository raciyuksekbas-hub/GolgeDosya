// Boyut ve küçülme oranı — kullanıcının okuyacağı tek sayı dili.
//
// Sahadan: "Bu sayısal değerlerin çoğumuz için hiçbir anlamı yok." Başarı
// satırı "13478.1 KB → 522.0 KB · %96.1" diyordu: İngilizce ondalık noktası,
// megabaytta bile KB, okunmayan ondalıklar.
import { describe, expect, it } from "vitest";
import { approxReduction, humanSize } from "./sizeText";

describe("humanSize", () => {
  it.each([
    [0, "0 bayt"],
    [1_000, "1000 bayt"],
    [1_123, "1 KB"],
    [95_133, "93 KB"],
    [816_361, "797 KB"],
    // 1023,6 KB, "1024 KB" yazılmaz; megabayta geçer.
    [1_048_300, "1,0 MB"],
    [1_048_576, "1,0 MB"],
    [1_317_864, "1,3 MB"],
    [4_299_161, "4,1 MB"],
    [9_834_246, "9,4 MB"],
    [10_276_045, "9,8 MB"],
    [13_801_597, "13,2 MB"],
  ])("%i → %s", (bytes, expected) => {
    expect(humanSize(bytes)).toBe(expected);
  });
});

describe("approxReduction", () => {
  it.each([
    [10_276_045, 4_299_161, "%58"],
    [13_801_597, 534_529, "%96"],
    // Sahada kaydedilen %2,1'lik sonuç: tamsayı ve yaklaşık.
    [9_834_246, 9_625_452, "%2"],
    [10_000_000, 9_990_000, "%1'den az"],
    // Neredeyse tamamı gitse de dosya duruyorsa "%100" denmez.
    [10_000_000, 1_000, "%99"],
  ])("%i → %i: %s", (source, output, expected) => {
    expect(approxReduction(source, output)).toBe(expected);
  });

  it("küçülme yoksa oran uydurulmaz", () => {
    expect(approxReduction(1_123, 2_407)).toBeNull();
    expect(approxReduction(5_000, 5_000)).toBeNull();
    expect(approxReduction(0, 0)).toBeNull();
  });
});

describe("Türkçe sayı biçimi", () => {
  it("hiçbir çıktıda ondalık noktası yok", () => {
    const samples = [0, 999, 1_123, 52_345, 1_048_576, 9_834_246, 13_801_597, 734_003_200];
    for (const bytes of samples) expect(humanSize(bytes)).not.toMatch(/\d\.\d/);
    for (const [s, o] of [[13_801_597, 534_529], [9_834_246, 9_625_452], [3_000_000, 2_911_000]])
      expect(approxReduction(s, o)).not.toMatch(/\d\.\d/);
  });
});
