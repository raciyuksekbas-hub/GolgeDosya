// Denetle — durum sözleşmesi.
//
// Bunlar GUI kabul testi DEĞİLDİR: ekran görüntüsü almaz, piksel ölçmezler.
// Gerçek bileşenleri statik biçimlendirmeye çizip sevk edilen CSS'i
// ayrıştırırlar — yani hangi yüzeyin hangi durumda var olduğunu bileşenden
// bağımsız biçimde kanıtlarlar.
//
// Motor çağrılmaz: `AnalysisResult` sözleşmesinin şekli sentetik bir belgeyle
// kurulur. Ciddiyet ve sayı anlamları burada tanımlanmaz, yalnız okunur.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { ReviewChrome, ReviewClear, visibleText } from "./ReviewWorkspace";
import type { AnalysisResult, Finding } from "./types";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "ReviewWorkspace.tsx"), "utf8");
const css = postcss.parse(readFileSync(resolve(here, "../../shared-ui/shell.css"), "utf8"));

function declarations(selector: string): Record<string, string> {
  const result: Record<string, string> = {};
  css.walkRules((rule) => {
    if (!rule.selectors.includes(selector)) return;
    rule.walkDecls((d) => {
      result[d.prop] = d.value;
    });
  });
  return result;
}

/** Sentetik belge — müvekkil belgesi değil, künye alanlarının şekli. */
const DOC: AnalysisResult["document"] = {
  fileName: "ornek-dilekce.docx",
  format: "docx",
  blockCount: 13,
  wordCount: 84,
  charCount: 517,
};

const FINDING: Finding = {
  rule_id: "ornek.kural",
  title: "Noktalama öncesi boşluk",
  severity: "warning",
  confidence: 1,
  message: "Noktadan önce boşluk var.",
  explanation: "Türkçe yazımda noktalama işaretinden önce boşluk bırakılmaz.",
  block_id: "p3",
  location: { blockIndex: 3, runIndex: null, charStart: 10, charEnd: 12, containerPath: null },
  context: null,
  fix: {
    block_id: "p3",
    char_start: 10,
    char_end: 12,
    original: " .",
    replacement: ".",
    description: "Fazla boşluk kaldırılır",
  },
};

/** Sayılar bilerek birbirinden farklı: her biri kendi yuvasında görünmeli. */
const result = (findings: Finding[]): AnalysisResult => ({
  document: DOC,
  blocks: [{ id: "p3", kind: "paragraph", text: "Örnek paragraf .", isHeading: false, level: null }],
  findings,
  errorCount: findings.length === 0 ? 0 : 1,
  warningCount: findings.length === 0 ? 0 : 2,
  reviewCount: findings.length === 0 ? 0 : 3,
  truncatedRules: [],
  elapsedMs: 9,
  profileName: null,
});

const chrome = (findings: Finding[]) =>
  renderToStaticMarkup(
    <ReviewChrome
      result={result(findings)}
      fixable={findings.filter((f) => f.fix !== null)}
      selected={new Set<string>()}
      onApply={() => undefined}
      onSelectAll={() => undefined}
      onClearSelection={() => undefined}
    />,
  );

describe("durum sözleşmesi", () => {
  it("0 bulguda sağ panel açılmaz", () => {
    // Panel üç kez "0" yazan bir sütuna dönüyordu: gerçek bağlam yokken
    // üçüncü kolon hiç açılmaz. Bar eylemi de yok — uygulanacak düzeltme yok.
    expect(chrome([])).toBe("");
    expect(source).toMatch(/if \(result\.findings\.length === 0\) return null;/);
  });

  it("0 bulgu, ana workspace'te sade bir kapanış olarak durur", () => {
    const markup = renderToStaticMarkup(<ReviewClear doc={DOC} />);
    expect(markup).toContain("Bulgu bulunmadı");
    expect(markup).toContain("Bu belge tanımlı kuralların hiçbirine takılmadı.");
    // Gerçek belge istatistiği tek satır künye olarak burada; panelde değil.
    expect(markup).toMatch(/class="empty-meta">13 paragraf · 84 kelime incelendi<\/p>/);
    expect((markup.match(/class="empty-meta"/g) ?? []).length).toBe(1);
    // Kutu, kart, başarı kutusu ya da ikon yok.
    for (const forbidden of ["status", "<svg", "card", "kv-value", "inspector"])
      expect(markup).not.toContain(forbidden);
    const box = declarations(".empty");
    for (const prop of ["background", "background-color", "border", "box-shadow"])
      expect(box[prop]).toBeUndefined();
    // Künye sessiz: ikincil renk, küçük punto.
    const meta = declarations(".empty-meta");
    expect(meta.color).toBe("var(--text-secondary)");
    expect(meta["font-size"]).toBe("var(--text-meta)");
  });

  it("bulgu varken panel ve bağlam geri gelir", () => {
    const markup = chrome([FINDING]);
    expect(markup).toContain('aria-label="Denetim özeti"');
    expect(markup).toContain('class="inspector"');
    // Sayılar ve ciddiyet anlamları değişmedi: her sayı kendi etiketinin yanında.
    for (const [label, count] of [
      ["Kesin hata", 1],
      ["Uyarı", 2],
      ["İncele", 3],
    ] as const)
      expect(markup).toMatch(
        new RegExp(`>${label}</span><span class="kv-value">${count}</span>`),
      );
    // Belge künyesi bulgulu durumda panelde kalır.
    expect(markup).toContain("13 paragraf · 84 kelime");
    // Düzeltilebilir bulgu varsa birincil eylem barda.
    expect(markup).toContain('class="toolbar-actions"');
    expect(markup).toContain("Kopyaya uygula…");
  });

  it("workspace iki durumu da tek yerden seçer", () => {
    expect(source).toMatch(/<ReviewChrome\s/);
    expect(source).toMatch(
      /\{result\.findings\.length === 0 \? \(\s*<ReviewClear doc=\{result\.document\} \/>\s*\) : \(/,
    );
    // Sonuç ekran okuyucuya her iki durumda da bildirilir.
    expect(source).toContain('"İnceleme tamamlandı. Bulgu yok."');
  });
});

describe("düzeltmenin yazımı", () => {
  it("boşluk düzeltmesi görünür yazılır", () => {
    // HTML boşlukları daraltıyor: "iki boşluk → bir boşluk" düzeltmesi
    // paketlenmiş uygulamada iki boş kutu olarak çiziliyordu.
    expect(visibleText("  ")).toBe("··");
    expect(visibleText(" ")).toBe("·");
    expect(visibleText("")).toBe("∅");
    expect(visibleText("\t")).toBe("⇥");
    // Gerçek metin değişmez.
    expect(visibleText("Ek-1'de")).toBe("Ek-1'de");
    expect(visibleText("Ek - 1 ' de")).toBe("Ek·-·1·'·de");
  });
});
