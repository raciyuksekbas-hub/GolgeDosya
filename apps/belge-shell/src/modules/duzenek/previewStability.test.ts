/**
 * Önizleme kararlılığı: kullanıcı hiçbir şey yapmazken sayfa titrememeli.
 *
 * Saha: "PDF belgem ekranda tir tir titriyor, hiç durmuyor; Genişliğe
 * Sığdır'a geçince düzeldi." Kök neden: sığdırma ölçeği kaydırma kutusunun
 * clientWidth/clientHeight'ından hesaplanıyordu; sayfanın KENDİ yarattığı
 * kaydırma çubuğu bu ölçüyü küçültüyordu ve sahne bir kare eski ölçüyle
 * boyutlanıyordu. Büyük ölçüde taşan, küçük ölçüde sığan bir sayfa her karede
 * iki durum arasında gidip geldi. Tetikleyiciler (başsız Chromium'da yerel
 * olarak yeniden üretildi):
 *   - Genişliğe sığdır: sayfa boyu kutudan çubuk kalınlığı × oran kadar uzun;
 *   - Sayfaya sığdır: keskin yeniden render başarısız olunca hata cümlesi
 *     sayfanın YANINA konuyordu.
 *
 * jsdom yerleşim yapmaz. Bu yüzden tarayıcının kaydırma çubuğu kuralını
 * kare kare uygulayan küçük bir model kurulur ve ÜRETİM fonksiyonları
 * (previewGeometry, previewStageSize) ile koşulur. Modelin varsayımları
 * (çubuk yeri ayrılıyor mu, sahne kutuyu dolduruyor mu, hata yerleşim dışı
 * mı) ÜRETİM pdf.css'inden okunur. Modelin bu hatayı gerçekten yakaladığı,
 * eski kuralla bilinen salınımın yeniden üretilmesiyle kanıtlanır.
 * Gerçek motor kanıtı: qa/windows/saha-e2e.mjs "önizleme kararlılığı".
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { previewGeometry, previewStageSize, type PreviewMode } from "./pdfWorkspaceState";

const here = dirname(fileURLToPath(import.meta.url));
const css = postcss.parse(readFileSync(resolve(here, "pdf.css"), "utf8"));
function decl(selector: string, prop: string): string | undefined {
  let value: string | undefined;
  css.walkRules((rule) => {
    if (rule.selector.split(",").map((s) => s.trim()).includes(selector))
      rule.walkDecls(prop, (d) => { value = d.value; });
  });
  return value;
}

/** Üretim CSS'inin model varsayımları. */
const PRODUCTION = {
  gutter: decl(".pdf-root .pdf-page-viewport", "scrollbar-gutter") === "stable",
  fill: decl(".pdf-root .pdf-preview-stage", "min-width") === "100%" && decl(".pdf-root .pdf-preview-stage", "min-height") === "100%",
  errorBeside: decl(".pdf-root .pdf-wait-overlay", "position") !== "absolute",
};
/** v0.3.1'in kuralı: çubuk yeri yok, sahne ölçülen (bayat) kutuyla boyutlanır, hata sayfanın yanında. */
const LEGACY = { gutter: false, fill: false, errorBeside: true, legacyStage: true };

type Policy = { gutter: boolean; fill: boolean; errorBeside: boolean; legacyStage?: boolean };
type Page = { w: number; h: number };

/**
 * Tarayıcı modeli: kaydırma kutusu (dış iç boyutu outerW × outerH), yer
 * kaplayan çubuk kalınlığı T. Her kare: ölçü (bir önceki karenin ResizeObserver
 * teslimi) → geometri → sahne → taşma → çubuklar → yeni ölçü. clientWidth/
 * clientHeight çubukları dışlar; `scrollbar-gutter: stable` dikey çubuğun yerini
 * her zaman düşer.
 */
function simulate(outerW: number, outerH: number, page: Page, mode: PreviewMode, policy: Policy, errorWidth: number, T = 17) {
  const cssW = page.w * 96 / 72, cssH = page.h * 96 / 72;
  let v: boolean = false, hbar: boolean = false;
  let measured: { W: number; H: number } = { W: outerW - (policy.gutter ? T : 0), H: outerH };
  const history: string[] = [];
  for (let frame = 0; frame < 16; frame++) {
    const g = previewGeometry(cssW, cssH, 0, measured.W, measured.H, mode);
    const explicit = policy.legacyStage
      ? { width: Math.max(measured.W, g.width + 16), height: Math.max(measured.H, g.height + 16) }
      : previewStageSize(g);
    // Tarayıcı aynı yerleşimde çubukları dengeye getirir (en fazla birkaç tur).
    for (let pass = 0; pass < 4; pass++) {
      const boxW: number = outerW - (policy.gutter || v ? T : 0);
      const boxH: number = outerH - (hbar ? T : 0);
      const stageW: number = policy.fill ? Math.max(boxW, explicit.width) : explicit.width;
      const stageH: number = policy.fill ? Math.max(boxH, explicit.height) : explicit.height;
      // Sahne esnek ve ortalayan bir kutudur: taşan içerik iki yana eşit taşar,
      // kaydırılabilir taşma yalnız sağdaki yarıdır.
      const inner: number = errorWidth > 0 && policy.errorBeside ? g.width + errorWidth : g.width;
      const contentW: number = stageW + Math.max(0, inner - (stageW - 16)) / 2;
      // Tarayıcılar taşmayı piksele yuvarlanmış değerlerle karşılaştırır.
      const nextH: boolean = Math.round(contentW) > Math.round(boxW);
      const nextV: boolean = Math.round(stageH) > Math.round(boxH);
      if (nextH === hbar && nextV === v) break;
      hbar = nextH; v = nextV;
    }
    const next = { W: Math.round(outerW - (policy.gutter || v ? T : 0)), H: Math.round(outerH - (hbar ? T : 0)) };
    history.push(`${next.W}x${next.H} ${g.scale.toFixed(6)}`);
    measured = next; // ResizeObserver bir sonraki karede teslim eder
  }
  return history;
}
const settled = (history: string[]) => new Set(history.slice(-8)).size === 1;

const CORPUS: Record<string, Page> = {
  "A4 portre": { w: 595, h: 842 },
  "A4 yatay": { w: 842, h: 595 },
  "taranmış büyük sayfa": { w: 2480, h: 3508 },
  "Letter": { w: 612, h: 792 },
  "CropBox ≠ MediaBox (400×610 görünür alan)": { w: 400, h: 610 },
  "küçük sayfa (A6)": { w: 150, h: 210 },
};
const MODES: PreviewMode[] = ["fit-page", "fit-width", 75, 100, 125];

describe("önizleme ölçeği saf ve belirlenimci", () => {
  it("aynı kutu + aynı kip + aynı sayfa → her zaman aynı ölçek ve boyut", () => {
    for (const { w, h } of Object.values(CORPUS))
      for (const mode of MODES)
        for (const [W, H] of [[592, 547], [397, 547], [380, 530], [1261, 983]]) {
          const a = previewGeometry(w, h, 0, W, H, mode), b = previewGeometry(w, h, 0, W, H, mode);
          expect(b).toEqual(a);
        }
  });

  it("sığdırma kiplerinde sahne ölçülen kutudan büyük olamaz (kendi başına çubuk doğurmaz)", () => {
    for (const { w, h } of Object.values(CORPUS))
      for (let W = 200; W <= 1400; W += 37)
        for (let H = 200; H <= 1100; H += 41) {
          const page = previewStageSize(previewGeometry(w * 96 / 72, h * 96 / 72, 0, W, H, "fit-page"));
          expect(page.width).toBeLessThanOrEqual(W + 1e-9);
          expect(page.height).toBeLessThanOrEqual(H + 1e-9);
          const width = previewStageSize(previewGeometry(w * 96 / 72, h * 96 / 72, 0, W, H, "fit-width"));
          expect(width.width).toBeLessThanOrEqual(W + 1e-9);
        }
  });
});

describe("üretim CSS'i modelin kararlılık varsayımlarını taşır", () => {
  it("dikey çubuk yeri ayrılır, sahne kutuyu doldurur, hata yerleşim dışıdır", () => {
    expect(PRODUCTION).toEqual({ gutter: true, fill: true, errorBeside: false });
  });
});

describe("model hatayı yakalıyor (negatif kontrol, v0.3.1 kuralı)", () => {
  const oscillates = (mode: PreviewMode, errors: number[]) => {
    const found: string[] = [];
    for (let W = 360; W <= 420; W++)
      for (const H of [530, 547])
        for (const e of errors)
          if (!settled(simulate(W, H, { w: 150, h: 210 }, mode, LEGACY, e))) found.push(`${W}x${H} hata ${e}px`);
    return found;
  };
  it("Genişliğe sığdır, hatasız: eski kural titreyen durumlar üretir (yerelde 397×547 ↔ 380×530 ölçüldü)", () => {
    expect(oscillates("fit-width", [0]).length).toBeGreaterThan(0);
  });
  it("Sayfaya sığdır, hata sayfanın yanında: eski kural titreyen durumlar üretir (yerelde 376×547 ↔ 359×530 ölçüldü)", () => {
    expect(oscillates("fit-page", [10, 20, 30]).length).toBeGreaterThan(0);
  });
  it("Sayfaya sığdır, hatasız: eski kural da sabitti (sahadaki titreme hatayla tetiklenir)", () => {
    expect(oscillates("fit-page", [0])).toEqual([]);
  });
});

describe("kullanıcı hiçbir şey yapmazken önizleme sabit kalır (üretim kuralı)", () => {
  for (const [name, page] of Object.entries(CORPUS))
    for (const mode of MODES)
      for (const errorWidth of [0, 20, 140])
        it(`${name} · ${mode}${errorWidth ? ` · yeniden render hatası (${errorWidth}px)` : ""}`, () => {
          const unsettled: string[] = [];
          for (let W = 300; W <= 1300; W += 3)
            for (const H of [430, 530, 547, 647, 800, 983]) {
              const history = simulate(W, H, page, mode, PRODUCTION, errorWidth);
              if (!settled(history)) unsettled.push(`${W}x${H}: ${history.slice(-4).join(" → ")}`);
              // İlk yüklemenin sınırlı geçişi dışında ölçek değişmez: en geç 3. karede oturur.
              expect(new Set(history.slice(3)).size, `${W}x${H}`).toBe(1);
            }
          expect(unsettled).toEqual([]);
        });
});
