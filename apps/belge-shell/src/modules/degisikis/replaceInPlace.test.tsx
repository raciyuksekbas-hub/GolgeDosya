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
import { replacedPair } from "./CompareWorkspace";

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
    expect(source).toContain("onReplace={() => void replaceSide(0)}");
    expect(source).toContain("onReplace={() => void replaceSide(1)}");
  });

  it("başlıkta gizli tarayıcı dosya girdisi yok: yolsuz dosya alınmaz", () => {
    const html = renderToStaticMarkup(<PaneHeader doc={doc} side="revised" onReplace={() => undefined} />);
    expect(html).not.toContain('type="file"');
  });
});

describe("§34 — değiştirme kabuğun belge akışından geçer", () => {
  // Saha denetimi: "Değiştir" gizli bir tarayıcı girdisinden yolsuz bir File
  // alıyor, yol yerine dosya ADINI saklıyordu. Bardaki belge adları eskide
  // kalıyor, son belgeler güncellenmiyor, başka kipe geçince ESKİ belge
  // taşınıyordu.
  const body = source.slice(source.indexOf("const replaceSide"), source.indexOf("const scrollFrom"));
  const app = readFileSync(resolve(here, "../../App.tsx"), "utf8");

  it("yalnız istenen taraf değişir, diğeri aynı yol", () => {
    const pair: [string, string] = ["C:\\Belgeler\\temel.docx", "C:\\Belgeler\\değişik.docx"];
    expect(replacedPair(pair, 0, "C:\\Belgeler\\yeni.docx")).toEqual(["C:\\Belgeler\\yeni.docx", pair[1]]);
    expect(replacedPair(pair, 1, "C:\\Belgeler\\yeni.docx")).toEqual([pair[0], "C:\\Belgeler\\yeni.docx"]);
  });

  it("yerel seçici gerçek yolu verir; çift kabukta açılır (bar, son belgeler, kip taşıma)", () => {
    expect(body).toContain("await open({");
    expect(body).toContain("onReplaceDocuments?.(next)");
    expect(body).not.toContain("file.name");
    expect(app).toContain("onReplaceDocuments={openDocuments}");
  });

  it("seçici açılamazsa sessiz kalınmaz", () => {
    expect(body).toContain('logFailure("degisikis replace picker"');
    expect(body).toContain("Belge seçici açılamadı");
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
