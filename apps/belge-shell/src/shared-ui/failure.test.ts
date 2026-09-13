// Kaydetme ve dışa aktarma hatalarının sunum sözleşmesi.
//
// Açma yolundaki ilkenin aynısı: motorun teknik metni kullanıcıya çıkmaz.
// Burada kilitlenen şey, o ilkenin kaydetme/dışa aktarma yollarında da
// geçerli olduğudur.
import { describe, expect, it } from "vitest";
import {
  describeSaveFailure,
  plainMessage,
  safeMessage,
  SAVE_FAILURE_FALLBACK,
} from "./failure";

/** Motorun kaydetme yolunda gerçekten ürettiği metinler (pdf-core/src/error.rs). */
const ENGINE_SAVE_MESSAGES = [
  "Error: Kaynak dosya bütünlük hatası! Kaynak değiştirilmiş: /Users/ornek/Belgeler/dilekce.pdf",
  "Error: Sayfa hesaplama uyumsuzluğu (Strict Page Accounting Failure): 12 beklenen sayfa, ancak 11 sayfa bulundu",
  "Error: Hedef dosya boyutu sınırı aşıldı: 91230012 bayt > 90000000 bayt sınırı",
  "Error: Elektronik imzalı UDF için kullanıcı onayı verilmedi: /Users/ornek/Belgeler/dilekce.udf",
  "Error: Doğrulama hatası (Validation Failed): çıktı akışı yeniden açılamadı",
  "Error: Bozuk veya geçersiz PDF dosyası: xref tablosu okunamadı",
  "Error: Dosya okunamadı veya erişilemedi: /Users/ornek/Çıktı: Permission denied (os error 13)",
  "Error: Kaydedilen PDF yeniden doğrulanamadı.",
];

/** Kullanıcı metninde asla bulunmayacak izler. */
const LEAKS = [
  "Error:",
  "Validation Failed",
  "Strict Page Accounting",
  "Doğrulama hatası",
  "os error",
  "/Users/",
  "bayt >",
  "xref",
  "duzenek_",
];

describe("kaydetme hatalarının sunumu", () => {
  it("teknik önek, sınıf adı, sayaç ve dosya yolu kullanıcıya çıkmaz", () => {
    for (const raw of ENGINE_SAVE_MESSAGES) {
      const detail = describeSaveFailure(raw);
      for (const leak of LEAKS) expect(detail).not.toContain(leak);
      expect(detail.length).toBeGreaterThan(20);
      expect(detail.length).toBeLessThan(140);
    }
  });

  it("güvenlik denetimleri bilgi olarak korunur, sınıf adıyla değil", () => {
    // Bu üçü kullanıcının bilmesi gereken sonuçlardır: kopya YAZILMADI.
    expect(describeSaveFailure(ENGINE_SAVE_MESSAGES[0])).toMatch(/işlem sırasında değişti/);
    expect(describeSaveFailure(ENGINE_SAVE_MESSAGES[1])).toMatch(/sayfa sayısı/);
    expect(describeSaveFailure(ENGINE_SAVE_MESSAGES[2])).toMatch(/boyut sınırını/);
    // Her kategori ayrı bir cümleye düşer; hepsi yedeğe düşseydi test boş olurdu.
    const mapped = ENGINE_SAVE_MESSAGES.map(describeSaveFailure);
    expect(mapped.filter((d) => d === SAVE_FAILURE_FALLBACK).length).toBe(0);
    expect(new Set(mapped).size).toBeGreaterThanOrEqual(7);
  });

  it("tanınmayan hata güvenli yedeğe düşer", () => {
    expect(describeSaveFailure(new Error("¿?"))).toBe(SAVE_FAILURE_FALLBACK);
    expect(describeSaveFailure(null)).toBe(SAVE_FAILURE_FALLBACK);
  });
});

describe("motorun kendi kullanıcı cümlesini taşıyan yollar", () => {
  it("temiz cümle olduğu gibi geçer", () => {
    // İkinciGöz ve Tavzih kullanıcı cümlesini kendisi üretir; kategoriye
    // indirmek bilgi kaybettirirdi.
    for (const clean of [
      "Belge, düzeltmeler hesaplandıktan sonra değişmiş görünüyor. Belgeyi yeniden inceleyin.",
      "Seçilen düzeltmeler birbiriyle çakışıyor. Daha az düzeltme seçin.",
      "Yeni dosya kaydedilemedi.",
    ])
      expect(safeMessage(clean, "yedek")).toBe(clean);
    // Sınıf öneki takılmışsa yalnız o düşer.
    expect(safeMessage("TypeError: Yeni dosya kaydedilemedi.", "yedek")).toBe(
      "Yeni dosya kaydedilemedi.",
    );
  });

  it("teknik görünen metin kullanıcıya çıkmaz, yedek cümle gider", () => {
    for (const technical of [
      "Permission denied (os error 13)",
      "thread 'main' panicked at src/lib.rs:42",
      "ikincigoz::writeback::apply failed",
      "command ikincigoz_apply_fixes not found",
      "Cannot read properties of undefined",
      "",
    ])
      expect(safeMessage(technical, "yedek")).toBe("yedek");
  });

  it("plainMessage sınıf önekini düşürür, cümleyi bırakır", () => {
    expect(plainMessage(new Error("Klasör oluşturulamadı."))).toBe("Klasör oluşturulamadı.");
  });
});
