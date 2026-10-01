// Sağ panelin açık/kapalı kuralı.
//
// Saha bildirimi: "PDF düzenle aracında sağ araçlar kısmı yalnızca tam ekranda
// gösteriliyor." Neden: Düzenle'nin Araçlar paneli Karşılaştır/Denetle'nin
// Ayrıntılar paneliyle aynı 1280 px eşiğine bağlıydı. 1366×768 @%125 bir
// Windows ekranında büyütülmüş pencere 1093 CSS px'tir; panel `display:none`
// oluyor, yedi araç ve imza onayı görünmüyordu.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import {
  activeToolsChoice,
  INSPECTOR_SLOT_W,
  inspectorShown,
  inspectorToggleLabel,
  TOOLS_MIN_WORKSPACE_W,
  toolsPanelFits,
} from "./inspectorRule";

const here = dirname(fileURLToPath(import.meta.url));
const read = (p: string) => readFileSync(resolve(here, p), "utf8");
const layout = read("Layout.tsx");

/** tokens.css: `--sidebar-w: min(calc(220px * var(--text-scale)), 34vw)`. */
const sidebar = (vw: number, scale: number) => Math.min(220 * scale, vw * 0.34);
/** İçerik sütunu = pencere − kenar çubuğu (Layout'un ölçtüğü `.content`). */
const content = (vw: number, scale: number) => vw - sidebar(vw, scale);

describe("Araçlar paneli: workspace'e yer kaldıkça açık", () => {
  // Headless Chrome'da ölçülen `.content` genişlikleriyle aynı aritmetik
  // (v032 a03 kanıtı: 1280→1060, 1093→873, 1024→804, 900→680; %200'de
  // 1280→844.8, 1440→1000; %150'de 1280→950, 1093→763).
  const cases: Array<[vw: number, scale: number, open: boolean, why: string]> = [
    [1920, 1, true, "1920×1080"],
    [1440, 1, true, "1440×900"],
    [1280, 1, true, "varsayılan pencere 1280×800"],
    [1366 / 1.25, 1, true, "1366×768 @%125, büyütülmüş pencere (1092.8 CSS px)"],
    [1024, 1, false, "1024×700: panel açılsa workspace 492 px kalır"],
    [900, 1, false, "en küçük pencere 900×600"],
    [1920, 1.5, true, "1920 @%150"],
    [1440, 1.5, true, "1440 @%150"],
    [1280, 1.5, true, "1280 @%150: workspace 638 px"],
    [1093, 1.5, false, "1093 @%150: workspace 451 px"],
    [900, 1.5, false, "900 @%150: panel açılınca sayfa kutusu 146×0 px oluyordu"],
    [1920, 2, true, "1920 @%200"],
    [1440, 2, true, "1440 @%200: workspace 688 px"],
    [1280, 2, false, "1280 @%200: workspace 533 px"],
    [1024, 2, false, "1024 @%200: panel açılınca sayfa kutusu yüksekliği 0"],
  ];
  for (const [vw, scale, open, why] of cases)
    it(`${why} → ${open ? "açık" : "kapalı"}`, () => {
      expect(toolsPanelFits(content(vw, scale))).toBe(open);
      expect(
        inspectorShown({ kind: "tools", detailsOpen: false, toolsChoice: null, contentWidth: content(vw, scale) }),
      ).toBe(open);
    });

  it("eşik: panel açıkken workspace'te en az 560 px kalır", () => {
    expect(INSPECTOR_SLOT_W).toBe(312);
    expect(TOOLS_MIN_WORKSPACE_W).toBe(560);
    expect(toolsPanelFits(312 + 560)).toBe(true);
    expect(toolsPanelFits(312 + 559)).toBe(false);
    // Ölçü yoksa (sunucu tarafı çizim) araçlar saklanmaz.
    expect(toolsPanelFits(null)).toBe(true);
  });

  it("panel genişliği token'larla aynı; 300 px'ten daraltılmadı", () => {
    const tokens = postcss.parse(read("../shared-ui/tokens.css"));
    const root: Record<string, string> = {};
    tokens.walkRules(":root", (rule) => {
      if (rule.parent?.type !== "root") return;
      rule.walkDecls((d) => {
        root[d.prop] = d.value;
      });
    });
    expect(root["--inspector-w"]).toBe("300px");
    expect(root["--inspector-inset"]).toBe("12px");
    expect(INSPECTOR_SLOT_W).toBe(300 + 12);
  });

  it("kullanıcının elle seçimi kuralı yener", () => {
    for (const vw of [900, 1024, 1920])
      for (const choice of [true, false])
        expect(
          inspectorShown({ kind: "tools", detailsOpen: !choice, toolsChoice: choice, contentWidth: content(vw, 1) }),
        ).toBe(choice);
  });

  it("seçim yalnız verildiği kipte geçerli", () => {
    expect(activeToolsChoice({ route: "duzenek", open: false }, "duzenek")).toBe(false);
    expect(activeToolsChoice({ route: "duzenek", open: true }, "degisikis")).toBeNull();
    expect(activeToolsChoice(null, "duzenek")).toBeNull();
    // Layout kipten ayrılınca seçimi düşürür; geri dönüldüğünde kural işler.
    expect(layout).toMatch(/if \(toolsChoice && toolsChoice\.route !== current\) setToolsChoice\(null\);/);
    expect(layout).toContain("toolsChoice: activeToolsChoice(toolsChoice, current)");
  });
});

describe("Ayrıntılar paneli (Karşılaştır, Denetle) değişmedi", () => {
  it("1280 eşiği ve kendi durumu", () => {
    expect(layout).toContain('const WIDE = "(min-width: 1280px)";');
    expect(layout).toContain("const [inspectorOpen, setInspectorOpen] = useState(wide);");
    expect(layout).toMatch(/setWide\(query\.matches\);\s*setInspectorOpen\(query\.matches\);/);
  });

  it("Araçlar kuralı ve Araçlar seçimi Ayrıntılar'a sızmaz", () => {
    for (const vw of [900, 1093, 1279, 1280, 1920])
      for (const detailsOpen of [true, false])
        for (const toolsChoice of [true, false, null])
          expect(
            inspectorShown({ kind: "details", detailsOpen, toolsChoice, contentWidth: content(vw, 1) }),
          ).toBe(detailsOpen);
    // Araçlar düğmesi Ayrıntılar durumuna dokunmaz; ayrı durum tutulur.
    const toggle = layout.slice(layout.indexOf("const togglePanel"), layout.indexOf("};", layout.indexOf("const togglePanel")));
    expect(toggle).toMatch(/if \(inspectorKind === "tools"\) setToolsChoice\(/);
    expect(toggle).toMatch(/else setInspectorOpen\(\(v\) => !v\);/);
    // Kipin paneli açma isteği de (imza onayı) türe göre ayrılır.
    expect(layout).toContain('if (kindRef.current === "tools") setToolsChoice({ route: routeRef.current, open: true });');
  });

  it("panel yokken hiçbir şey çizilmez", () => {
    expect(inspectorShown({ kind: null, detailsOpen: true, toolsChoice: true, contentWidth: 2000 })).toBe(false);
  });
});

describe("panel düğmesinin adı", () => {
  it("Düzenle'de panelin adını söyler, diğer kiplerde 'Ayrıntılar'", () => {
    expect(inspectorToggleLabel("tools", true)).toBe("Araçları Gizle");
    expect(inspectorToggleLabel("tools", false)).toBe("Araçları Göster");
    expect(inspectorToggleLabel("details", true)).toBe("Ayrıntıları Gizle");
    expect(inspectorToggleLabel("details", false)).toBe("Ayrıntıları Göster");
    expect(layout).toContain("label={inspectorToggleLabel(inspectorKind, panelOpen)}");
    expect(layout).toContain("hidden={!panelOpen}");
  });
});

describe("pencere sınırları değişmedi", () => {
  it("tauri.conf.json: 1280×800 açılış, en küçük 900×600", () => {
    const conf = JSON.parse(read("../../src-tauri/tauri.conf.json"));
    const main = conf.app.windows[0];
    expect([main.width, main.height, main.minWidth, main.minHeight]).toEqual([1280, 800, 900, 600]);
  });
});
