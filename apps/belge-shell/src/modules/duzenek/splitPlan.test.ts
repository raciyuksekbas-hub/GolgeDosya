// Ayır — sayfa aralığı ifadesinin sözleşmesi.
//
// Bu testler motoru çağırmaz; ifadeden çıkan PLANI sabitler. Plan yanlışsa
// kullanıcı yanlış dosyaları alır ve bunu ancak açtığında fark eder.
import { describe, expect, it } from "vitest";
import {
  describePages,
  everyPage,
  parseRanges,
  partFileName,
  partLabel,
  partPageCount,
  partPages,
  uncoveredPages,
} from "./splitPlan";

const parts = (expression: string, pageCount = 12) => {
  const plan = parseRanges(expression, pageCount);
  if (!plan.ok) throw new Error(`beklenmedik hata: ${plan.error}`);
  return plan.parts;
};

const error = (expression: string, pageCount = 12) => {
  const plan = parseRanges(expression, pageCount);
  if (plan.ok) throw new Error("hata bekleniyordu");
  return plan.error;
};

describe("sayfa aralığı ifadesi", () => {
  it("aralık, tek sayfa ve açık uçları birlikte okur", () => {
    expect(parts("1-3, 4-10, 11")).toEqual([
      { start: 1, end: 3 },
      { start: 4, end: 10 },
      { start: 11, end: 11 },
    ]);
    // Açık uçlar: baştan ve sona kadar.
    expect(parts("-3")).toEqual([{ start: 1, end: 3 }]);
    expect(parts("8-")).toEqual([{ start: 8, end: 12 }]);
    expect(parts("-2, 5, 9-")).toEqual([
      { start: 1, end: 2 },
      { start: 5, end: 5 },
      { start: 9, end: 12 },
    ]);
  });

  it("boşluk, noktalı virgül ve satır sonu da ayırıcıdır", () => {
    expect(parts("1-3 ;4-10\n11")).toEqual(parts("1-3, 4-10, 11"));
    expect(parts("  1 - 3 ,   4-10  ")).toEqual([
      { start: 1, end: 3 },
      { start: 4, end: 10 },
    ]);
  });

  it("Word’den yapıştırılan uzun tire de tire sayılır", () => {
    // Dilekçeden kopyalayan kullanıcı "1–3" yapıştırır; bu bir hata değildir.
    expect(parts("1–3, 4—10")).toEqual([
      { start: 1, end: 3 },
      { start: 4, end: 10 },
    ]);
  });

  it("çakışan aralıklar geçerlidir", () => {
    // Aynı ek iki dilekçede kullanılabilir; bu gerçek bir iştir.
    expect(parts("1-5, 4-6")).toEqual([
      { start: 1, end: 5 },
      { start: 4, end: 6 },
    ]);
  });

  it("anlaşılmayan parçayı adıyla söyler", () => {
    expect(error("1-3, ek")).toContain("“ek”");
    expect(error("1-3, ek")).toContain("Örnek");
    expect(error("1..3")).toContain("“1..3”");
    expect(error("-")).toContain("“-”");
  });

  it("ters, sıfır ve belge dışı sayfayı reddeder", () => {
    expect(error("10-4")).toContain("tersten");
    expect(error("0-3")).toContain("1’den başlar");
    expect(error("5-99")).toBe("Belge 12 sayfa; 99. sayfa yok.");
    expect(error("13")).toBe("Belge 12 sayfa; 13. sayfa yok.");
  });

  it("boş ifade bir hata değil, bir davettir", () => {
    for (const empty of ["", "   ", ",", " , ; "]) {
      const message = error(empty);
      expect(message).toContain("Ayrılacak sayfa aralıklarını yazın");
      expect(message).toContain("1-3, 4-10, 11");
    }
  });

  it("kullanıcı metninde teknik iz yok", () => {
    for (const bad of ["ek", "1..3", "10-4", "0-3", "5-99", "", "-"]) {
      const message = error(bad);
      for (const leak of ["Error", "undefined", "NaN", "parse", "regex", "token"])
        expect(message).not.toContain(leak);
    }
  });
});

describe("çıktı planı", () => {
  it("her sayfa ayrı seçeneği sayfa sayısı kadar parça üretir", () => {
    expect(everyPage(3)).toEqual([
      { start: 1, end: 1 },
      { start: 2, end: 2 },
      { start: 3, end: 3 },
    ]);
    expect(everyPage(0)).toEqual([]);
  });

  it("parça sayfa sayısı ve sayfa listesi tutarlıdır", () => {
    const part = { start: 4, end: 10 };
    expect(partPageCount(part)).toBe(7);
    expect(partPages(part)).toEqual([4, 5, 6, 7, 8, 9, 10]);
    expect(partPages({ start: 11, end: 11 })).toEqual([11]);
    // Plandaki sayfa toplamı, belgeden alınan sayfa toplamına eşit olmalı.
    const all = parts("1-3, 4-10, 11").flatMap(partPages);
    expect(all).toHaveLength(11);
    expect(new Set(all).size).toBe(11);
  });

  it("dosya adı sayfayı söyler ve doğru sıralanır", () => {
    const stem = "GolgeDosya-ayir-2026-10-28_221500";
    expect(partFileName(stem, { start: 1, end: 3 }, 12)).toBe(`${stem}-sayfa-01-03.pdf`);
    expect(partFileName(stem, { start: 11, end: 11 }, 12)).toBe(`${stem}-sayfa-11.pdf`);
    // Basamak genişliği belgeye göre: 300 sayfalık belgede üç hane.
    expect(partLabel({ start: 7, end: 12 }, 300)).toBe("007-012");
    // Sıfır doldurma olmadan "sayfa-10" "sayfa-2"den önce gelirdi.
    const names = [
      { start: 2, end: 2 },
      { start: 10, end: 10 },
    ].map((p) => partFileName(stem, p, 12));
    expect([...names].sort()).toEqual(names);
  });

  it("hiçbir dosyaya girmeyen sayfalar sayılır", () => {
    expect(uncoveredPages(parts("1-3, 4-10, 11"), 12)).toEqual([12]);
    expect(uncoveredPages(parts("1-12"), 12)).toEqual([]);
    expect(uncoveredPages(parts("4-6"), 12)).toEqual([1, 2, 3, 7, 8, 9, 10, 11, 12]);
    expect(uncoveredPages(everyPage(12), 12)).toEqual([]);
  });

  it("unutulan sayfalar insan diliyle toplanır", () => {
    expect(describePages([1, 2, 3, 7, 8, 12])).toBe("1-3, 7-8, 12");
    expect(describePages([5])).toBe("5");
    expect(describePages([])).toBe("");
  });
});
