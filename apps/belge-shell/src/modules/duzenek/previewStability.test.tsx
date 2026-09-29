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
 * (previewGeometry, previewStageSize, nextViewport) ile koşulur. Modelin
 * varsayımları (çubuk yeri ayrılıyor mu, sahne kutuyu dolduruyor mu, hata
 * yerleşim dışı mı) ÜRETİM pdf.css'inden okunur; sahnenin gerçekten bu
 * boyutla ve bu hata şeridiyle çizildiği ÜRETİM bileşeninin (PreviewStage)
 * çıktısından okunur. Modelin bu hatayı gerçekten yakaladığı, eski kurallarla
 * bilinen salınımların yeniden üretilmesiyle kanıtlanır.
 * Gerçek motor kanıtı: qa/windows/saha-e2e.mjs "önizleme kararlılığı".
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PreviewStage } from "./PdfWorkspace";
import { nextViewport, previewGeometry, previewStageSize, type PreviewMode } from "./pdfWorkspaceState";

const here = dirname(fileURLToPath(import.meta.url));
const css = postcss.parse(readFileSync(resolve(here, "pdf.css"), "utf8"));
const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
/** Seçicinin bildirimi; `atRule` verilirse yalnız o @-kuralının içindeki, verilmezse yalnız en üst düzeydeki. */
function decl(selector: string, prop: string, atRule?: string): string | undefined {
  let value: string | undefined;
  css.walkRules((rule) => {
    const parent = rule.parent?.type === "atrule" ? `@${(rule.parent as postcss.AtRule).name} ${(rule.parent as postcss.AtRule).params}` : undefined;
    if (parent !== atRule) return;
    if (rule.selector.split(",").map((s) => s.trim()).includes(selector))
      rule.walkDecls(prop, (d) => { value = d.value; });
  });
  return value;
}

const VIEWPORT = ".pdf-root .pdf-page-viewport";
const STAGE = ".pdf-root .pdf-preview-stage";
/**
 * Üretim CSS'inin model varsayımları, iki motor için: `scrollbar-gutter`'ı
 * bilen (WebView2, Safari 18.2+) ve bilmeyen (macOS 11–15.1 WebKit'i).
 */
const fill = decl(STAGE, "min-width") === "100%" && decl(STAGE, "min-height") === "100%";
const errorBeside = decl(".pdf-root .pdf-wait-overlay", "position") !== "absolute";
const clipX = decl(`${VIEWPORT}[data-fit]`, "overflow-x") === "hidden";
const ENGINES = {
  "scrollbar-gutter destekli motor": { reserve: decl(VIEWPORT, "scrollbar-gutter") === "stable", fill, errorBeside, clipX },
  "scrollbar-gutter desteksiz WebKit": { reserve: decl(VIEWPORT, "overflow-y", "@supports not (scrollbar-gutter: stable)") === "scroll", fill, errorBeside, clipX },
};
/** v0.3.1'in kuralı: çubuk yeri yok, sahne ölçülen (bayat) kutuyla boyutlanır, hata sayfanın yanında. */
const LEGACY: Policy = { reserve: false, fill: false, errorBeside: true, clipX: false, legacyStage: true };
/** İlk düzeltme (1bcd223) desteksiz WebKit'te: `scrollbar-gutter` yok sayılır, yedek yok. */
const FIRST_FIX_OLD_WEBKIT: Policy = { reserve: false, fill: true, errorBeside: false, clipX: false };

/** `clipX`: sığdırma kiplerinde yatay taşma kırpılır (yatay çubuk hiç çıkmaz). */
type Policy = { reserve: boolean; fill: boolean; errorBeside: boolean; clipX: boolean; legacyStage?: boolean };
type Page = { w: number; h: number };
type Frame = { W: number; H: number; scale: number; v: boolean; h: boolean };

/**
 * Tarayıcı modeli: kaydırma kutusu (dış iç boyutu outerW × outerH), yer
 * kaplayan çubuk kalınlığı T. Her kare: ölçü (bir önceki karenin ResizeObserver
 * teslimi) → geometri → sahne → taşma → çubuklar → yeni ölçü. clientWidth/
 * clientHeight çubukları dışlar; `reserve` dikey çubuğun yerini hep düşer
 * (`scrollbar-gutter: stable` ya da hep çizilen çubuk). `snap`: taşma,
 * Chromium ve WebKit'teki gibi piksele yuvarlanmış değerlerle karşılaştırılır;
 * `false` iken kesirli karşılaştırılır (yuvarlamaya güvenmeyen katı motor).
 */
function simulate(outerW: number, outerH: number, page: Page, mode: PreviewMode, policy: Policy, errorWidth: number, snap = true, T = 17): Frame[] {
  const cssW = page.w * 96 / 72, cssH = page.h * 96 / 72;
  let v: boolean = false, hbar: boolean = false;
  const fit = typeof mode === "string";
  const exceeds = (a: number, b: number) => snap ? Math.round(a) > Math.round(b) : a > b + 1e-9;
  let measured: { width: number; height: number } = { width: Math.round(outerW - (policy.reserve ? T : 0)), height: Math.round(outerH) };
  const frames: Frame[] = [];
  for (let frame = 0; frame < 16; frame++) {
    const g = previewGeometry(cssW, cssH, 0, measured.width, measured.height, mode);
    const explicit = policy.legacyStage
      ? { width: Math.max(measured.width, g.width + 16), height: Math.max(measured.height, g.height + 16) }
      : previewStageSize(g);
    // Tarayıcı aynı yerleşimde çubukları dengeye getirir (en fazla birkaç tur).
    for (let pass = 0; pass < 4; pass++) {
      const boxW: number = outerW - (policy.reserve || v ? T : 0);
      const boxH: number = outerH - (hbar ? T : 0);
      const stageW: number = policy.fill ? Math.max(boxW, explicit.width) : explicit.width;
      const stageH: number = policy.fill ? Math.max(boxH, explicit.height) : explicit.height;
      // Sahne esnek ve ortalayan bir kutudur: taşan içerik iki yana eşit taşar;
      // kaydırılabilir taşma, sahnenin kenar kutusundan sağa taşan yarıdır.
      const inner: number = errorWidth > 0 && policy.errorBeside ? g.width + errorWidth : g.width;
      const contentW: number = stageW + Math.max(0, inner - stageW) / 2;
      const nextH: boolean = !(policy.clipX && fit) && exceeds(contentW, boxW);
      const nextV: boolean = exceeds(stageH, boxH);
      if (nextH === hbar && nextV === v) break;
      hbar = nextH; v = nextV;
    }
    // ResizeObserver bir sonraki karede teslim eder; üretimdeki güncelleme kuralı.
    measured = nextViewport(measured, { clientWidth: Math.round(outerW - (policy.reserve || v ? T : 0)), clientHeight: Math.round(outerH - (hbar ? T : 0)) });
    frames.push({ W: measured.width, H: measured.height, scale: g.scale, v, h: hbar });
  }
  return frames;
}
const key = (f: Frame) => `${f.W}x${f.H} ${f.scale.toFixed(6)}`;
const settled = (frames: Frame[]) => new Set(frames.slice(-8).map(key)).size === 1;

const CORPUS: Record<string, Page> = {
  "A4 portre": { w: 595, h: 842 },
  "A4 yatay": { w: 842, h: 595 },
  "taranmış büyük sayfa": { w: 2480, h: 3508 },
  "Letter": { w: 612, h: 792 },
  "CropBox ≠ MediaBox (400×610 görünür alan)": { w: 400, h: 610 },
  "küçük sayfa (A6)": { w: 150, h: 210 },
};
/** Yakınlaştır listesinin tamamı: varsayılan (Sayfaya sığdır), Genişliğe sığdır ve bütün elle yakınlaştırma değerleri. */
const MODES: PreviewMode[] = ["fit-page", "fit-width", 75, 100, 125, 150, 200];
const WIDTHS = Array.from({ length: 334 }, (_, i) => 300 + 3 * i);
const HEIGHTS = [430, 530, 547, 647, 800, 983];

describe("ölçü kaydırma çubuğundan bağımsız", () => {
  it("aynı kutu → aynı ölçü nesnesi (React yeniden çizmez, render zinciri başlamaz)", () => {
    const previous = { width: 592, height: 547 };
    expect(nextViewport(previous, { clientWidth: 592, clientHeight: 547 })).toBe(previous);
    expect(nextViewport(previous, { clientWidth: 575, clientHeight: 547 })).toEqual({ width: 575, height: 547 });
  });

  for (const [engine, policy] of Object.entries(ENGINES))
    it(`${engine}: sayfa dikeyde taşsa da taşmasa da ölçülen genişlik aynı`, () => {
      let overflowing = 0, fitting = 0;
      for (const page of Object.values(CORPUS))
        for (const mode of MODES)
          for (const W of WIDTHS.filter((_, i) => i % 10 === 0))
            for (const H of HEIGHTS)
              for (const f of simulate(W, H, page, mode, policy, 0)) {
                expect(f.W, `${W}x${H} ${mode}`).toBe(W - 17);
                if (f.v) overflowing++; else fitting++;
              }
      // İki durum da gerçekten yaşandı: özellik boş yere geçmiyor.
      expect(overflowing).toBeGreaterThan(0);
      expect(fitting).toBeGreaterThan(0);
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

describe("sınanan kipler Yakınlaştır listesinin tamamıdır", () => {
  it("arayüzdeki her seçenek sınanıyor", () => {
    const zoom = source.match(/<label className="zoom">[\s\S]*?<\/label>/)?.[0] ?? "";
    const offered = [...zoom.matchAll(/value="(fit-[a-z]+)"/g)].map((m) => m[1] as PreviewMode)
      .concat((zoom.match(/\{\[([\d, ]+)\]\.map/)?.[1] ?? "").split(",").map(Number));
    expect(offered.length).toBeGreaterThan(2);
    expect([...offered].sort()).toEqual([...MODES].sort());
  });
});

describe("üretim CSS'i modelin kararlılık varsayımlarını taşır", () => {
  it("her motorda dikey çubuk yeri ayrılır, sahne kutuyu doldurur, hata yerleşim dışıdır", () => {
    expect(ENGINES).toEqual({
      "scrollbar-gutter destekli motor": { reserve: true, fill: true, errorBeside: false, clipX: true },
      "scrollbar-gutter desteksiz WebKit": { reserve: true, fill: true, errorBeside: false, clipX: true },
    });
    expect(decl(STAGE, "position")).toBe("relative");
  });
});

describe("üretim bileşeni (PreviewStage) sahneyi yalnız sayfa boyuyla çizer", () => {
  const url = "blob:onizleme";
  const box = { width: 592, height: 547 };
  const stageStyle = (html: string) => {
    const style = html.match(/^<div class="pdf-preview-stage" style="([^"]*)"/)?.[1] ?? "";
    return Object.fromEntries(style.split(";").filter(Boolean).map((d) => d.split(":") as [string, string]));
  };
  for (const [name, page] of Object.entries(CORPUS))
    for (const mode of MODES)
      it(`${name} · ${mode}: sahne = sayfa + iç boşluk; kutu ölçüsü sahneye yazılmaz`, () => {
        const geometry = previewGeometry(page.w * 96 / 72, page.h * 96 / 72, 0, box.width, box.height, mode);
        const html = renderToStaticMarkup(<PreviewStage large geometry={geometry} bitmap={{ url }} error="" rotation={0} label="sayfa 1"/>);
        const size = previewStageSize(geometry);
        expect(stageStyle(html)).toEqual({ width: `${size.width}px`, height: `${size.height}px` });
        if (mode === "fit-page" || mode === "fit-width") expect(size.width).toBeLessThanOrEqual(box.width);
      });

  it("sayfa çizilmişken render hatası: sayfanın yanında değil, sahnenin içinde yerleşim dışı şerit", () => {
    const geometry = previewGeometry(200, 280, 0, box.width, box.height, "fit-page");
    const html = renderToStaticMarkup(<PreviewStage large geometry={geometry} bitmap={{ url }} error="Yerel PDF renderer başarısız" rotation={0} label="sayfa 1"/>);
    const paragraphs = html.match(/<p [^>]*>/g) ?? [];
    expect(paragraphs).toHaveLength(1);
    expect(paragraphs[0]).toContain('class="pdf-wait pdf-wait-overlay"');
    expect(html).toContain(`<img src="${url}"`);
    // Sahnenin boyu hata yüzünden değişmez.
    expect(stageStyle(html)).toEqual(stageStyle(renderToStaticMarkup(<PreviewStage large geometry={geometry} bitmap={{ url }} error="" rotation={0} label="sayfa 1"/>)));
  });

  it("önizleme bileşeni sahneyi PreviewStage ile çizer, ölçüyü nextViewport ile günceller, sığdırma kipini kutuya bildirir", () => {
    expect(source).toContain("return <PreviewStage ref={ref} large={large} geometry={geometry}");
    expect(source).toContain("setViewport(previous => nextViewport(previous, target))");
    expect(source).toContain(`<div className="pdf-page-viewport" data-fit={typeof zoom === 'string' ? '' : undefined}`);
  });
});

describe("model hatayı yakalıyor (negatif kontrol, eski kurallar)", () => {
  const oscillates = (mode: PreviewMode, policy: Policy, errors: number[], page: Page = { w: 150, h: 210 }, widths = [360, 420], heights = [530, 547]) => {
    const found: string[] = [];
    for (let W = widths[0]; W <= widths[1]; W++)
      for (const H of heights)
        for (const e of errors)
          if (!settled(simulate(W, H, page, mode, policy, e))) found.push(`${W}x${H} hata ${e}px`);
    return found;
  };
  it("v0.3.1 · Genişliğe sığdır, hatasız: titreyen durumlar üretir (yerelde 397×547 ↔ 380×530 ölçüldü)", () => {
    expect(oscillates("fit-width", LEGACY, [0]).length).toBeGreaterThan(0);
  });
  it("v0.3.1 · Sayfaya sığdır, hata sayfanın yanında: titreyen durumlar üretir (yerelde 376×547 ↔ 359×530 ölçüldü)", () => {
    expect(oscillates("fit-page", LEGACY, [20, 30, 40, 50, 60]).length).toBeGreaterThan(0);
  });
  it("v0.3.1 · Sayfaya sığdır, hatasız: eski kural da sabitti (sahadaki titreme hatayla tetiklenir)", () => {
    expect(oscillates("fit-page", LEGACY, [0])).toEqual([]);
  });
  it("yedeksiz ilk düzeltme, scrollbar-gutter desteksiz WebKit · Genişliğe sığdır: titrer (gerçek WKWebView'da 49/808 pencere ölçüldü)", () => {
    expect(oscillates("fit-width", FIRST_FIX_OLD_WEBKIT, [0], CORPUS["A4 portre"], [500, 1300], HEIGHTS).length).toBeGreaterThan(0);
  });
});

describe("piksel altı farklar döngü kuramaz (yuvarlamaya güvenmeyen katı motor)", () => {
  // Kutu kesirli (Windows %125/%150 ölçek); ölçü yuvarlanmış tamsayıdır ve
  // gerçek kutudan yarım piksele kadar büyük olabilir.
  const sweep = (policy: Policy, page: Page, mode: PreviewMode) => {
    const found: string[] = [];
    for (let W = 400; W <= 1300; W++)
      for (const fx of [0, 0.3, 0.6])
        for (const H of [530.4, 547.6, 647.3, 800.5])
          if (!settled(simulate(W + fx, H, page, mode, policy, 0, false))) found.push(`${W + fx}x${H}`);
    return found;
  };
  for (const [engine, policy] of Object.entries(ENGINES))
    for (const [name, page] of Object.entries(CORPUS))
      for (const mode of ["fit-page", "fit-width"] as PreviewMode[])
        it(`${engine} · ${name} · ${mode}`, () => { expect(sweep(policy, page, mode)).toEqual([]); });
  it("negatif kontrol: yatay taşma kırpılmazsa katı motorda Sayfaya sığdır döngü kurar", () => {
    const unclipped = { ...ENGINES["scrollbar-gutter destekli motor"], clipX: false };
    const found = Object.values(CORPUS).flatMap((page) => sweep(unclipped, page, "fit-page"));
    expect(found.length).toBeGreaterThan(0);
  });
});

describe("kullanıcı hiçbir şey yapmazken önizleme sabit kalır (üretim kuralı)", () => {
  for (const [engine, policy] of Object.entries(ENGINES))
    for (const [name, page] of Object.entries(CORPUS))
      for (const mode of MODES)
        for (const errorWidth of [0, 20, 140])
          it(`${engine} · ${name} · ${mode}${errorWidth ? ` · yeniden render hatası (${errorWidth}px)` : ""}`, () => {
            const unsettled: string[] = [];
            for (const W of WIDTHS)
              for (const H of HEIGHTS) {
                const frames = simulate(W, H, page, mode, policy, errorWidth);
                if (!settled(frames)) unsettled.push(`${W}x${H}: ${frames.slice(-4).map(key).join(" → ")}`);
                // Ölçek ve kutu ilk kareden itibaren tek değerdir: ilk yükleme bile
                // ikinci bir ölçü/ölçek geçişi doğurmaz.
                expect(new Set(frames.map(key)).size, `${W}x${H}`).toBe(1);
              }
            expect(unsettled).toEqual([]);
          });
});
