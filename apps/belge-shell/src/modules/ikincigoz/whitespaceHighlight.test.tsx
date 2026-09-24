// Denetle "çok boşluk"u gösterir (saha maddesi 35).
//
// Motor aralığı doğru veriyordu (TYPO_MULTIPLE_SPACES → char_start/char_end).
// Ama alıntı HTML'de boşlukları daraltıyordu: iki ve yedi boşluk aynı 3,6 px'lik
// şeride, sıfır genişlikli karakter 0 px'e iniyordu. Burada GERÇEK FindingList
// motorun biçimindeki bir bulguyla çizilir.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { FindingList, findingKey, visibleRange } from "./ReviewWorkspace";
import type { Finding } from "./types";

const here = dirname(fileURLToPath(import.meta.url));

function finding(start: number, end: number): Finding {
  return {
    rule_id: "TYPO_MULTIPLE_SPACES",
    title: "Çok boşluk",
    severity: "error",
    confidence: 0.9,
    message: `Kelimeler arasında ${end - start} boşluk var.`,
    explanation: "Tek boşluk olmalı.",
    block_id: "p0",
    // Motorun seri hâli: snake_case.
    location: { block_index: 0, char_start: start, char_end: end } as unknown as Finding["location"],
    context: null,
    fix: null,
  };
}

function render(text: string, start: number, end: number) {
  const f = finding(start, end);
  return renderToStaticMarkup(
    <FindingList
      findings={[f]}
      blockText={new Map([["p0", text]])}
      activeKey={findingKey(f)}
      selectedKeys={new Set()}
      onSetActive={() => {}}
      onToggleFix={() => {}}
    />,
  );
}

describe("boşluk vurgusu görünür", () => {
  it("yedi boşluk yedi görünür işaret; ekran okuyucu '7 boşluk' duyar", () => {
    const text = "Davacı       vekili dilekçesini sunar.";
    const html = render(text, 6, 13);
    const mark = /<mark data-visible-ws="" aria-hidden="true">([^<]*)<\/mark>/.exec(html);
    expect(mark?.[1]).toBe("·······");
    expect(html).toContain('<span class="sr-only">7 boşluk</span>');
    // Belgenin kendisi değişmez: çevresindeki metin aynen.
    expect(html).toContain("Davacı");
    expect(html).toContain("vekili dilekçesini sunar.");
  });

  it("sıfır genişlikli karakter ve bölünmez boşluk adıyla görünür", () => {
    expect(visibleRange("​")).toEqual({ shown: "[U+200B]", spoken: "görünmeyen karakter (U+200B)" });
    expect(visibleRange("  ")).toEqual({ shown: "[U+00A0]·", spoken: "1 boşluk, bölünmez boşluk" });
    expect(visibleRange("\t")?.shown).toBe("⇥");
  });

  it("metin aralığı (ör. yinelenen kelime) olduğu gibi işaretlenir", () => {
    expect(visibleRange("ve ve")).toBeNull();
    const html = render("Taraflar ve ve vekilleri", 9, 14);
    expect(html).toContain("<mark>ve ve</mark>");
  });

  it("alıntı boşlukları daraltmaz; görünür işaretler kopyalanmaz", () => {
    const css = postcss.parse(readFileSync(resolve(here, "../../shared-ui/shell.css"), "utf8"));
    const decl = (selector: string, prop: string) => {
      let value: string | undefined;
      css.walkRules((r) => {
        if (r.selectors.includes(selector))
          r.walkDecls(prop, (d) => {
            value = d.value;
          });
      });
      return value;
    };
    expect(["pre-wrap", "break-spaces"]).toContain(decl(".finding-excerpt", "white-space"));
    expect(decl(".finding-excerpt mark[data-visible-ws]", "user-select")).toBe("none");
  });
});
