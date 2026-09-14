// Düzenle — taşınan PDF çalışma alanının sözleşme testleri.
//
// Bağımsız DüzenEk'in "qa/tests/pdf-workspace.test.mjs" dosyasından taşındı.
// Test gövdeleri değiştirilmedi; yalnız çalıştırıcı node:test'ten vitest'e,
// CSS kaynağı da `src/styles.css` yerine taşınan `pdf.css` dosyasına çevrildi.
//
// Bunlar GUI kabul testi DEĞİLDİR: ekran görüntüsü almazlar, piksel ölçmezler.
// Bileşeni statik biçimlendirmeye çizip sevk edilen CSS'i ayrıştırırlar —
// yani migration'ın yerleşim ve durum sözleşmesini koruduğunu bileşenden
// bağımsız biçimde kanıtlarlar.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { PdfWorkspace } from "./PdfWorkspace";
import { previewGeometry, rotatePages, workspaceSurfaces } from "./pdfWorkspaceState";
import { PreviewPlaceholder } from "./PdfWorkspace";
import { describeOpenFailure, plainMessage, OPEN_FAILURE_FALLBACK } from "../../shared-ui/failure";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
/** Kipe hiç belge verilmemiş hâl. */
const html = renderToStaticMarkup(<PdfWorkspace />);
/** Kabuğun verdiği belge henüz taranıyor: yüzeyler daha açılmamıştır. */
const scanningHtml = renderToStaticMarkup(<PdfWorkspace paths={["/belgeler/dilekce.pdf"]} />);
const css = postcss.parse(readFileSync(resolve(here, "pdf.css"), "utf8"));

/** Verilen genişlikte yürürlükte olan bildirimler. */
function declarations(selector: string, width: number): Record<string, string> {
  const result: Record<string, string> = {};
  css.walkRules((rule) => {
    if (!rule.selectors.includes(selector)) return;
    // Kuralı çevreleyen @media sorguları: verilen genişlikte geçerli mi?
    type Ancestor = { type: string; params?: string; parent?: Ancestor };
    for (let parent = rule.parent as Ancestor | undefined; parent; parent = parent.parent) {
      if (parent.type !== "atrule") continue;
      const params = parent.params ?? "";
      const max = params.match(/max-width:\s*(\d+)px/);
      const min = params.match(/min-width:\s*(\d+)px/);
      if ((max && width > +max[1]) || (min && width < +min[1])) return;
    }
    rule.walkDecls((d) => {
      result[d.prop] = d.value;
    });
  });
  return result;
}

describe("döndürme durumu", () => {
  it("rotate_left_right_preview_state", () => {
    const original = {};
    const left = rotatePages(original, ["a"], -90);
    expect(left).toEqual({ a: 270 });
    expect(original).toEqual({});
    const right = rotatePages(left, ["a"], 90);
    expect(right).toEqual({ a: 0 });
    const multi = rotatePages(left, ["a", "b"], 90);
    expect(multi).toEqual({ a: 0, b: 90 });
    expect(rotatePages(multi, ["b"], -90)).toEqual({ a: 0, b: 0 });
    expect(rotatePages(multi, [], 90)).toEqual(multi);
  });

  it("rotate_left_updates_preview", () =>
    expect(rotatePages({}, ["a"], -90)).toEqual({ a: 270 }));
  it("rotate_right_updates_preview", () =>
    expect(rotatePages({}, ["a"], 90)).toEqual({ a: 90 }));

  it("multi_page_rotate_preview_state", () => {
    const r = rotatePages({}, ["a", "b", "c"], 90);
    expect(r).toEqual({ a: 90, b: 90, c: 90 });
    for (const angle of Object.values(r))
      expect(previewGeometry(794, 1123, angle, 400, 300, 100).width).toBe(1123);
  });
});

describe("yerleşim sözleşmesi", () => {
  /**
   * Yerleşim, görsel yeniden kompozisyon turunda değişti (docs/DESIGN.md):
   * belge alanı ortada ve en geniş, solunda sayfa şeridi; araçlar kabuğun sağ
   * panelinde, kaydetme yardımcı barda. Buradaki iddialar o kompozisyonu
   * tarif eder — eski iki sütunlu "kontroller solda" düzenini değil.
   *
   * Kabuk sağlayıcısı olmadan render edildiğinde (bu testte olduğu gibi) bar
   * ve panel içeriği satır içi çizilir; hiçbir yüzey kaybolmaz.
   */
  it("pdf_workspace_page_strip_left_document_right", () => {
    // Sayfa varken: şerit solda, belge sağda. `html` belgesiz çizim olduğu için
    // şeridin varlığı kaynaktan, ölçüsü CSS'ten doğrulanır.
    expect(source).toMatch(
      /data-pages=\{surfaces\.strip\}>\s*\{surfaces\.strip && <aside className="thumbnail-list"/,
    );
    for (const width of [1120, 1440]) {
      const rules = declarations('.pdf-root .pdf-workspace-layout[data-pages="true"]', width);
      expect(rules["grid-template-columns"]).toBe("124px minmax(0, 1fr)");
    }
    // Dar pencerede şerit daralır ama kaybolmaz: sayfa seçimi Düzenle'nin işi.
    expect(
      declarations('.pdf-root .pdf-workspace-layout[data-pages="true"]', 1000)[
        "grid-template-columns"
      ],
    ).toBe("104px minmax(0, 1fr)");
  });

  it("document_area_is_the_widest_surface", () => {
    // Belge yokken belge alanı ızgaranın tek çocuğudur; sayfa varken şeridin
    // hemen ardından gelir. İkisinde de tek bir önizleme paneli vardır.
    expect(html).toMatch(
      /class="pdf-workspace-layout" data-pages="false"><aside class="pdf-preview-panel" aria-label="PDF önizleme çalışma alanı">/,
    );
    expect(source).toMatch(/<\/aside>\}\s*\n\s*<aside className="pdf-preview-panel"/);
    expect((html.match(/class="pdf-preview-panel"/g) || []).length).toBe(1);
    expect(html).toMatch(/Belge önizlemesi/);
    for (const width of [1120, 1440]) {
      const rules = declarations(".pdf-root .pdf-preview-panel", width);
      expect(rules["min-width"]).toBe("0");
      expect(rules.display).toBe("flex");
    }
  });

  it("araçlar panelde, kaydetme barda, sonuç workspace'te", () => {
    // Araç grupları kabuğun sağ paneline çizilir.
    expect(source).toMatch(/<InspectorPanel title="Araçlar" scope="pdf-root">/);
    expect(source).toMatch(/<div className="pdf-controls"/);
    // Kaydetme eylemleri yardımcı barda.
    const bar = source.slice(source.indexOf("<ToolbarActions>"), source.indexOf("</ToolbarActions>"));
    expect(bar).toContain("Yeni PDF Kaydet");
    expect(bar).toContain("Klasör Seçerek Kaydet");
    // Sonuç metni panelde DEĞİL: panel kapalıyken de görünmeli.
    const workspace = source.slice(source.indexOf('className="pdf-workspace-layout"'));
    expect(workspace).toMatch(/<Status tone=/);
    // Günlük sayfa araçları belgenin üstündeki şeritte segment olarak.
    expect(source).toMatch(/PAGE_TOOLS: Kind\[\] = \['select', 'reorder', 'delete', 'rotate'\]/);
    expect(source).toMatch(/className="tool-segment" role="group"/);
    // Birleştir ve Görseller → PDF için ikinci belge yolu duruyor.
    expect(source).toContain("Belge Ekle");
  });
});

describe("durum sözleşmesi", () => {
  /**
   * Düzenle'nin yüzeyleri belgeye bağlıdır, araç seçimine değil.
   *
   * Hangi yüzeyin çizileceği saf bir karardır (`workspaceSurfaces`); bileşen o
   * kararı bağlar, CSS de ızgarayı ona göre kurar. Üç halka da burada
   * denetlenir: karar, bağlama, yerleşim. 'ready' ve 'failed' hâlleri gerçek
   * bir tarama gerektirdiği için sunucu tarafı çizimde üretilemez — o yüzden
   * karar doğrudan, bağlama kaynak üzerinden doğrulanır.
   */
  it("işlenebilir PDF yokken ne şerit ne araç paneli çizilir, workspace tam genişlik", () => {
    for (const state of ["none", "loading", "failed"] as const)
      expect(workspaceSurfaces(state, 0)).toEqual({ tools: false, strip: false });
    // Açılamayan belge de sayfa üretmiş olabilir mi? Hayır — ama olsa bile
    // yüzeyler açılmaz: karar belgenin durumundadır.
    expect(workspaceSurfaces("failed", 4)).toEqual({ tools: false, strip: false });

    // Belge açılamadığında bu durum gerçekten kurulur.
    expect(source).toMatch(
      /catch \(e\) \{[\s\S]{0,900}?setDocState\(previous => previous === 'ready' \? previous : 'failed'\)/,
    );

    // Çizilen iki hâl: belge hiç yok, ve kabuğun verdiği belge taranıyor.
    for (const markup of [html, scanningHtml]) {
      expect(markup).not.toContain('class="thumbnail-list"');
      expect(markup).not.toContain('class="inspector');
      expect(markup).not.toContain('class="toolbar-actions"');
      expect(markup).not.toContain("Seçili araç");
      expect(markup).toMatch(/class="pdf-workspace-layout" data-pages="false"/);
    }
    // Tek sütun: kalan alanı hata/boş durum kullanır.
    for (const width of [1000, 1120, 1440]) {
      const rules = declarations(".pdf-root .pdf-workspace-layout", width);
      expect(rules.display).toBe("grid");
      expect(rules["grid-template-columns"]).toBe("minmax(0, 1fr)");
    }
  });

  it("geçerli PDF ve sayfalar oluştuğunda çalışma yüzeyleri geri gelir", () => {
    expect(workspaceSurfaces("ready", 3)).toEqual({ tools: true, strip: true });
    // Kullanıcı yeni girdi bekleyen bir araca geçtiğinde (Görseller → PDF)
    // oturum sürer: panel kalır ki belge eklenebilsin, yalnız şerit kapanır.
    expect(workspaceSurfaces("ready", 0)).toEqual({ tools: true, strip: false });

    // Bileşen üç yüzeyi de bu karara bağlar.
    expect(source).toContain("const surfaces = workspaceSurfaces(docState, order.length);");
    expect(source).toMatch(/\{surfaces\.tools && <ToolbarActions>/);
    expect(source).toMatch(/\{surfaces\.tools && <InspectorPanel title="Araçlar" scope="pdf-root">/);
    expect(source).toMatch(
      /data-pages=\{surfaces\.strip\}>\s*\{surfaces\.strip && <aside className="thumbnail-list"/,
    );
    // Sayfa varken şerit kendi sütununu alır; belge alanı en geniş yüzey kalır.
    expect(
      declarations('.pdf-root .pdf-workspace-layout[data-pages="true"]', 1440)[
        "grid-template-columns"
      ],
    ).toBe("124px minmax(0, 1fr)");
  });
});

describe("açma hatası sunumu", () => {
  /**
   * Motorun gerçek hata metinleri. Rust kaynağından birebir alındı
   * (`ekler-core/src/scanner.rs`, `pdf-core/src/error.rs`); sözleşme
   * değişirse bu liste de değişmeli.
   */
  const ENGINE_MESSAGES = [
    "Error: Doğrulama hatası (Validation Failed): PDF dönüşümü için LibreOffice bulunamadı. LibreOffice'i bu bilgisayara kurup yeniden deneyin. Microsoft Word gerekli değildir.",
    "Error: 'dilekce.pdf' okunamadı: Bozuk veya geçersiz PDF dosyası: xref tablosu okunamadı.",
    "Error: 'dilekce.udf' okunamadı: geçersiz UDF yapısı (content.xml yok).",
    "Error: 'tarama.tif' okunamadı: Bozuk veya geçersiz görsel dosyası: desteklenmeyen sıkıştırma.",
    "Error: 'rapor.key' için desteklenmeyen dosya biçimi (.key).",
    "Error: 'dilekce.pdf' dosyası bulunamadı veya erişilemez.",
    "Error: 'Belgeler' bir klasördür, lütfen dosya seçin.",
    "Error: 'dilekce.pdf' sağlama toplamı hesaplanamadı: Permission denied (os error 13).",
    "Error: Belge tarama sırasında değişti; yeniden ekleyin",
    "Error: Doğrulama hatası (Validation Failed): Sayfa sayısı beklenenden farklı.",
    "TypeError: Cannot read properties of undefined (reading 'sources')",
    "command duzenek_scan_source_files not found",
  ];

  /** Kullanıcı metninde asla bulunmayacak izler. */
  const LEAKS = [
    "Error:",
    "Validation Failed",
    "Doğrulama hatası",
    "TypeError",
    "os error",
    "duzenek_",
    "invoke",
    ".rs",
    "/Users/",
    "command",
    "undefined",
  ];

  it("failed — kullanıcı metninde teknik önek, sınıf adı ya da yol yok", () => {
    for (const raw of ENGINE_MESSAGES) {
      const detail = describeOpenFailure(raw);
      for (const leak of LEAKS) expect(detail).not.toContain(leak);
      // Kısa ve eyleme dönük: tek cümlelik bir yönlendirme.
      expect(detail.length).toBeGreaterThan(20);
      expect(detail.length).toBeLessThan(140);
    }
    // Tanınmayan hata sessizce kaybolmaz, güvenli yedeğe düşer.
    expect(describeOpenFailure(new Error("¿?"))).toBe(OPEN_FAILURE_FALLBACK);
    expect(describeOpenFailure(null)).toBe(OPEN_FAILURE_FALLBACK);
    // Bilinen kategoriler gerçekten tanınır; hepsi yedeğe düşseydi bu test boş olurdu.
    const mapped = ENGINE_MESSAGES.map(describeOpenFailure).filter((d) => d !== OPEN_FAILURE_FALLBACK);
    expect(mapped.length).toBe(ENGINE_MESSAGES.length - 2);
    expect(new Set(mapped).size).toBe(ENGINE_MESSAGES.length - 2);
  });

  it("teknik metin düşer ama motorun kendi cümlesi korunur", () => {
    // Kaydetme sonucu gibi motorun cümlesinin taşındığı yerlerde yalnız sınıf
    // adı ve sarmalayıcı etiket düşer; bilgi kaybolmaz.
    expect(plainMessage(new Error("Hedef dosya boyutu sınırı aşıldı: 9 bayt > 8 bayt sınırı"))).toBe(
      "Hedef dosya boyutu sınırı aşıldı: 9 bayt > 8 bayt sınırı",
    );
    expect(plainMessage("Doğrulama hatası (Validation Failed): Kaynak değişti")).toBe("Kaynak değişti");
    expect(plainMessage("TypeError: x")).toBe("x");
  });

  it("failed — yalnız hata durumu çizilir, boş durum daveti çizilmez", () => {
    const failed = renderToStaticMarkup(
      <PreviewPlaceholder state="failed" detail={describeOpenFailure(ENGINE_MESSAGES[0])} onOpenAnother={() => undefined} />,
    );
    expect(failed).toContain("Belge açılamadı");
    expect(failed).toContain("dönüştürücü");
    // Normal boş durum metni burada YOK: durumlar dışlayıcıdır.
    expect(failed).not.toContain("Belge önizlemesi");
    expect(failed).not.toContain("PDF seçtiğinizde");
    expect(failed).not.toContain("inceleniyor");
    for (const leak of LEAKS) expect(failed).not.toContain(leak);
    // Kart, dev panel ya da teknik ayrıntı alanı yok: iki satır ve bir eylem.
    expect(failed).not.toContain("<svg");
    expect(failed).not.toContain("<details");
    expect((failed.match(/<p/g) ?? []).length).toBe(1);
    // Ekran okuyucuya bildirilir.
    expect(failed).toContain('role="alert"');
  });

  it("failed — en az bir kurtarma eylemi sunulur", () => {
    const failed = renderToStaticMarkup(
      <PreviewPlaceholder state="failed" detail="" onOpenAnother={() => undefined} />,
    );
    expect(failed).toMatch(/<button[^>]*>Başka Belge Aç<\/button>/);
    // Açıklama boş kalırsa güvenli yedek cümle yazılır.
    expect(failed).toContain(OPEN_FAILURE_FALLBACK);
    // Eylem gerçek bir seçiciyi açar ve seçimi kabuğa bildirir.
    expect(source).toMatch(/const openAnother = async \(\) => \{[\s\S]{0,400}?onOpenDocument\(\[picked\]\)/);
    expect(source).toContain("onOpenAnother={openAnother}");
  });

  it("durumlar birbirini dışlar", () => {
    const none = renderToStaticMarkup(<PreviewPlaceholder state="none" detail="" onOpenAnother={() => undefined} />);
    const loading = renderToStaticMarkup(<PreviewPlaceholder state="loading" detail="" onOpenAnother={() => undefined} />);
    expect(none).toContain("Belge önizlemesi");
    expect(none).not.toContain("Belge açılamadı");
    expect(none).not.toContain("inceleniyor");
    expect(loading).toContain("Belgeler inceleniyor…");
    expect(loading).not.toContain("Belge önizlemesi");
    expect(loading).not.toContain("Belge açılamadı");
    // Kabuğun verdiği belge taranırken çizilen gerçek biçimlendirme de öyle.
    expect(scanningHtml).toContain("Belgeler inceleniyor…");
    expect(scanningHtml).not.toContain("Belge önizlemesi");
    // Belge hiç yokken davet durur.
    expect(html).toContain("Belge önizlemesi");
    expect(html).not.toContain("Belge açılamadı");
  });
});

describe("önizleme geometrisi", () => {
  it("preview_zoom_preserves_aspect_ratio", () => {
    for (const [w, h] of [
      [794, 1123],
      [1123, 794],
      [200, 200],
    ])
      for (const rotation of [0, 90, 180, 270])
        for (const zoom of [75, 100, 125, 150, 200]) {
          const g = previewGeometry(w, h, rotation, 500, 450, zoom);
          expect(Math.abs(g.imageWidth / g.imageHeight - w / h)).toBeLessThan(1e-10);
          expect(g.scale).toBe(zoom / 100);
          expect(g.width).toBe(((rotation % 180 ? h : w) * zoom) / 100);
          expect(g.height).toBe(((rotation % 180 ? w : h) * zoom) / 100);
        }
  });

  it("preview_fit_width_no_clipping", () => {
    for (const rotation of [0, 90]) {
      const g = previewGeometry(794, 1123, rotation, 400, 300, "fit-width");
      expect(g.width).toBeLessThanOrEqual(400 - 16);
      expect(g.height).toBeGreaterThan(0);
      expect(declarations(".pdf-root .pdf-page-viewport", 1120).overflow).toBe("auto");
    }
  });

  it("preview_fit_page_no_stretch", () => {
    for (const [w, h] of [
      [794, 1123],
      [1123, 794],
    ])
      for (const rotation of [0, 90]) {
        const g = previewGeometry(w, h, rotation, 400, 300, "fit-page");
        expect(g.width).toBeLessThanOrEqual(384);
        expect(g.height).toBeLessThanOrEqual(284);
        expect(Math.abs(g.imageWidth / g.imageHeight - w / h)).toBeLessThan(1e-10);
      }
  });
});

describe("kabuğa bağlanma", () => {
  /**
   * Birleşik binary'de dört modülün komutları tek isim uzayını paylaşıyor.
   * Öneksiz bir çağrı derlenir, tip denetiminden geçer ve yalnız çalışma
   * anında "command not found" ile ölür — o yüzden kaynak metin üzerinden
   * denetleniyor.
   */
  it("her yerel komut çağrısı duzenek_ önekli", () => {
    const calls = [...source.matchAll(/invoke<[^>]*>\(\s*'([^']+)'/g)].map((m) => m[1]);
    expect(calls.length).toBeGreaterThan(0);
    for (const name of calls) expect(name).toMatch(/^duzenek_/);
    expect(new Set(calls)).toEqual(
      new Set([
        "duzenek_preview_pdf_page",
        "duzenek_scan_source_files",
        "duzenek_pdf_to_images",
        "duzenek_run_pdf_tool",
      ]),
    );
  });

  /**
   * Korunması istenen yüzeyler: sağ önizleme, küçük resimler, seçim, sıra,
   * sola/sağa döndürme, yakınlaştırma, sayfaya/genişliğe sığdır, kaydırma.
   *
   * Belge açılmadan yalnız boş durum çizilir — bağımsız uygulamada da öyle.
   * Bu yüzden açılış biçimlendirmesi `html` üzerinden, belge açıldıktan sonra
   * çizilen yüzeyler kaynak metin üzerinden denetleniyor. Gerçek belgelerle
   * çalışan doğrulama ayrı bir kapıdır (native parity).
   */
  it("taşınan yüzeyler duruyor", () => {
    // Belge yokken yalnız önizleme yüzeyi ve boş durum çizilir; araç paneli
    // belgeyle birlikte gelir (bkz. "durum sözleşmesi").
    for (const marker of ['class="pdf-preview-panel"', "Belge önizlemesi"])
      expect(html).toContain(marker);

    for (const marker of [
      'className="pdf-controls"',
      'className="tool-grid"',
      'className="thumbnail-list"',
      'className="pdf-page-viewport"',
      'aria-label="Kaydırılabilir PDF sayfası"',
      "Sayfaya sığdır",
      "Genişliğe sığdır",
      "↶ Sola 90°",
      "↷ Sağa 90°",
      "Yeni PDF Kaydet",
      "Klasör Seçerek Kaydet",
      "Tümünü İşaretle",
      "Seçimi Temizle",
      "sayfayı yukarı taşı",
      "sayfayı aşağı taşı",
    ])
      expect(source).toContain(marker);
  });

  /** Araçlar eşit ağırlıkta değil: gruplara ayrılmış ve nadir olanlar katlı. */
  it("araç hiyerarşisi var, on bir araç düz bir yığın değil", () => {
    // Sayfa araçlarının TEK evi belgenin üstündeki şerit; panelde ikinci kez
    // çizilmez. Panelde belgenin bütününe ait işler ve katlı nadir işler kalır.
    expect(source).not.toMatch(/title: 'Sayfalar'/);
    expect(source).toMatch(/PAGE_TOOLS: Kind\[\] = \['select', 'reorder', 'delete', 'rotate'\]/);
    for (const group of ["Belge", "Diğer"]) {
      expect(source).toMatch(new RegExp(`title: '${group}'`));
    }
    // Kısa segment etiketi görünürde; tam ad erişilebilir adda kalır.
    expect(source).toMatch(/className=\{`segment [^`]*`\} title=\{tools\[key\]\[1\]\} aria-label=\{tools\[key\]\[0\]\}/);
    // Nadir grup katlı gelir; <details> olmadan hiyerarşi yalnız görsel olurdu.
    expect(source).toContain("<details");
    expect(source).toMatch(/keys: \['crop', 'watermark', 'number', 'images'\], collapsed: true/);
    // Ama seçili araç içindeyse açık açılmalı — kullanıcı aracını kaybetmemeli.
    expect(source).toContain("open={group.keys.includes(kind)}");
  });

  /** On bir araç da taşındı; hiçbiri migration sırasında düşmedi. */
  it("araç listesi eksiksiz", () => {
    for (const label of [
      "Birleştir",
      "Seçili sayfalar → yeni PDF",
      "Sayfa sırasını değiştir",
      "Sayfa sil",
      "Döndür",
      "Sıkıştır",
      "Görseller → PDF",
      "Kırp",
      "Filigran",
      "Sayfa numarası",
      "PDF → PNG/JPG",
    ])
      expect(source).toContain(label);
  });
});
