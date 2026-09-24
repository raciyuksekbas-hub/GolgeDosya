// Karşılaştır görsel sözleşmesi — saha maddeleri 20, 24, 25.
//
// Değerler sevk edilen compare.css ve tokens.css'ten okunur.
//  * 24: "Değiştirilen" için yan renk yok — liste satırının kenarı saydam, ray
//    numarası griydi; tür işaretleri yalnız tonla ayrışıyordu.
//  * 25: dinlenen fark işaretleri zemine karşı 1.13–1.28:1 idi (görünmüyordu).
//  * 20: tek kaydırıcı — ayrıntı kutusu panelin içinde ikinci bir kaydırıcıydı;
//    uzun dizgiler paneli yatay taşırıyordu; iğnelenen ray düğümü kabuğu
//    kaydırıyordu.
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
