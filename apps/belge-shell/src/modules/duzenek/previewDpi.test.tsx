// Büyük sayfaların önizlemesi: dpi, yerel 40 MP sınırı ve ilk render.
//
// Denetim bulgusu (a01): Retina'da (dpr 2) ilk büyük render sayfanın boyunu
// bilmeden ceil(96 · dpr) = 192 dpi istiyordu. A0 ve piksel birimiyle yazılmış
// taranmış sayfalar (2480×3508 pt) bu dpi'de yerel motorun 40 MP güvenlik
// sınırını aşar (crates/ekler-core/src/raster.rs). Önizleme hiç gelmiyor,
// GEÇERLİ belge için "Bozuk veya geçersiz PDF dosyası" okunuyordu. A1 %150 ve
// %200'de sayfa çiziliyor, ama üstünde aynı hata şeridi kalıyordu.
//
// Sınır güvenlik denetimidir ve değişmez. Ön yüz onu aşmamayı öğrenir:
// sayfanın boyu bilinince dpi bütçeye göre sınırlanır; boy bilinmeden istenen
// ilk render başarısız olursa bir kez 96 dpi ile yeniden denenir.
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PreviewStage } from "./PdfWorkspace";
import { previewDpi, previewGeometry, previewRetryDpi } from "./pdfWorkspaceState";

/** raster.rs: genişlik × yükseklik > 40 000 000 ise "Görsel boyutu güvenli sınırı aşıyor". */
const BUDGET = 40_000_000;
type Pt = { w: number; h: number };
/** Yerel render'ın piksel sayısı, raster.rs ile aynı yuvarlama: her kenar ceil(pt · dpi / 72). */
const nativePixels = (page: Pt, dpi: number) => Math.ceil((page.w * dpi) / 72) * Math.ceil((page.h * dpi) / 72);
/**
 * Ön yüzün bildiği sayfa boyu: ilk başarılı render'ın bitmap'i, 96 dpi CSS
 * pikseli (PdfWorkspace `naturalWidth * 96 / dpi`).
 */
const bitmapCss = (page: Pt, dpi: number) => ({
  width: (Math.ceil((page.w * dpi) / 72) * 96) / dpi,
  height: (Math.ceil((page.h * dpi) / 72) * 96) / dpi,
});
/** v0.3.1 / saha-0.3.2 kuralı: clamp(96..300, ceil(96 · ölçek · dpr)). */
const legacyDpi = (scale: number, dpr: number) => Math.min(300, Math.max(96, Math.ceil(96 * scale * dpr)));

const A4: Pt = { w: 595, h: 842 };
const A3: Pt = { w: 842, h: 1191 };
const LETTER: Pt = { w: 612, h: 792 };
const A1: Pt = { w: 1684, h: 2384 };
const A0: Pt = { w: 2384, h: 3370 };
/** Sayfa boyu piksel sayısıyla yazılmış 300 dpi A4 taraması. */
const SCAN: Pt = { w: 2480, h: 3508 };

const DPRS = [1, 1.25, 1.5, 1.75, 2, 2.5, 3];
/** Yakınlaştırma listesi (75..200) + sığdırma kiplerinin büyük sayfada verdiği küçük ölçekler + uçlar. */
const SCALES = [0.1, 0.18, 0.25, 0.33, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

describe("previewDpi: sayfa boyu bilinmeden (ilk render) bugünkü kural", () => {
  it("dpr 2, bitmap yok → 192 (Retina keskinliği korunur)", () => {
    expect(previewDpi(1, 2)).toBe(192);
  });
  it.each([
    [1, 96],
    [1.25, 120],
    [1.5, 144],
    [1.75, 168],
    [3, 288],
    [4, 300],
  ])("dpr %s, bitmap yok → %s", (dpr, expected) => {
    expect(previewDpi(1, dpr)).toBe(expected);
  });
});

describe("previewDpi: sayfa boyu bilinince yerel 40 MP sınırı aşılmaz", () => {
  it("A1 (2245×3179 CSS px) %150, dpr 2: 288 değil, bütçeye sığan en keskin dpi", () => {
    const css = { width: 2245, height: 3179 };
    const dpi = previewDpi(1.5, 2, css);
    expect(dpi).toBeLessThanOrEqual(227);
    expect(Math.ceil((css.width * dpi) / 96) * Math.ceil((css.height * dpi) / 96)).toBeLessThanOrEqual(BUDGET);
    // Gereksiz yere bulanıklaşmaz: bir üst dpi artık sığmaz.
    expect(Math.ceil((css.width * (dpi + 1)) / 96) * Math.ceil((css.height * (dpi + 1)) / 96)).toBeGreaterThan(BUDGET);
  });

  for (const [name, page] of Object.entries({ A1, A0, SCAN }))
    it(`${name}: her ölçek × dpr, ilk render 96 dpi'de geldiyse sonraki her istek sınırın içinde`, () => {
      const known = bitmapCss(page, 96);
      for (const dpr of DPRS)
        for (const scale of SCALES) {
          const dpi = previewDpi(scale, dpr, known);
          expect(nativePixels(page, dpi), `${name} ölçek ${scale} dpr ${dpr} → ${dpi} dpi`).toBeLessThanOrEqual(BUDGET);
          expect(dpi).toBeGreaterThanOrEqual(96);
          expect(dpi).toBeLessThanOrEqual(legacyDpi(scale, dpr));
        }
    });

  it("A1 %200 Retina: sınır yalnız gerektiğinde devreye girer (192 sığdığı için ilk render aynen kalır)", () => {
    expect(nativePixels(A1, 192)).toBeLessThanOrEqual(BUDGET);
    const known = bitmapCss(A1, 192);
    expect(nativePixels(A1, previewDpi(2, 2, known))).toBeLessThanOrEqual(BUDGET);
  });
});

describe("previewDpi: olağan sayfalarda keskinlik değişmez", () => {
  for (const [name, page] of Object.entries({ A4, A3, LETTER }))
    it(`${name}: her ölçek × dpr için sonuç bugünkü formülle aynı`, () => {
      for (const dpr of DPRS) {
        const known = bitmapCss(page, legacyDpi(1, dpr));
        for (const scale of SCALES) expect(previewDpi(scale, dpr, known), `ölçek ${scale} dpr ${dpr}`).toBe(legacyDpi(scale, dpr));
      }
    });

  it("sonuç hep 96..300 aralığında (yerel motor 36..300 dışını reddeder)", () => {
    const huge = bitmapCss({ w: 14400, h: 14400 }, 96);
    for (const scale of SCALES)
      for (const dpr of DPRS) {
        const dpi = previewDpi(scale, dpr, huge);
        expect(dpi).toBeGreaterThanOrEqual(96);
        expect(dpi).toBeLessThanOrEqual(300);
        expect(Number.isInteger(dpi)).toBe(true);
      }
  });
});

describe("previewRetryDpi: başarısız ilk büyük render bir kez, bütçeye sığan dpi ile yeniden denenir", () => {
  it("A0 ve 2480×3508 pt: 192 dpi sınırı aşar, 96 dpi sığar", () => {
    for (const page of [A0, SCAN]) {
      expect(nativePixels(page, 192)).toBeGreaterThan(BUDGET);
      expect(nativePixels(page, 96)).toBeLessThanOrEqual(BUDGET);
    }
  });
  it("bitmap yokken 96'nın üstündeki dpi başarısız olursa → 96", () => {
    expect(previewRetryDpi(192, false)).toBe(96);
    expect(previewRetryDpi(288, false)).toBe(96);
    expect(previewRetryDpi(144, false)).toBe(96);
  });
  it("tek sefer: 96 dpi de başarısızsa yeniden deneme yok, hata gösterilir", () => {
    expect(previewRetryDpi(96, false)).toBeNull();
  });
  it("sayfa zaten çizilmişse yeniden deneme yok: hata yerleşim dışı şerittir", () => {
    expect(previewRetryDpi(288, true)).toBeNull();
    expect(previewRetryDpi(192, true)).toBeNull();
  });
  it("küçük resim (40 dpi) yeniden denenmez", () => {
    expect(previewRetryDpi(40, false)).toBeNull();
  });

  /**
   * Preview'ın istek sırası, yalnız saf kurallarla: ilk istek boy bilinmeden,
   * başarısızsa yeniden deneme, sonra bitmap'in boyuyla sığdırma kipindeki dpi.
   * Bitmap'in CSS boyu İLK BAŞARILI render'dan gelir (previous?.width kuralı).
   */
  const requests = (page: Pt, dpr: number, mode: "fit-page" | number) => {
    const asked: number[] = [];
    let at = previewDpi(1, dpr);
    asked.push(at);
    if (nativePixels(page, at) > BUDGET) {
      const retry = previewRetryDpi(at, false);
      if (retry === null || nativePixels(page, retry) > BUDGET) return { asked, shown: false };
      at = retry;
      asked.push(at);
    }
    const known = bitmapCss(page, at);
    const geometry = previewGeometry(known.width, known.height, 0, 1100, 800, mode);
    const next = previewDpi(geometry.scale, dpr, known);
    // Aynı dpi'de çizilmiş bitmap zaten görünüyor: ikinci render istenmez.
    if (next !== at) asked.push(next);
    const failed = nativePixels(page, next) > BUDGET;
    return { asked, shown: true, failed, cssWidth: known.width };
  };

  it("A0 Retina, Sayfaya sığdır: [192, 96]; sayfa görünür, hata yok", () => {
    expect(requests(A0, 2, "fit-page")).toEqual({ asked: [192, 96], shown: true, failed: false, cssWidth: bitmapCss(A0, 96).width });
  });
  it("2480×3508 pt tarama Retina, Sayfaya sığdır: [192, 96]; sayfa görünür, hata yok", () => {
    expect(requests(SCAN, 2, "fit-page")).toMatchObject({ asked: [192, 96], shown: true, failed: false });
  });
  it("A1 Retina %150: üçüncü istek 288 değil, sınırın içinde; hata şeridi yok", () => {
    const run = requests(A1, 2, 150);
    expect(run.asked[0]).toBe(192);
    expect(run.asked[run.asked.length - 1]).toBeLessThan(288);
    expect(run).toMatchObject({ shown: true, failed: false });
  });
  it("A4 Retina, Sayfaya sığdır: istekler bugünküyle aynı [192, ölçek dpi'si]", () => {
    const run = requests(A4, 2, "fit-page");
    const known = bitmapCss(A4, 192);
    expect(run.asked).toEqual([192, legacyDpi(previewGeometry(known.width, known.height, 0, 1100, 800, "fit-page").scale, 2)]);
  });
});

describe("önizleme sahnesi yerel boyut sınırını 'bozuk' diye anlatmaz", () => {
  const native = "Bozuk veya geçersiz PDF dosyası: Görsel boyutu güvenli sınırı aşıyor";
  it("sayfa hiç çizilemediyse: sakin cümle, 'bozuk' yok", () => {
    const html = renderToStaticMarkup(<PreviewStage large geometry={null} bitmap={null} error={native} rotation={0} label="sayfa 1" />);
    expect(html).toContain("önizleme için çok büyük");
    expect(html).toContain("belge sağlam");
    expect(html).not.toMatch(/bozuk|geçersiz|güvenli sınırı/i);
  });
  it("sayfa çizilmişken: aynı cümle, yerleşim dışı tek şerit olarak", () => {
    const geometry = previewGeometry(200, 280, 0, 592, 547, "fit-page");
    const html = renderToStaticMarkup(<PreviewStage large geometry={geometry} bitmap={{ url: "blob:x" }} error={native} rotation={0} label="sayfa 1" />);
    const paragraphs = html.match(/<p [^>]*>/g) ?? [];
    expect(paragraphs).toHaveLength(1);
    expect(paragraphs[0]).toContain('class="pdf-wait pdf-wait-overlay"');
    expect(html).not.toMatch(/bozuk|geçersiz/i);
  });
  it("küçük resimde de aynı", () => {
    const html = renderToStaticMarkup(<PreviewStage large={false} geometry={null} bitmap={null} error={native} rotation={0} label="sayfa 1" />);
    expect(html).toContain("önizleme için çok büyük");
    expect(html).not.toMatch(/bozuk|geçersiz/i);
  });
});
