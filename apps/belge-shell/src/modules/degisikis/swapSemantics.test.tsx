// Taşınan motorun sürüm-değiştirme semantiği ve Fark Özeti erişilebilirliği.
//
// Bağımsız Değişikİş'in "workspaceUx.test.tsx" dosyasından türetildi. Oradaki
// 11 testin 7'si standalone kabuğun kendi yerleşimini ve kenar çubuğunu
// sınıyordu (App, AppSidebar); o kabuk taşınmadığı için o testlerin konusu da
// kalmadı ve emekliye ayrıldılar. Burada tutulanlar, konusu taşınan koda ait
// olanlardır: karşılaştırma motorunun swap davranışı ve taşınan
// ChangeInspector bileşeninin erişilebilirlik sözleşmesi.
//
// Test gövdeleri değiştirilmedi.
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ChangeInspector } from "./ChangeInspector";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";
import { buildComparisonViewModel, summarize } from "./viewModels/comparisonViewModel";
import { buildReportJson } from "./report";

const BASE = [
  { text: "MADDE 3 — HİZMET BEDELİ" },
  { text: "3.1. Hizmet bedeli KDV hariç 100.000 TL'dir." },
  { text: "3.9. Yalnız temel sürümde bulunan hüküm." },
];
const REVISED = [
  { text: "MADDE 3 — HİZMET BEDELİ" },
  { text: "3.1. Hizmet bedeli KDV dahil 118.000 TL'dir." },
  { text: "3.7. Yalnız değişik sürümde bulunan hüküm." },
];

const model = (base: typeof BASE, revised: typeof REVISED) =>
  buildComparisonViewModel(compareDocuments(makeBlocks(base, "base"), makeBlocks(revised, "revised")));

describe("sürüm değiştirme semantiği", () => {
  it("ekleme ve silme yönü karşılıklı olarak yer değiştirir", () => {
    const forward = model(BASE, REVISED);
    const swapped = model(REVISED as never, BASE as never);
    expect(forward.summary.total).toBe(swapped.summary.total);
    expect(swapped.summary.added).toBe(forward.summary.removed);
    expect(swapped.summary.removed).toBe(forward.summary.added);
    expect(swapped.summary.modified).toBe(forward.summary.modified);
  });

  it("swap sonrası numaralandırma ve oranlar yeniden tutarlı üretilir", () => {
    const swapped = model(REVISED as never, BASE as never);
    expect(swapped.changes.map((change) => change.displayIndex))
      .toEqual(swapped.changes.map((_, index) => String(index + 1).padStart(2, "0")));
    expect(swapped.summary.addedPct + swapped.summary.removedPct + swapped.summary.modifiedPct).toBe(100);
  });

  it("rapor verisi swap sonrası yeni yöne göre üretilir", () => {
    const forward = model(BASE, REVISED);
    const swapped = model(REVISED as never, BASE as never);
    const report = (m: typeof forward, baseName: string, revisedName: string) => JSON.parse(buildReportJson({
      baseName, revisedName, generatedAt: new Date(2026, 0, 1), summary: m.summary, changes: m.changes,
    }));
    const before = report(forward, "v3.docx", "v4.docx");
    const after = report(swapped, "v4.docx", "v3.docx");
    expect(before.documents).toEqual({ base: "v3.docx", revised: "v4.docx" });
    expect(after.documents).toEqual({ base: "v4.docx", revised: "v3.docx" });
    expect(after.summary.added).toBe(before.summary.removed);
    expect(after.changes).toHaveLength(before.changes.length);
  });
});

describe("Fark paneli erişilebilirliği", () => {
  /**
   * Panel artık kendi başlığını ve kapatma düğmesini çizmiyor: çerçeveyi kabuk
   * sahipleniyor ve gizleme düğmesi yardımcı barda. Sözleşme aynı kaldı —
   * paneli gizleyen kontrolün erişilebilir bir adı olmalı — yalnız evi değişti.
   */
  it("paneli gizleyen kontrol etiketlidir", () => {
    const layout = readFileSync(
      resolve(import.meta.dirname, "../../shell/Layout.tsx"),
      "utf8",
    );
    expect(layout).toContain('label={inspectorOpen ? "Ayrıntıları gizle" : "Ayrıntıları göster"}');
    expect(layout).toContain("pressed={inspectorOpen}");
  });

  it("panel kendi başlığını ve kapatma düğmesini çizmez", () => {
    const changes = model(BASE, REVISED).changes;
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={summarize(changes)} filter="all"
        onFilterChange={() => undefined} filteredChanges={changes}
        selectedChange={undefined} onSelect={() => undefined}
      />,
    );
    expect(markup).not.toContain("<aside");
    expect(markup).not.toContain("inspector-close");
  });
});
