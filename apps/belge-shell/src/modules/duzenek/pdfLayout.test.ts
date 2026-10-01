// Düzenle — belgenin üstündeki şeridin (Seç · Sırala · Sil · Döndür, ↷ Döndür,
// Yakınlaştır) yerleşim sözleşmesi.
//
// Sahada görülen: şerit sarmıyordu ve segment daralabiliyordu. 1280×800'de
// metin %150 iken "Döndür", Döndür kipinde "Sırala · Sil · Döndür"; %200'de
// dördü birden kırpılıyordu. Araçlar paneli dar pencerede açılınca 900 px'te
// segmentin genişliği 0'a iniyordu. Ölçümler headless Chrome ile alındı
// (v032 a03 kanıt dizini); burada o ölçümleri sabitleyen CSS sözleşmesi ve
// sarma sonrası her öğenin satıra sığdığını gösteren geometri durur.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";
import { describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const css = postcss.parse(readFileSync(resolve(here, "pdf.css"), "utf8"));
const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");

/** Medya sorgusu dışındaki bildirimler (şerit kuralları sorgusuzdur). */
function decls(selector: string): Record<string, string> {
  const out: Record<string, string> = {};
  css.walkRules((rule) => {
    if (rule.parent?.type === "atrule") return;
    if (!rule.selectors.includes(selector)) return;
    rule.walkDecls((d) => {
      out[d.prop] = d.value;
    });
  });
  return out;
}

/**
 * Şeridin öğeleri, metin ölçeğine göre ölçülen doğal genişlikleriyle (px).
 * Chrome/macOS'ta ölçüldü; Windows yazı tipleri için %10 pay bırakılır.
 */
const ITEMS: Record<number, { segment: number; segmentRotate: number; rotateButton: number; zoom: number }> = {
  1: { segment: 196, segmentRotate: 197, rotateButton: 70.2, zoom: 191.8 },
  1.5: { segment: 245, segmentRotate: 246, rotateButton: 93.9, zoom: 252.2 },
  2: { segment: 295, segmentRotate: 297.5, rotateButton: 116.8, zoom: 314.5 },
};

/**
 * Sarabilen bir flex satırında her öğe kendi doğal genişliğinde kalır; tek
 * koşul, en geniş öğenin satıra sığmasıdır. Satır sayısını da döndürür.
 */
function wrapLines(items: number[], line: number, gap: number) {
  let lines = 1;
  let used = 0;
  for (const w of items) {
    if (w > line) return { fits: false, lines };
    if (used > 0 && used + gap + w > line) {
      lines += 1;
      used = w;
    } else used += (used > 0 ? gap : 0) + w;
  }
  return { fits: true, lines };
}

describe("şerit sözleşmesi", () => {
  it("segment hiç daralmaz; şerit sığmayınca alt satıra iner", () => {
    expect(decls(".pdf-root .tool-segment").flex).toBe("none");
    const strip = decls(".pdf-root .pdf-strip");
    expect(strip["flex-wrap"]).toBe("wrap");
    // Satırlar arasında nefes payı; sütun aralığı eskisi gibi 12 px.
    expect(strip.gap).toBe("var(--space-2) var(--space-3)");
    // Alt satıra inen yakınlaştırma sağda kalır; onu sağa iten artık kendi
    // kenar payı. 16 px'lik boşluk öğesi şeridi 1093 px'te boş yere ikinci
    // satıra itiyordu.
    expect(decls(".pdf-root .pdf-strip .zoom")["margin-left"]).toBe("auto");
    expect(decls(".pdf-root .pdf-strip .toolbar-spacer").display).toBe("none");
    // Şeridin dört aracı ve Döndür kipinin ek düğmesi şeritte kalır.
    expect(source).toMatch(/<div className="pdf-strip">\s*<div className="tool-segment" role="group"/);
    expect(source).toMatch(/kind === 'rotate' && <div className="row">/);
  });

  it("Döndür kipinde %100/%150/%200 metinle her öğe tam görünür", () => {
    // Model, şeridin sardığını ve segmentin daralmadığını varsayar; ikisi de
    // CSS'te sabit olmalı. Sarma yokken öğeler satıra sığmayınca daralıp
    // kırpılıyordu (1280×800 @%150 Döndür kipi: segment 84/246 px).
    expect(decls(".pdf-root .pdf-strip")["flex-wrap"]).toBe("wrap");
    expect(decls(".pdf-root .tool-segment").flex).toBe("none");
    // Belge alanının şerit için kalan iç genişliği (şerit yatay dolgusu 2×16).
    // Değerler, Araçlar panelinin yeni kuralıyla ölçülen belge alanları:
    const lines = [
      { scale: 1, previewPanel: 576 }, // 900×600, panel kapalı
      { scale: 1, previewPanel: 457 }, // 1093×614 (1366×768 @%125), panel açık
      { scale: 1, previewPanel: 624 }, // 1280×800, panel açık
      { scale: 1.5, previewPanel: 490 }, // 900×600, panel kapalı
      { scale: 1.5, previewPanel: 514 }, // 1280×800, panel açık
      { scale: 2, previewPanel: 490 }, // 900×600, panel kapalı
      { scale: 2, previewPanel: 564 }, // 1440×900, panel açık
      { scale: 2, previewPanel: 720.8 }, // 1280×800, panel kapalı
    ];
    for (const { scale, previewPanel } of lines) {
      const item = ITEMS[scale];
      const width = previewPanel - 32;
      const items = [item.segmentRotate, item.rotateButton, item.zoom].map((w) => w * 1.1);
      const result = wrapLines(items, width, 12);
      expect(result.fits, `%${scale * 100}, belge alanı ${previewPanel}px`).toBe(true);
      expect(result.lines).toBeLessThanOrEqual(3);
    }
    // 1366×768 @%125'te panel açıkken Seç kipinde şerit tek satır kalır:
    // önizleme yüksekliğinden boşuna pay alınmaz.
    expect(wrapLines([ITEMS[1].segment, ITEMS[1].zoom], 457 - 32, 12).lines).toBe(1);
  });
});

describe("imzalı belgede kaydetme", () => {
  /**
   * Kaydetme, imzalı belgede Araçlar panelindeki onay kutusuna bağlıdır. Panel
   * kapalıyken (dar pencere) iki kaydetme düğmesi nedensiz sönük duruyordu ve
   * onay kutusu görünmüyordu: kullanıcı kaydedemiyor, nedenini de göremiyordu.
   */
  it("neden kaydetmenin yanında görünür ve paneli açıp onaya götürür", () => {
    const bar = source.slice(source.indexOf("<ToolbarActions>"), source.indexOf("</ToolbarActions>"));
    expect(source).toContain("const needsApproval = sources.some(s => s.is_signed) && !approved;");
    // Neden, kaydetme düğmelerinden ÖNCE ve aynı barda.
    expect(bar).toMatch(
      /\{needsApproval && !busy && <Button variant="quiet" className="save-reason" onClick=\{showApproval\}>İmza Onayı Gerekli<\/Button>\}[\s\S]*Klasör Seçerek Kaydet[\s\S]*Yeni PDF Kaydet/,
    );
    // Tıklanınca panel açılır ve odak onay kutusuna gider.
    expect(source).toContain("const revealTools = useRevealInspector();");
    expect(source).toMatch(/const showApproval = \(\) => \{ revealTools\(\); requestAnimationFrame\(\(\) => approvalRef\.current\?\.focus\(\)\); \};/);
    expect(source).toMatch(/<label className="approval"><input ref=\{approvalRef\} type="checkbox"/);
    // Kaydetme kuralı değişmedi: onaysız imzalı belge kaydedilmez.
    expect(source).toMatch(/const cannotSave = busy \|\| !sources\.length \|\| \(sources\.some\(s => s\.is_signed\) && !approved\)/);
  });
});
