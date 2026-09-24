// Dönüştür sonuç yüzeyi: bilgi alarm gibi görünmez (madde 15), akış ortada
// ve en küçük pencerede yatay taşmaz (madde 17).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { ConvertDone } from "./ConvertWorkspace";
import type { ConversionResult } from "./types";

const here = dirname(fileURLToPath(import.meta.url));
const css = postcss.parse(readFileSync(resolve(here, "../../shared-ui/shell.css"), "utf8"));
const decls = (selector: string) => {
  const out: Record<string, string> = {};
  css.walkRules((r) => {
    if (r.selectors.includes(selector)) r.walkDecls((d) => {
      out[d.prop] = d.value;
    });
  });
  return out;
};

const result = {
  status: "success",
  source: "C:\\Users\\Çağrı Şahin\\Belgeler\\dilekçe.udf",
  source_name: "dilekçe.udf",
  output: "C:\\Users\\Çağrı Şahin\\Documents\\GölgeDosya\\Dönüştürülen Belgeler\\dilekçe.docx",
  output_name: "dilekçe.docx",
  warnings: [
    { code: "PAGE_NUMBER_FIELD_APPROXIMATED", severity: "INFO", title: "Sayfa numarası Word alanı olarak yeniden oluşturuldu ve ortalandı", location: null },
    { code: "EMBEDDED_OBJECT_DROPPED", severity: "LOSS", title: "Gömülü nesneler aktarılamadı", location: "s. 2" },
  ],
  error: null,
  source_unchanged: true,
} as unknown as ConversionResult;

describe("bilgi alarm değildir (madde 15)", () => {
  const html = renderToStaticMarkup(
    <ConvertDone items={[result]} from="UYAP (.udf)" to="Word (.docx)" folder={null} onReveal={() => {}} onNew={() => {}} />,
  );
  const rows = html.match(/<p class="flow-warn"[^>]*>.*?<\/p>/g) ?? [];

  it("bilgi satırı sakin işaretle, kayıp satırı ▲ ile", () => {
    const info = rows.find((r) => r.includes('data-severity="INFO"'))!;
    const loss = rows.find((r) => r.includes('data-severity="LOSS"'))!;
    expect(info).not.toContain("▲");
    expect(info).toContain("Bilgi: Sayfa numarası");
    expect(loss).toContain("▲");
    expect(loss).toContain("Kayıp: Gömülü nesneler");
  });

  it("bilgi rengi uyarı rengi değildir", () => {
    expect(decls('.flow-warn[data-severity="INFO"]').color).toBe("var(--text-secondary)");
    expect(decls(".flow-warn").color).toBe("var(--warning)");
  });
});

describe("akış ortada, yatay taşmaz (madde 17)", () => {
  it("sütun karşılama bloğuyla aynı eksende; yüzde sol boşluk yok", () => {
    const body = decls(".convert-body");
    expect(body["margin-inline"]).toBe("auto");
    expect(body["margin-left"]).toBeUndefined();
    expect(decls(".welcome")["margin-inline"]).toBe("auto");
  });
});
