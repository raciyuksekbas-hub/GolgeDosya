// Önizleme hatalarının sunumu.
//
// Yerel motor önizleme bitmap'i için 40 MP güvenlik sınırını "Bozuk veya
// geçersiz PDF dosyası: Görsel boyutu güvenli sınırı aşıyor" diye bildirir
// (crates/ekler-core/src/raster.rs, InvalidPdf). Metin teknik görünmediği
// için önizleme yüzeyine olduğu gibi çıkıyordu: geçerli bir A0 paftası ya da
// taranmış ek için kullanıcı "Bozuk" okuyordu. Belge bozuk değildir; yalnız
// sayfa önizleme için çok büyüktür ve sayfa işlemlerini etkilemez.
//
// Eşleme YALNIZ önizleme bağlamındadır: kaydetme/dışa aktarma kategorileri
// ve açma kategorileri değişmez.
import { describe, expect, it } from "vitest";
import {
  describeOpenFailure,
  describePreviewFailure,
  describeSaveFailure,
  PREVIEW_TOO_LARGE,
  safeMessage,
} from "./failure";

const FALLBACK = "Bu sayfanın önizlemesi oluşturulamadı.";
/** Tauri komutu hatayı `e.to_string()` ile taşır; dize olarak gelir. */
const NATIVE = "Bozuk veya geçersiz PDF dosyası: Görsel boyutu güvenli sınırı aşıyor";

describe("önizleme: yerel boyut sınırı", () => {
  it("sakin cümleye eşlenir; 'bozuk', 'geçersiz' ya da motor metni görünmez", () => {
    const text = describePreviewFailure(NATIVE, FALLBACK);
    expect(text).toBe(PREVIEW_TOO_LARGE);
    expect(text).toBe("Bu sayfa önizleme için çok büyük; belge sağlam, sayfa işlemleri etkilenmez.");
    // "bozuk" hiç geçmez: küçük resim cümleyi dört satırda keser, "…belge bozuk…" okunmamalı.
    expect(text).not.toMatch(/bozuk|geçersiz|güvenli sınırı/i);
  });
  it("Error nesnesi ve önek de aynı cümleye gider", () => {
    expect(describePreviewFailure(new Error(NATIVE), FALLBACK)).toBe(PREVIEW_TOO_LARGE);
    expect(describePreviewFailure(`Error: ${NATIVE}`, FALLBACK)).toBe(PREVIEW_TOO_LARGE);
  });
});

describe("önizleme: diğer hatalar bugünkü gibi", () => {
  it("gerçek yapısal ret 'belge sağlam' denmez (pdf-core'un aynı kalıplı sınırları)", () => {
    for (const raw of [
      "Bozuk veya geçersiz PDF dosyası: PDF akışının açılmış boyutu güvenli sınırı aşıyor",
      "Bozuk veya geçersiz PDF dosyası: PDF nesne yuvalanması güvenli sınırı aşıyor (65 > 64); belge açılmadı",
      "Bozuk veya geçersiz PDF dosyası: xref tablosu okunamadı",
    ]) {
      expect(describePreviewFailure(raw, FALLBACK)).not.toBe(PREVIEW_TOO_LARGE);
      expect(describePreviewFailure(raw, FALLBACK)).toBe(safeMessage(raw, FALLBACK));
    }
  });
  it("temiz motor cümlesi geçer, teknik metin yedeğe düşer", () => {
    expect(describePreviewFailure("Yerel PDF renderer başarısız", FALLBACK)).toBe("Yerel PDF renderer başarısız");
    expect(describePreviewFailure("Error: Dosya okunamadı: /Users/ornek/dilekce.pdf (os error 2)", FALLBACK)).toBe(FALLBACK);
    expect(describePreviewFailure("", FALLBACK)).toBe(FALLBACK);
  });
});

describe("kaydetme ve açma kategorileri önizleme cümlesini almaz", () => {
  it("dışa aktarmadaki aynı sınır önizleme cümlesine dönüşmez", () => {
    expect(describeSaveFailure(NATIVE)).not.toBe(PREVIEW_TOO_LARGE);
    expect(describeOpenFailure(NATIVE)).not.toBe(PREVIEW_TOO_LARGE);
  });
});
