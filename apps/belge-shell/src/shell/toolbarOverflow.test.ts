// Yardımcı barın taşma sözleşmesi.
//
// Metin %150–%200 iken dar pencerede kipin eylemleri barı taşırıyordu:
// `.content { overflow: hidden }` taşan ucu kesiyor, panel düğmesi ve Kapat
// ekranın dışında kalıyordu (1024×700 @%200: düğme x=1016–1072, Kapat
// x=1080–1167). Panel o genişliklerde kapalı geldiği için araçlara hiçbir
// yoldan ulaşılamıyordu. Eylemler artık kendilerine ayrılan alanı aşınca alt
// satıra iner; %100'de hiçbir izin verilen pencerede sarma olmaz.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";
import { describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const css = postcss.parse(readFileSync(resolve(here, "../shared-ui/shell.css"), "utf8"));

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

const sidebar = (vw: number, scale: number) => Math.min(220 * scale, vw * 0.34);
/** `.toolbar-actions` için CSS'te bildirilen `max-width`, px olarak. */
function actionsMax(vw: number, scale: number): number {
  const declared = decls(".toolbar-actions")["max-width"] ?? "";
  const m = declared.match(
    /^calc\(100vw - var\(--sidebar-w\) - (\d+)px - (\d+)px \* var\(--text-scale\)\)$/,
  );
  if (!m) throw new Error(`.toolbar-actions max-width beklenmiyor: "${declared}"`);
  return vw - sidebar(vw, scale) - Number(m[1]) - Number(m[2]) * scale;
}

describe("yardımcı bar taşmaz", () => {
  it("eylemler alanını aşınca alt satıra iner; panel düğmesi ve Kapat hiç daralmaz", () => {
    const actions = decls(".toolbar-actions");
    expect(actions["flex-wrap"]).toBe("wrap");
    expect(actions["justify-content"]).toBe("flex-end");
    expect(actions["max-width"]).toBe("calc(100vw - var(--sidebar-w) - 100px - 150px * var(--text-scale))");
    // Kabuğun kendi düğmeleri (panel, Kapat) eskisi gibi daralmaz.
    expect(decls(".toolbar-shell").flex).toBe("none");
    // Bar gerektiğinde uzar: ızgara satırı içerik kadar büyüyebilir.
    expect(decls(".content")["grid-template-rows"]).toBe(
      "var(--titlebar-h) minmax(var(--toolbar-h), auto) minmax(0, 1fr)",
    );
  });

  it("%100'de eylemler sarmaz", () => {
    // Ölçülen eylem dizileri (%100, Chrome/macOS): Düzenle iki kaydetme
    // düğmesi 292.5 px; imzalı belgede "İmza Onayı Gerekli" ile 435 px.
    for (const vw of [900, 1024, 1093, 1280, 1440])
      expect(actionsMax(vw, 1), `${vw} px`).toBeGreaterThanOrEqual(292.5);
    // Neden yazısı yalnız en küçük pencerede (900) iki satıra iner; orada
    // inmeseydi Kapat'ı ekran dışına iterdi.
    for (const vw of [1024, 1093, 1280, 1440])
      expect(actionsMax(vw, 1), `${vw} px, imzalı`).toBeGreaterThanOrEqual(435.1);
  });

  it("%150 ve %200'de panel düğmesi ve Kapat ekranda kalır", () => {
    // Headless Chrome'da ölçülen doğal genişlikler (v032 a03 widths.json):
    // ölçü ("6 sayfa"), kabuk düğmeleri (panel + Kapat), en geniş tek eylem.
    const measured: Record<number, { status: number; shell: number; widest: number }> = {
      1.5: { status: 64.7, shell: 122, widest: 220.1 },
      2: { status: 81, shell: 151.2, widest: 279.9 },
    };
    for (const scale of [1.5, 2])
      for (const vw of [900, 1024, 1093, 1280, 1440]) {
        const m = measured[scale];
        const contentWidth = vw - sidebar(vw, scale);
        // Eylemler sınırına iner, ama en geniş tek düğmesinden dar olmaz.
        const actions = Math.min(Math.max(actionsMax(vw, scale), m.widest), 730);
        // Sol dolgu 24 + dört aralık 48 + boşluk en az 8; sağdaki 16 px'lik
        // dolgu gerektiğinde kullanılır, ekran kenarı aşılmaz.
        const used = 24 + 48 + 8 + m.status + actions + m.shell;
        expect(contentWidth - used, `${vw} px @%${scale * 100}`).toBeGreaterThanOrEqual(0);
      }
    expect(decls(".toolbar-actions")["min-width"]).toBe("min-content");
    expect(decls(".toolbar-spacer")["min-width"]).toBe("var(--space-2)");
  });
});
