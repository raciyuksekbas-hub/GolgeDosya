// Denetle "çok boşluk"u gösterir (saha maddesi 35; işaret v0.3.2'de ␣).
//
// Motor aralığı doğru veriyordu (TYPO_MULTIPLE_SPACES → char_start/char_end).
// Ama alıntı HTML'de boşlukları daraltıyordu: iki ve yedi boşluk aynı 3,6 px'lik
// şeride, sıfır genişlikli karakter 0 px'e iniyordu. Burada GERÇEK FindingList
// motorun biçimindeki bir bulguyla çizilir.
//
// v0.3.2: işaret "·" metin olarak boşlukların YERİNE yazılıyordu. Noktalar
// üç nokta gibi okunuyordu ve paragraf kopyalanınca boşluklar yerine ekran
// okuyucu metni geliyordu ("Davacı7 boşlukvekili"). Artık gerçek karakter
// DOM'da kalır, ␣ yalnız CSS ile çizilir.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { FindingList, findingKey, visibleRange } from "./ReviewWorkspace";
import type { Finding, Fix } from "./types";

const here = dirname(fileURLToPath(import.meta.url));

function finding(start: number, end: number, fix: Fix | null = null): Finding {
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
    fix,
  };
}

function render(text: string, start: number, end: number, fix: Fix | null = null) {
  const f = finding(start, end, fix);
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

const decode = (s: string) =>
  s
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&amp;/g, "&");

/**
 * Alıntı paragrafından kopyalanan metin: DOM metni, yalnız ekran okuyucuya
 * konan (sr-only) metin hariç. Görünür işaret DOM metni olamaz; olsaydı ya
 * kopyaya girer ya da gerçek boşluğun yerini alırdı.
 */
function copied(html: string): string {
  const p = /<p class="finding-excerpt selectable">([\s\S]*?)<\/p>/.exec(html);
  expect(p, "alıntı paragrafı çizilmeli").not.toBeNull();
  const body = p![1].replace(/<span class="sr-only[^"]*">[^<]*<\/span>/g, "");
  return decode(body.replace(/<[^>]*>/g, ""));
}

const markOf = (html: string) =>
  /<mark data-visible-ws="" aria-hidden="true">([\s\S]*?)<\/mark>/.exec(html)?.[1] ?? "";

describe("boşluk vurgusu görünür, metin değişmez", () => {
  it("yedi boşluk: kopya kaynağın aynısı, yedi işaret, ekran okuyucu '7 boşluk' duyar", () => {
    const text = "Davacı       vekili dilekçesini sunar.";
    const html = render(text, 6, 13);
    expect(copied(html)).toBe(text);
    const mark = markOf(html);
    expect(mark.match(/<span class="ws" data-ch="space"> <\/span>/g)?.length).toBe(7);
    // İşaret DOM metni değildir: ne eski nokta ne yeni ␣ yazılır.
    expect(html).not.toContain("·");
    expect(html).not.toContain("␣");
    expect(html).toContain('<span class="sr-only ws-spoken">7 boşluk</span>');
  });

  it("iki boşluk iki işaret; yedi ile ayırt edilir", () => {
    const text = "Dava,  görevli mahkemede açılmıştır.";
    const html = render(text, 5, 7);
    expect(copied(html)).toBe(text);
    expect(markOf(html).match(/data-ch="space"/g)?.length).toBe(2);
    expect(html).toContain('<span class="sr-only ws-spoken">2 boşluk</span>');
  });

  it("bölünmez boşluk: gerçek karakter kalır, etiket öznitelikte", () => {
    const text = "Davacı beyanda bulundu.";
    const html = render(text, 6, 7);
    expect(copied(html)).toBe(text);
    expect(markOf(html)).toBe('<span class="ws" data-ch="invisible" data-label="[U+00A0]"> </span>');
    expect(html).not.toContain("[U+00A0]<");
    expect(html).toContain('<span class="sr-only ws-spoken">bölünmez boşluk</span>');
  });

  it("sıfır genişlikli karakter ve sekme adıyla görünür", () => {
    expect(visibleRange("​")).toMatchObject({ shown: "[U+200B]", spoken: "görünmeyen karakter (U+200B)" });
    expect(visibleRange("  ")).toMatchObject({ shown: "[U+00A0]␣", spoken: "1 boşluk, bölünmez boşluk" });
    expect(visibleRange("\t")?.shown).toBe("⇥");
    const text = "Davacı\tvekili";
    expect(copied(render(text, 6, 7))).toBe(text);
  });

  it("metin aralığı (ör. yinelenen kelime) olduğu gibi işaretlenir", () => {
    expect(visibleRange("ve ve")).toBeNull();
    const html = render("Taraflar ve ve vekilleri", 9, 14);
    expect(html).toContain("<mark>ve ve</mark>");
  });

  it("düzeltme satırı ␣ ile yazılır; ekran okuyucu sözle duyar", () => {
    const fix: Fix = {
      block_id: "p0",
      char_start: 5,
      char_end: 7,
      original: "  ",
      replacement: " ",
      description: "Tek boşluk",
    };
    const html = render("Dava,  görevli", 5, 7, fix);
    expect(html).toContain('<span aria-hidden="true"><code>␣␣</code> → <code>␣</code></span>');
    expect(html).toContain('<span class="sr-only">2 boşluk yerine 1 boşluk</span>');
    expect(html).not.toContain("·");
  });
});

describe("boşluk işaretinin CSS sözleşmesi", () => {
  const css = postcss.parse(readFileSync(resolve(here, "../../shared-ui/shell.css"), "utf8"));
  const decls = (selector: string, prop: string) => {
    const values: string[] = [];
    css.walkRules((r) => {
      if (r.selectors.includes(selector))
        r.walkDecls(prop, (d) => {
          values.push(d.value);
        });
    });
    return values;
  };
  const MARK = ".finding-excerpt mark[data-visible-ws]";

  it("alıntı boşlukları daraltmaz", () => {
    expect(["pre-wrap", "break-spaces"]).toContain(decls(".finding-excerpt", "white-space").at(-1));
  });

  it("işaretli koşu seçimden düşmez ve satır sonunda bölünmez", () => {
    // user-select:none mark'ta kalırsa gerçek boşluklar kopyadan düşer
    // ("Davacıvekili").
    css.walkRules((r) => {
      if (!r.selectors.some((s) => s.includes("mark[data-visible-ws]"))) return;
      r.walkDecls(/^(-webkit-)?user-select$/, (d) => {
        expect(d.value, `${r.selector} ${d.prop}`).not.toBe("none");
      });
    });
    expect(decls(MARK, "white-space").at(-1)).toBe("pre");
  });

  it("␣ yalnız ::before ile çizilir; eski WebKit için düz içerik satırı önce gelir", () => {
    const space = `${MARK} .ws[data-ch="space"]::before`;
    expect(decls(space, "content")).toEqual(['"\\2423"', '"\\2423" / ""']);
    const tab = `${MARK} .ws[data-ch="tab"]::before`;
    expect(decls(tab, "content")).toEqual(['"\\21E5"', '"\\21E5" / ""']);
    const invisible = `${MARK} .ws[data-ch="invisible"]::before`;
    expect(decls(invisible, "content")).toEqual(["attr(data-label)", 'attr(data-label) / ""']);
    // Gerçek karakter yer kaplamaz; işaret okunur boyda ve birincil renkte.
    expect(decls(`${MARK} .ws`, "font-size").at(-1)).toBe("0");
    expect(decls(`${MARK} .ws::before`, "color").at(-1)).toBe("var(--text-primary)");
    const family = decls(`${MARK} .ws[data-ch="space"]::before`, "font-family").at(-1) ?? "";
    expect(family).toContain("Menlo");
    expect(family).toContain('"Segoe UI Symbol"');
  });

  it("ekran okuyucu metni kopyaya girmez", () => {
    expect(decls(".finding-excerpt .ws-spoken", "user-select").at(-1)).toBe("none");
    expect(decls(".finding-excerpt .ws-spoken", "-webkit-user-select").at(-1)).toBe("none");
  });
});
