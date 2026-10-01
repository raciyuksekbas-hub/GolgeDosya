// Karşılaştır görsel sözleşmesi — saha maddeleri 20, 24, 25.
//
// Değerler sevk edilen compare.css ve tokens.css'ten okunur.
//  * 24: "Değiştirilen" için yan renk yok — liste satırının kenarı saydam, ray
//    numarası griydi; tür işaretleri yalnız tonla ayrışıyordu.
//  * 25: dinlenen fark işaretleri zemine karşı 1.13–1.28:1 idi (görünmüyordu).
//  * 20: tek kaydırıcı — ayrıntı kutusu panelin içinde ikinci bir kaydırıcıydı;
//    uzun dizgiler paneli yatay taşırıyordu; iğnelenen ray düğümü kabuğu
//    kaydırıyordu.
//  * a09 (v0.3.2): "Renkler tam seçilmiyor gibi" — satır içi fark şeridi
//    (%9 / %13 alfa) zemine karşı 1.13–1.28:1 idi; ekleme 1 px gölge, silme
//    1 px üstü çizgiydi; seçili satırda silinen metin AA'nın altındaydı
//    (4.33 / 4.46). Değişen paragrafın kendisi yalnız 2 px soluk bir kenardı.
import postcss from "postcss";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { clampRailY } from "./ChangeRail";

const here = dirname(fileURLToPath(import.meta.url));
const compare = readFileSync(resolve(here, "compare.css"), "utf8");
const tokens = readFileSync(resolve(here, "../../shared-ui/tokens.css"), "utf8");

function rule(css: string, selector: string): string {
  const esc = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const m = new RegExp(`(?:^|\\n)${esc}\\s*\\{([^}]*)\\}`).exec(css);
  return m?.[1] ?? "";
}
const prop = (body: string, name: string) => new RegExp(`(?:^|[\\s;])${name}:\\s*([^;]+);`).exec(body)?.[1]?.trim();

type Rgba = [number, number, number, number];
const ch = (c: number) => ((c /= 255) <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const lum = (c: Rgba) => 0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2]);
const contrast = (a: Rgba, b: Rgba) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};
const parse = (v: string): Rgba => {
  if (v === "none" || v === "transparent") return [0, 0, 0, 0];
  if (v.startsWith("#")) return [1, 3, 5].map((i) => parseInt(v.slice(i, i + 2), 16)).concat(1) as Rgba;
  const m = /rgba?\((\d+)\s+(\d+)\s+(\d+)(?:\s*\/\s*(\d+)%)?\)/.exec(v)!;
  return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] ? Number(m[4]) / 100 : 1];
};
const over = (fg: Rgba, bg: Rgba): Rgba => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3])).concat(1) as Rgba;

/** var() zincirini compare.css (.compare-root bağlaması) ve tema bloğu üzerinden çözer. */
function resolveVar(name: string, theme: "light" | "dark"): Rgba {
  const block = rule(tokens, theme === "light" ? ":root" : ':root[data-theme="dark"]');
  const local = rule(compare, ".compare-root");
  let v = prop(local, name) ?? prop(block, name) ?? prop(rule(tokens, ":root"), name);
  for (let i = 0; v && v.startsWith("var(") && i < 6; i++) {
    const next = /var\((--[\w-]+)\)/.exec(v)![1];
    v = prop(local, next) ?? prop(block, next) ?? prop(rule(tokens, ":root"), next);
  }
  if (!v) throw new Error(`${name} çözülemedi (${theme})`);
  return parse(v);
}

const KINDS = ["added", "removed", "modified"] as const;

describe("her tür kendi rengini ve biçimini taşır (madde 24)", () => {
  for (const kind of KINDS) {
    it(`${kind}: liste satırının yan çizgisi ve ray numarası renkli`, () => {
      const edge = prop(rule(compare, `.compare-root .change-row.${kind}`), "border-left-color");
      expect(edge, `.change-row.${kind}`).toMatch(/^var\(--diff-/);
      const index = prop(rule(compare, `.rail-node.${kind} .rail-node-index`), "color");
      expect(index, `.rail-node.${kind}`).toMatch(/^var\(--diff-/);
    });
  }

  it("değiştirme işareti BOŞ halka; ekleme ve silme dolu — renk körü için de ayrışır", () => {
    for (const sel of [".compare-root .summary-legend .modified i", ".compare-root .change-row.modified .change-type i"]) {
      const body = rule(compare, sel);
      expect(prop(body, "background"), sel).toBe("transparent");
      expect(prop(body, "box-shadow"), sel).toMatch(/inset 0 0 0 1\.5px var\(--diff-modified\)/);
    }
    expect(prop(rule(compare, ".compare-root .summary-legend .added i"), "background")).toBe("var(--diff-added)");
  });
});

describe("dinlenen işaretler iki temada da görünür (madde 25)", () => {
  for (const theme of ["light", "dark"] as const) {
    for (const kind of KINDS) {
      it(`${theme} · ${kind}: dinlenen çizgi zemine karşı ≥ 3:1, seçiliden zayıf`, () => {
        const paper = resolveVar("--surface-document", theme);
        const rest = contrast(over(resolveVar(`--diff-${kind}-rest`, theme), paper), paper);
        const selected = contrast(over(resolveVar(`--diff-${kind}`, theme), paper), paper);
        expect(rest).toBeGreaterThanOrEqual(3);
        // Seçili fark "yoğunluk artar" ilkesiyle ayrışır: dinlenen tam renk olamaz.
        expect(rest).toBeLessThan(selected - 0.5);
      });
    }
  }
});

describe("tek kaydırıcı (madde 20)", () => {
  it("ayrıntı kutusu kendi içinde kaydırmaz; paneli panel kaydırır", () => {
    const detail = rule(compare, ".compare-root .inspector-detail");
    expect(prop(detail, "overflow")).toBeUndefined();
    expect(prop(detail, "max-height")).toBeUndefined();
    expect(compare).not.toMatch(/\.inspector-detail\s*\{[^}]*max-height/);
  });

  it("uzun kesintisiz dizgiler satırda kırılır; bozuk seçici onarıldı", () => {
    const body = rule(compare, ".row-body");
    expect(prop(body, "overflow-wrap")).toBe("anywhere");
    // Boşluk korunmaz: motor boşlukları normalleştirir; pre-wrap parça
    // ayraçlarını üçlü boşluk olarak gösterirdi.
    expect(prop(body, "white-space")).toBeUndefined();
    expect(compare).not.toContain(".row-.document-row");
  });

  it("iğnelenen ray düğümü rayın dışına taşmaz", () => {
    // Düğüm 20 px ve merkezinden konumlanır: merkez [10, h-10] içinde kalmalı.
    expect(clampRailY(1000, 500)).toBe(490);
    expect(clampRailY(-50, 500)).toBe(10);
    expect(clampRailY(240, 500)).toBe(240);
    expect(clampRailY(5, 12)).toBe(6);
  });
});

// ---------------------------------------------------------------------------
// a09 — değişiklik vurgusu. Kurallar postcss ile okunur: birden fazla seçici
// taşıyan kurallar ve kaskattaki SON değer önemlidir.
const sheet = postcss.parse(compare);
const norm = (s: string) => s.replace(/\s+/g, " ").trim();
/** `selector`ı seçici listesinde taşıyan kurallardaki son `name` değeri. */
function cascaded(selector: string, ...names: string[]): string | undefined {
  let value: string | undefined;
  sheet.walkRules((r) => {
    if (r.parent?.type !== "root") return;
    if (!r.selectors.map(norm).includes(selector)) return;
    r.walkDecls((d) => { if (names.includes(d.prop)) value = d.value.trim(); });
  });
  return value;
}
const colorOf = (v: string | undefined, theme: "light" | "dark"): Rgba => {
  if (!v) return [0, 0, 0, 0];
  const ref = /var\((--[\w-]+)\)/.exec(v);
  return ref ? resolveVar(ref[1], theme) : parse(v);
};
const flat = (layers: Rgba[]): Rgba => layers.reduce((bg, fg) => over(fg, bg));

const FRAGMENTS = ["added", "removed"] as const;

describe("satır içi fark şeridi görünür ve okunur (a09)", () => {
  for (const theme of ["light", "dark"] as const) {
    for (const kind of FRAGMENTS) {
      // Şerit, GERÇEK komşusuna karşı ölçülür: değişen paragrafın tonu ya da
      // seçili satırın yüzeyi. Yalın kâğıda karşı ölçmek tonu iki kez sayardı.
      const paper = () => resolveVar("--surface-document", theme);
      const rowWash = () => colorOf(cascaded(".document-row.kind-modified", "background", "background-color"), theme);
      const selected = () => colorOf(cascaded(".document-row.is-selected", "background", "background-color"), theme);
      const band = () => colorOf(cascaded(`.fragment.${kind}`, "background", "background-color"), theme);
      const selBand = () => colorOf(
        cascaded(`.document-row.is-selected .fragment.${kind}`, "background", "background-color")
          ?? cascaded(`.fragment.${kind}`, "background", "background-color"),
        theme,
      );
      const text = () => {
        const v = cascaded(`.fragment.${kind}`, "color");
        return !v || v === "inherit" ? resolveVar("--text-primary", theme) : colorOf(v, theme);
      };

      it(`${theme} · ${kind}: şerit paragraf tonuna ve seçili yüzeye karşı ≥ 1.35:1`, () => {
        const under = flat([paper(), rowWash()]);
        expect(contrast(flat([under, band()]), under)).toBeGreaterThanOrEqual(1.35);
        const underSel = flat([paper(), selected()]);
        expect(contrast(flat([underSel, selBand()]), underSel)).toBeGreaterThanOrEqual(1.35);
      });

      it(`${theme} · ${kind}: şerit üstündeki metin seçili satırda da AA (≥ 4.5:1)`, () => {
        const onBand = flat([paper(), rowWash(), band()]);
        expect(contrast(over(text(), onBand), onBand)).toBeGreaterThanOrEqual(4.5);
        const onSel = flat([paper(), selected(), selBand()]);
        expect(contrast(over(text(), onSel), onSel)).toBeGreaterThanOrEqual(4.5);
      });

      it(`${theme} · ${kind}: alt/üstü çizgi şeride karşı ≥ 3:1`, () => {
        const deco = cascaded(`.fragment.${kind}`, "text-decoration") ?? "";
        const line = colorOf(/var\(--[\w-]+\)/.exec(deco)?.[0], theme);
        for (const under of [flat([paper(), rowWash(), band()]), flat([paper(), selected(), selBand()])]) {
          expect(contrast(over(line, under), under)).toBeGreaterThanOrEqual(3);
        }
      });

      it(`${theme} · ${kind}: seçili satırda şerit güçlenir, zayıflamaz`, () => {
        expect(selBand()[3]).toBeGreaterThan(band()[3]);
      });
    }

    it(`${theme}: seçili satırın yüzeyi paragraf tonundan ayrışır`, () => {
      for (const kind of KINDS) {
        const paper = resolveVar("--surface-document", theme);
        const wash = flat([paper, colorOf(cascaded(`.document-row.kind-${kind}`, "background", "background-color"), theme)]);
        const sel = flat([paper, colorOf(cascaded(".document-row.is-selected", "background", "background-color"), theme)]);
        expect(contrast(sel, wash), kind).toBeGreaterThanOrEqual(1.05);
      }
    });
  }
});

describe("fark şekli renkten bağımsız (a09)", () => {
  const thickness = (v: string) => Number(/(\d+(?:\.\d+)?)px/.exec(v)?.[1] ?? 0);

  it("ekleme 2 px alt çizgi, silme 2 px üstü çizgi; 1 px gölge yok", () => {
    const added = cascaded(".fragment.added", "text-decoration") ?? "";
    const removed = cascaded(".fragment.removed", "text-decoration") ?? "";
    expect(added).toMatch(/\bunderline\b/);
    expect(thickness(added)).toBeGreaterThanOrEqual(2);
    expect(removed).toMatch(/\bline-through\b/);
    expect(thickness(removed)).toBeGreaterThanOrEqual(2);
    for (const kind of FRAGMENTS) {
      expect(cascaded(`.fragment.${kind}`, "box-shadow") ?? "none").not.toMatch(/inset/);
    }
  });

  it("değişen sözcüğün metni gövde rengindedir; türü şerit ve çizgi taşır", () => {
    for (const kind of FRAGMENTS) {
      expect(cascaded(`.fragment.${kind}`, "color"), kind).toBe("inherit");
    }
  });

  it("bütünüyle eklenen/silinen paragraf baştan sona şeritle ve kalın çizgiyle çizilmez", () => {
    // core/compare.ts tüm paragrafı TEK parça yapar; satır tonu ve kenar
    // yeter. Silinen madde okunur kalmalı.
    for (const kind of FRAGMENTS) {
      const sel = `.document-row.kind-${kind} .fragment.${kind}`;
      expect(cascaded(sel, "background", "background-color"), sel).toMatch(/^(none|transparent)$/);
      expect(thickness(cascaded(sel, "text-decoration") ?? "0px"), sel).toBeLessThanOrEqual(1);
      expect(sheet.toString().indexOf(`${sel}`)).toBeGreaterThan(sheet.toString().indexOf(`.document-row.is-selected .fragment.${kind}`));
    }
  });

  it("ayrıntı panelinde Eski/Yeni metni ana metin rengindedir; etiket anlamı taşır", () => {
    for (const kind of FRAGMENTS) {
      const color = cascaded(`.compare-root .detail-block.${kind} p`, "color");
      expect(color === undefined || color === "var(--text-primary)", kind).toBe(true);
    }
  });
});

describe("vurgu senkron geometrisini bozmaz (a09)", () => {
  // useRowHeightSync.ts rowExtra(): padding-top/bottom ve border-top/bottom
  // İLK satırdan okunur ve bütün satırlara uygulanır. Türe ya da seçime göre
  // dikey ölçü değişirse iki panel kayar.
  const VERTICAL = /^(padding|padding-top|padding-bottom|padding-block.*|border|border-top.*|border-bottom.*|border-block.*|border-width|border-style|margin|margin-top|margin-bottom)$/;

  it("satır türü ve seçimi dikey dolgu ya da kenar eklemez", () => {
    const offenders: string[] = [];
    sheet.walkRules((r) => {
      for (const s of r.selectors.map(norm)) {
        // Yalnız SATIRIN kendisini hedefleyen seçiciler (alt öğe değil).
        if (!/^\.document-row(\.[\w-]+)+(::?[\w-]+)?$/.test(s)) continue;
        if (!/\.(kind-|is-selected)/.test(s) || s.includes("::")) continue;
        r.walkDecls((d) => { if (VERTICAL.test(d.prop)) offenders.push(`${s} { ${d.prop} }`); });
      }
    });
    expect(offenders).toEqual([]);
  });

  it("sol ray bütün satırlarda aynı kalınlıkta (3 px)", () => {
    expect(cascaded(".document-row", "border-left")).toBe("3px solid transparent");
    sheet.walkRules((r) => {
      if (r.selectors.some((s) => /^\.document-row\./.test(norm(s)))) {
        r.walkDecls((d) => { expect(d.prop, r.selector).not.toMatch(/^border-left(-width)?$/); });
      }
    });
  });

  it("seçili satırın yüzeyi paragraf tonundan SONRA gelir ve onu ezer", () => {
    const css = sheet.toString();
    const selectedAt = css.indexOf(".document-row.is-selected {");
    for (const kind of KINDS) {
      expect(css.indexOf(`.document-row.kind-${kind} {`), kind).toBeLessThan(selectedAt);
    }
    expect(prop(rule(compare, ".document-row.is-selected"), "background")).toBe("var(--surface-selected)");
  });
});

describe("yeni fark token'ları iki temada da tanımlı (a09)", () => {
  // darkNative "tanımsız token yok" yalnız adın BİR yerde tanımlı olduğuna
  // bakar; koyu blok unutulursa koyu tema açık temanın alfalarını kullanırdı.
  const NAMES = ["--success-mark", "--danger-mark", "--success-mark-strong", "--danger-mark-strong", "--success-row", "--danger-row", "--review-row"];
  for (const theme of [":root", ':root[data-theme="dark"]']) {
    it(`${theme}`, () => {
      const block = rule(tokens, theme);
      for (const name of NAMES) {
        expect(prop(block, name), `${theme} ${name}`).toMatch(/^rgb\(\d+ \d+ \d+ \/ \d+%\)$/);
      }
    });
  }
});
