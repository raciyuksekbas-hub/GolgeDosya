// §32–35 — karşılaştırma belgelerinin YERİNDE değiştirilmesi.
//
// SAHA BULGUSU: karşılaştırma açıldıktan sonra iki kaynak kilitleniyordu.
// Başka bir belge denemek için çalışma alanını kapatıp baştan başlamak
// gerekiyordu. `PaneHeader` bileşeni ve `.pane-header` stilleri bu akış için
// ZATEN vardı — bileşen hiç çizilmiyordu, ölü koddu.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PaneHeader, SIDE_CAPS } from "./DocumentPane";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "CompareWorkspace.tsx"), "utf8");
const css = readFileSync(resolve(here, "compare.css"), "utf8");

const doc = { name: "sozlesme-v3.docx", extension: "docx", size: 2048, blocks: [] } as never;

describe("her iki taraf da değiştirilebilir", () => {
  it("iki panelin de başlığı çizilir", () => {
    expect(source).toContain('<PaneHeader doc={docs[0].doc} side="base"');
    expect(source).toContain('<PaneHeader doc={docs[1].doc} side="revised"');
  });

  it("başlık belgeyi adıyla gösterir ve Değiştir sunar", () => {
    const html = renderToStaticMarkup(
      <PaneHeader doc={doc} side="base" onReplace={() => undefined} />,
    );
    expect(html).toContain("sozlesme-v3.docx");
    expect(html).toContain("Değiştir");
    expect(html).toContain(SIDE_CAPS.base);
  });

  it("temel ve değişik ayrı ayrı değiştirilir", () => {
    expect(source).toContain("void replaceSide(0, f)");
    expect(source).toContain("void replaceSide(1, f)");
  });
});

describe("§34 — replace semantiği", () => {
  const body = source.slice(source.indexOf("const replaceSide"), source.indexOf("const scrollFrom"));

  it("yalnız istenen taraf yeniden çıkarılır", () => {
    expect(body).toContain("next[side] = { path: file.name, doc: extracted }");
    // Diğer taraf kopyalanır, yeniden okunmaz.
    expect(body).toContain("const next: [Loaded, Loaded] = [...current]");
    expect(body).not.toContain("Promise.all");
  });

  it("eski seçim temizlenir: ekranda eskimiş fark kalmaz", () => {
    expect(body).toContain("setSelected(undefined)");
  });

  it("kabuk barı yeni sırayı öğrenir", () => {
    expect(body).toContain("onPairChange?.(next.map((d) => d.path))");
  });

  it("değişiklik duyurulur", () => {
    expect(body).toContain("sürüm ${file.name} ile değiştirildi");
  });

  it("okunamayan belge sessizce geçilmez", () => {
    expect(body).toContain("catch");
    expect(body).toContain("setFailure(message)");
    expect(body).toContain('logFailure("degisikis replace side"');
  });
});

describe("§35 — swap ile değiştirme ayrı eylemlerdir", () => {
  it("swap hâlâ var ve yalnız sırayı çevirir", () => {
    expect(source).toContain("Sürümler değiştirildi. Temel sürüm artık");
    expect(source).toContain("const swapped: [Loaded, Loaded] = [docs[1], docs[0]]");
  });

  it("swap yeniden çıkarma yapmaz", () => {
    const swap = source.slice(source.indexOf("onSwap={"), source.indexOf("canSwap"));
    expect(swap).not.toContain("extractDocument");
  });
});

describe("yerleşim", () => {
  it("panel başlık + gövde olarak kurulur", () => {
    expect(css).toContain(".pane {");
    expect(css).toContain(".pane-header {");
  });
});
