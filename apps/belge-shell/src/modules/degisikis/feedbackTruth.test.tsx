// Karşılaştır — ekrandaki sayı ve işaretlerin doğruluğu.
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { diffCounterLabel } from "./CompareWorkspace";
import { DocumentPane } from "./DocumentPane";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";

describe("fark sayacı", () => {
  // Yeniden üretim: yalnız DEĞİŞTİRİLMİŞ fark içeren iki belge açılıp süzgeç
  // "Eklenen"e alınır. `visible` boşalır, `selectedIndex` -1 olur ve sayaç
  // "1 / 0 fark" yazardı: aritmetik olarak imkânsız bir cümle.
  it("boş süzgeçte imkânsız sayı üretmez", () => {
    const label = diffCounterLabel(12, 0, -1);
    expect(label).not.toContain("/ 0");
    expect(label).not.toMatch(/^1 \//);
  });

  it("boş süzgeçte farkın var olduğunu gizlemez", () => {
    // "Fark yok" demek yanlış olurdu: belgede 12 fark VAR, süzgeç tutmuyor.
    expect(diffCounterLabel(12, 0, -1)).toContain("12");
    expect(diffCounterLabel(12, 0, -1)).not.toBe("Fark yok");
  });

  it("gerçekten fark yokken 'Fark yok' der", () => {
    expect(diffCounterLabel(0, 0, -1)).toBe("Fark yok");
  });

  it("normal durumda konum ve toplam doğru", () => {
    expect(diffCounterLabel(12, 5, 2)).toBe("3 / 5 fark");
    // Henüz seçim yokken ilk farkı gösterir, sıfırıncıyı değil.
    expect(diffCounterLabel(12, 5, -1)).toBe("1 / 5 fark");
  });
});

describe("eklenen ve çıkarılan sözcükler", () => {
  // Satırlar GERÇEK motordan gelir; elle kurulmuş bir satır, panelin
  // beklediği blokları taşımadığı için yer tutucuya düşer ve test hiçbir şey
  // kanıtlamaz.
  const comparison = compareDocuments(
    makeBlocks([{ text: "Bedel 100.000 TL'dir." }], "base"),
    makeBlocks([{ text: "Bedel 118.000 TL'dir." }], "revised"),
  );

  const pane = (side: "base" | "revised") =>
    renderToStaticMarkup(
      <DocumentPane
        side={side}
        doc={{ name: "s.docx", blocks: [] } as never}
        rows={comparison.rows}
        changes={[]}
        selectedRows={[]}
        paneRef={{ current: null }}
        rowRefs={{ current: new Map() }}
        onScroll={() => undefined}
      />,
    );

  // Yeniden üretim: ekran okuyucuyla panelleri okuyunca eklenen ve çıkarılan
  // sözcükler düz metin olarak geliyordu — "100.000" ile "118.000" arasındaki
  // fark yalnız GÖRSELDİ. `<span>` "bu bir sözcük" der; `<ins>` ve `<del>`
  // "bu sözcük eklendi / çıkarıldı" der.
  it("çıkarılan sözcük anlamlı etiketle çizilir", () => {
    expect(pane("base")).toContain("<del");
    expect(pane("base")).toContain("100.000");
  });

  it("eklenen sözcük anlamlı etiketle çizilir", () => {
    expect(pane("revised")).toContain("<ins");
    expect(pane("revised")).toContain("118.000");
  });

  it("değişmeyen sözcük işaretlenmez", () => {
    expect(pane("base")).toMatch(/<span[^>]*class="fragment equal"[^>]*>Bedel/);
  });

  it("görünüş değişmez: sınıflar korunur", () => {
    // CSS `.fragment.added` / `.fragment.removed` üstünden çalışır; etiket
    // değişikliği görsel bir yeniden tasarım DEĞİLDİR.
    expect(pane("base")).toContain('class="fragment removed"');
    expect(pane("revised")).toContain('class="fragment added"');
  });
});
