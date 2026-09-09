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
import { previewGeometry, rotatePages } from "./pdfWorkspaceState";

const here = dirname(fileURLToPath(import.meta.url));
const html = renderToStaticMarkup(<PdfWorkspace />);
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
  it("pdf_workspace_two_column_layout", () => {
    expect(html).toMatch(/class="pdf-workspace-layout"><aside class="pdf-controls"/);
    for (const width of [850, 1120, 1440]) {
      const rules = declarations(".pdf-root .pdf-workspace-layout", width);
      expect(rules.display).toBe("grid");
      expect(rules["grid-template-columns"]).toBe("minmax(260px, 310px) minmax(0, 1fr)");
    }
    expect(declarations(".pdf-root .pdf-workspace-layout", 760)["grid-template-columns"]).toBe(
      "minmax(0, 1fr)",
    );
  });

  it("preview_panel_right_side_desktop", () => {
    expect(html).toMatch(
      /<aside class="pdf-controls"[\s\S]*<\/aside><aside class="pdf-preview-panel" aria-label="PDF önizleme çalışma alanı">/,
    );
    expect((html.match(/class="pdf-preview-panel"/g) || []).length).toBe(1);
    expect(html).toMatch(/Belge önizlemesi/);
    for (const width of [850, 1120, 1440]) {
      expect(declarations(".pdf-root .pdf-preview-panel", width).position).toBe("sticky");
      expect(declarations(".pdf-root .pdf-preview-panel", width)["min-width"]).toBe("0");
    }
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
    const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
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
    for (const marker of [
      'class="pdf-preview-panel"',
      'class="pdf-controls"',
      'class="tool-grid"',
      "Belge önizlemesi",
    ])
      expect(html).toContain(marker);

    const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
    for (const marker of [
      'className="thumbnail-list"',
      'className="pdf-page-viewport"',
      'aria-label="Kaydırılabilir PDF sayfası"',
      "Sayfaya sığdır",
      "Genişliğe sığdır",
      "↶ Sola 90°",
      "↷ Sağa 90°",
      "Yeni PDF kaydet",
      "Klasör seçerek kaydet",
      "Tümünü işaretle",
      "Seçimi temizle",
      "sayfayı yukarı taşı",
      "sayfayı aşağı taşı",
    ])
      expect(source).toContain(marker);
  });

  /** Araçlar eşit ağırlıkta değil: üç gruba ayrılmış ve nadir olanlar katlı. */
  it("araç hiyerarşisi var, on bir araç düz bir yığın değil", () => {
    for (const group of ["Sayfalar", "Belge", "Diğer"]) {
      expect(html).toContain(group);
    }
    // Nadir grup katlı gelir; <details> olmadan hiyerarşi yalnız görsel olurdu.
    expect(html).toContain("<details");
    // Ama seçili araç içindeyse açık açılmalı — kullanıcı aracını kaybetmemeli.
    const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
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
      expect(html).toContain(label);
  });
});
