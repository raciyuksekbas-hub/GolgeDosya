// Erişilebilirlik regresyonları.
//
// Buradaki her test, ürünü klavye ve ekran okuyucuyla kullanan bir hukukçunun
// GERÇEKTEN takıldığı bir noktayı kilitler. Hiçbiri kozmetik değildir; her
// birinin karşılığı, düzeltmeden önce kendi elimle yeniden ürettiğim bir
// kullanıcı davranışıdır.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { Status } from "./primitives";
import { PdfWorkspace } from "../modules/duzenek/PdfWorkspace";

const here = dirname(fileURLToPath(import.meta.url));
const read = (p: string) => readFileSync(resolve(here, p), "utf8");
const tokens = postcss.parse(read("tokens.css"));
const shell = postcss.parse(read("shell.css"));
const pdfSource = read("../modules/duzenek/PdfWorkspace.tsx");

/** Bir seçici için yürürlükteki bildirimler. */
function decls(sheet: postcss.Root, selector: string): Record<string, string> {
  const out: Record<string, string> = {};
  sheet.walkRules((rule) => {
    if (!rule.selectors.includes(selector)) return;
    rule.walkDecls((d) => {
      out[d.prop] = d.value;
    });
  });
  return out;
}

describe("hareketi azalt", () => {
  // Yeniden üretim: Tercihler → Erişilebilirlik → "Hareketi azalt: Açık".
  // Genel kural her animasyonun süresini 1 ms'ye indiriyor. Bu, bir KEZ oynayan
  // animasyonu söndürür ama `.spinner` gibi `infinite` bir döngüyü saniyede
  // ~1000 tura çıkarır: tercihi açan kullanıcı, kapalıyken olduğundan çok daha
  // saldırgan bir titreme görür.
  it("genel kural tek seferlik animasyonları hâlâ söndürür", () => {
    expect(decls(tokens, ':root[data-motion="reduced"] *')["animation-duration"]).toBe("1ms");
  });

  it("sonsuz dönen gösterge stroba dönüşmez", () => {
    const spinner = decls(tokens, ':root[data-motion="reduced"] .spinner');
    const duration = spinner["animation-duration"];
    expect(duration, "hareketi azalt kipinde .spinner için ayrı süre yok").toBeTruthy();
    const ms = Number(/([\d.]+)ms/.exec(duration)?.[1] ?? 0);
    // İnsan gözünün tek bir turu izleyebildiği, titremeyen bir hız.
    expect(ms).toBeGreaterThanOrEqual(1000);
  });

  it("gösterge tümden kaldırılmaz; meşguliyet geri bildirimi kalır", () => {
    expect(decls(tokens, ':root[data-motion="reduced"] .spinner')["display"]).toBeUndefined();
    expect(decls(tokens, ':root[data-motion="reduced"] .spinner')["animation-name"]).toBeUndefined();
  });
});

describe("canlı durum bölgesi", () => {
  // Yeniden üretim: bir canlı bölge, İÇERİĞİYLE AYNI anda DOM'a girerse ekran
  // okuyucuların çoğu hiçbir şey duyurmaz — bölgenin önce durması, sonra
  // içeriğinin değişmesi gerekir. `{status && <Status/>}` tam olarak bunu
  // yapıyordu: kullanıcı kaydete basıyor, sonuç ekrana yazılıyor, ekran
  // okuyucu susuyordu.
  it("boş durumda bile erişilebilirlik ağacında durur", () => {
    const html = renderToStaticMarkup(<Status tone="info">{""}</Status>);
    expect(html).toContain('role="status"');
    expect(html).toContain('data-empty="true"');
  });

  it("boşken kutusu yer kaplamaz", () => {
    expect(decls(shell, '.status[data-empty="true"]')["margin"]).toBe("0");
  });

  it("boşken meşgul göstergesi çizmez", () => {
    expect(renderToStaticMarkup(<Status tone="busy">{""}</Status>)).not.toContain("spinner");
    expect(renderToStaticMarkup(<Status tone="busy">Hazırlanıyor…</Status>)).toContain("spinner");
  });

  it("Düzenle çalışma alanı bölgeyi belge yokken de çizer", () => {
    // Görseller → PDF aracında sonuç satırı hiç çizilmiyordu: kullanıcı
    // düğmeye basıp hiçbir şey olmadığını sanıyordu.
    expect(renderToStaticMarkup(<PdfWorkspace />)).toContain('role="status"');
  });
});

describe("sayfa kutularının erişilebilir adı", () => {
  // Yeniden üretim: 30 sayfalık bir PDF'te Düzenle → Sayfa Çıkar. Ekran
  // okuyucu her kutuda aynı şeyi söylüyordu: "Çıkar, onay kutusu, işaretli
  // değil". Kullanıcı hangi sayfayı işaretlediğini ayırt edemiyor; yanlış
  // sayfayı çıkarma riski doğrudan.
  it("her kutu hangi sayfa olduğunu söyler", () => {
    expect(pdfSource).toMatch(
      /<input type="checkbox" aria-label=\{`\$\{index \+ 1\}\. sayfa \(kaynak s\. \$\{item\.page\}\)/,
    );
  });

  it("görünen sözcük erişilebilir adın içinde kalır (WCAG 2.5.3)", () => {
    // Sesle komut veren kullanıcı "Dahil et" der; ad bu metni İÇERMELİ.
    const label = /aria-label=\{`[^`]*\$\{pickVerb\}`\}/.test(pdfSource);
    expect(label, "erişilebilir ad görünen etiketi içermiyor").toBe(true);
    expect(pdfSource).toMatch(/\/>\{pickVerb\}<\/label>/);
  });

  it("taşıma düğmeleri görünen sıra numarasıyla konuşur", () => {
    // Sayfalar yeniden sıralandığında görünen numara ÇIKTI sırasıdır; düğme
    // adı KAYNAK sayfasını söylüyordu, ikisi birbirini tutmuyordu.
    expect(pdfSource).toContain("${index + 1}. sayfayı yukarı taşı");
    expect(pdfSource).toContain("${index + 1}. sayfayı aşağı taşı");
  });
});
