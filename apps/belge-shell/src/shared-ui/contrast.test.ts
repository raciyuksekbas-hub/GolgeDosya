// Kontrast regresyonu.
//
// Renkler sevk edilen `tokens.css` dosyasından OKUNUR; testte elle
// kopyalanmış bir palet yoktur. Token değişince test onunla birlikte değişir,
// bu yüzden "yeşil ama yalan" olamaz.
//
// Yeniden üretim: ikincil ve soluk metin, açık temada 2.46:1'e kadar
// düşüyordu (kenar çubuğu yüzeyinde). Bu, güneşli bir odada ya da orta yaş
// üstü bir okuyucuda "yazı var ama okunmuyor" demektir; ölçtüğüm metinler
// süs değil: "Yeni bir kopya oluşturulur; kaynak belgeleriniz korunur."
// güvenlik sözü, Karşılaştır'daki satır numaraları ve boş durum yönergeleri.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "tokens.css"), "utf8");

type Rgb = [number, number, number];

const channel = (c: number) => {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};
const luminance = ([r, g, b]: Rgb) =>
  0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
const contrast = (a: Rgb, b: Rgb) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const hex = (h: string): Rgb => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
/** Yarı saydam metin, arkasındaki yüzeyle karışarak görünür. */
const composite = (fg: Rgb, alpha: number, bg: Rgb): Rgb =>
  fg.map((c, i) => c * alpha + bg[i] * (1 - alpha)) as Rgb;

/** Bir kuralın gövdesi — yalnız O bloğun bildirimleri. */
function ruleBody(selector: string): string {
  const at = css.indexOf(selector);
  if (at < 0) throw new Error(`${selector} tokens.css içinde yok`);
  const open = css.indexOf("{", at);
  return css.slice(open + 1, css.indexOf("}", open));
}

/**
 * Temanın KENDİ bloğundaki değeri okur.
 *
 * Kapsam daraltması kasıtlı: `[data-contrast="more"]` bloğu aynı adları
 * `var(--text-primary)` ile ezer. Onu buraya karıştırmak, yüksek kontrast
 * KAPALIYKEN geçerli olan gerçek rengi gizler ve testi yalancı yeşile
 * çevirirdi.
 */
function token(name: string, block: "light" | "dark"): string {
  const body = ruleBody(block === "light" ? ":root {" : ':root[data-theme="dark"]');
  const value = new RegExp(`${name}:\\s*([^;]+);`).exec(body)?.[1]?.trim();
  if (!value) throw new Error(`${name} (${block}) tokens.css içinde yok`);
  return value;
}

/** Metin rengini, üstünde durduğu yüzeye göre gerçek RGB'ye çevirir. */
function resolve_(value: string, bg: Rgb): Rgb {
  if (value.startsWith("#")) return hex(value);
  const rgba = /rgb\(\s*(\d+)\s+(\d+)\s+(\d+)\s*\/\s*([\d.]+)%\s*\)/.exec(value);
  if (!rgba) throw new Error(`çözülemeyen renk: ${value}`);
  const [, r, g, b, a] = rgba;
  return composite([+r, +g, +b], +a / 100, bg);
}

/** Metnin üstünde durabildiği gerçek yüzeyler. */
const SURFACES = ["--surface-sidebar", "--surface-app", "--surface-workspace", "--surface-raised"];

/** WCAG 2.1 AA, normal boyutlu metin. */
const AA = 4.5;

describe.each(["light", "dark"] as const)("%s tema kontrastı", (theme) => {
  const surfaces = SURFACES.map((s) => ({ name: s, rgb: hex(token(s, theme)) }));

  it.each(["--text-secondary", "--text-muted"])("%s her yüzeyde AA'yı geçer", (name) => {
    const raw = token(name, theme);
    for (const surface of surfaces) {
      const ratio = contrast(resolve_(raw, surface.rgb), surface.rgb);
      expect(
        Number(ratio.toFixed(2)),
        `${name} (${theme}) ${surface.name} üstünde ${ratio.toFixed(2)}:1 — AA ${AA}:1 ister`,
      ).toBeGreaterThanOrEqual(AA);
    }
  });
});

describe("saydamlık kontrastı geri düşüremez", () => {
  // `.row-gutter` satır numaralarını `opacity: .6` ile çiziyordu: token AA'yı
  // geçse bile ekranda görünen renk yeniden AA'nın altına iniyordu. Bilgi
  // taşıyan metne opaklık uygulanmaz.
  const compare = readFileSync(
    resolve(dirname(fileURLToPath(import.meta.url)), "../modules/degisikis/compare.css"),
    "utf8",
  );
  it("Karşılaştır satır numaraları soluklaştırılmaz", () => {
    const rule = /\.row-gutter\s*\{([^}]*)\}/.exec(compare)?.[1] ?? "";
    expect(rule).toContain("var(--text-muted)");
    expect(rule, ".row-gutter hâlâ opacity ile soluklaştırılıyor").not.toMatch(/opacity:/);
  });
});
