// Koyu tema ve tarayıcının kendi çizdiği parçalar (saha maddesi 8, §46).
//
// Windows'ta (WebView2) açılır listenin penceresini Chromium çizer:
// internal_popup_menu.cc, pencerenin renk şemasını sahibin `color-scheme`inden,
// arka planını da sahibin arka planından alır. Koyu temada `color-scheme`
// yoktu; seçicinin arka planı %5 beyazdı: açık pencerede %92 beyaz yazı —
// 1.00:1. Tema seçicisinin kendisi de bu listeydi.
//
// Değerler sevk edilen CSS'ten okunur.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const tokens = readFileSync(resolve(here, "tokens.css"), "utf8");
const shell = readFileSync(resolve(here, "shell.css"), "utf8");

type Rgba = [number, number, number, number];
const channel = (c: number) => {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};
const lum = ([r, g, b]: Rgba) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
const contrast = (a: Rgba, b: Rgba) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const over = (fg: Rgba, bg: Rgba): Rgba => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3])).concat(1) as Rgba;

function body(css: string, selector: string): string {
  const at = css.indexOf(selector);
  if (at < 0) return "";
  const open = css.indexOf("{", at);
  return css.slice(open + 1, css.indexOf("}", open));
}
function parse(value: string): Rgba {
  const v = value.trim();
  if (v.startsWith("#")) return [1, 3, 5].map((i) => parseInt(v.slice(i, i + 2), 16)).concat(1) as Rgba;
  const m = /rgba?\((\d+)\s+(\d+)\s+(\d+)(?:\s*\/\s*(\d+)%)?\)/.exec(v);
  if (!m) throw new Error(`çözülemeyen renk: ${v}`);
  return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] ? Number(m[4]) / 100 : 1];
}
function token(name: string, theme: "light" | "dark"): Rgba {
  const block = body(tokens, theme === "light" ? ":root {" : ':root[data-theme="dark"]');
  const raw = new RegExp(`${name}:\\s*([^;]+);`).exec(block)?.[1];
  if (!raw) throw new Error(`${name} yok (${theme})`);
  return parse(raw);
}
const decl = (css: string, selector: string, prop: string) =>
  new RegExp(`(?:^|[\\s;{])${prop}:\\s*var\\((--[\\w-]+)\\)`).exec(body(css, selector))?.[1];

/** Chromium'un açılır liste penceresi: tuval şemadan, arka plan seçenekten ya da seçiciden. */
function popup(theme: "light" | "dark") {
  const scheme = /color-scheme:\s*(\w+)/.exec(body(tokens, theme === "light" ? ":root {" : ':root[data-theme="dark"]'))?.[1];
  const canvas: Rgba = scheme === "dark" ? [59, 59, 59, 1] : [255, 255, 255, 1];
  const optionBg = decl(shell, "\noption {", "background-color");
  const selectBg = decl(shell, "\nselect,", "background");
  const bg = over(token((optionBg ?? selectBg)!, theme), canvas);
  const text = over(token(decl(shell, "\noption {", "color") ?? "--text-primary", theme), bg);
  return { scheme, ratio: contrast(text, bg) };
}

describe("açılır liste penceresi okunur (Windows, WebView2)", () => {
  it("koyu temada seçenek metni ≥ 4.5:1", () => {
    expect(popup("dark").ratio).toBeGreaterThanOrEqual(4.5);
  });
  it("açık tema değişmedi", () => {
    expect(popup("light").ratio).toBeGreaterThanOrEqual(4.5);
  });
  it("tema, tarayıcı parçalarına color-scheme ile bildirilir", () => {
    expect(popup("light").scheme).toBe("light");
    expect(popup("dark").scheme).toBe("dark");
  });
});

describe("tanımsız token yok", () => {
  it("var(--x) ile okunan her ad tokens.css'te tanımlı", () => {
    const defined = new Set([...tokens.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
    const files = [
      "shell.css",
      "../modules/degisikis/compare.css",
      "../modules/duzenek/pdf.css",
      "../modules/duzenek/ekler/ekler.css",
    ];
    const missing: string[] = [];
    for (const f of files) {
      const css = readFileSync(resolve(here, f), "utf8");
      const local = new Set([...css.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
      for (const m of css.matchAll(/var\((--[\w-]+)/g)) if (!defined.has(m[1]) && !local.has(m[1])) missing.push(`${f}: ${m[1]}`);
    }
    // Tanımsız ad sessizce düşer: kenarlık hiç çizilmiyordu (--border).
    expect(missing).toEqual([]);
  });
});
