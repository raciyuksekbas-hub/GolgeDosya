import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";
import { buildComparisonViewModel, filterChanges } from "./viewModels/comparisonViewModel";
import { buildReport, buildReportJson, reportFileName, saveReport } from "./report";
import { ChangeRail } from "./ChangeRail";
import { ChangeInspector } from "./ChangeInspector";

const here = dirname(fileURLToPath(import.meta.url));

const BASE = [
  { text: "MADDE 3 — HİZMET BEDELİ" },
  { text: "3.1. Hizmet bedeli KDV hariç 100.000 TL'dir." },
  { text: "MADDE 4 — ÖDEME KOŞULLARI" },
  { text: "4.1. Ödemeler işin tamamlanmasını takiben 30 gün içinde yapılacaktır." },
  { text: "4.2. Gecikme hâlinde temerrüt faizi uygulanır." },
];
const REVISED = [
  { text: "MADDE 3 — HİZMET BEDELİ" },
  { text: "3.1. Hizmet bedeli KDV dahil 118.000 TL'dir." },
  { text: "3.2. Hizmet bedeli sözleşme süresince sabittir." },
  { text: "MADDE 4 — ÖDEME KOŞULLARI" },
  { text: "4.1. Ödemeler işin tamamlanmasını takiben 15 gün içinde yapılacaktır." },
];

function model() {
  return buildComparisonViewModel(compareDocuments(makeBlocks(BASE, "base"), makeBlocks(REVISED, "revised")));
}

describe("karşılaştırma akışı: motordan raya, panele ve rapora", () => {
  it("motor çıktısı ile özet sayıları birebir tutarlıdır", () => {
    const { changes, summary } = model();
    expect(summary.total).toBe(changes.length);
    expect(summary.added + summary.removed + summary.modified).toBe(summary.total);
    expect(summary.addedPct + summary.removedPct + summary.modifiedPct).toBe(100);
  });

  it("ray her fark için bir numaralı düğüm çizer", () => {
    const { changes } = model();
    const markup = renderToStaticMarkup(
      <ChangeRail
        changes={changes}
        selectedIndex={1}
        onSelect={() => undefined}
        onMove={() => undefined}
        onSwap={() => undefined}
        canSwap
        paneRef={createRef<HTMLDivElement>()}
        rowRefs={{ current: new Map() }}
        syncToken="t"
      />,
    );
    const nodes = markup.match(/class="rail-node /gu) ?? [];
    expect(nodes).toHaveLength(changes.length);
    for (const change of changes) expect(markup).toContain(`>${change.displayIndex}<`);
    expect(markup).toContain("is-active");
    expect(markup).toContain(String(changes.length));
  });

  it("seçili fark sağ panelde eski ve yeni metinle gösterilir", () => {
    const { changes, summary } = model();
    const selected = changes.find((change) => change.kind === "modified")!;
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={summary}
        filter="all"
        onFilterChange={() => undefined}
        filteredChanges={changes}
        selectedChange={selected}
        onSelect={() => undefined}
      />,
    );
    expect(markup).toContain("Eski");
    expect(markup).toContain("Yeni");
    // Seçili farkın özeti hemen altındaki listede zaten yazılıyor; panel aynı
    // üç satırı ikinci kez çizmez (gerçek pencerede iki kez okunuyordu).
    expect(markup).not.toContain("detail-summary");
    expect(markup).toContain(selected.sectionLabel);
    expect(markup).toContain(selected.displayIndex);
    // Oran halkası da yüzdeler de kaldırıldı: sayının yanındaki türetilmiş
    // yüzde kararı değiştirmiyordu. Panel kendi grafiğini çizmez.
    expect(markup).not.toContain("<svg");
    expect(markup).not.toContain("%");
    expect(markup).toContain(`>${summary.added}<`);
  });

  it("panel boş yer tutucu çizmez: ilk fark açılışta seçilir", () => {
    // Eskiden panel "Rayda ya da listede bir fark seçin." diye bir yer tutucu
    // açıyordu. Yer tutucu yerine gerçek içerik: workspace ilk farkı seçer.
    const { changes, summary } = model();
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={summary} filter="all"
        onFilterChange={() => undefined} filteredChanges={changes}
        selectedChange={undefined} onSelect={() => undefined}
      />,
    );
    expect(markup).not.toContain("bir fark seçin");
    expect(markup).not.toContain("detail-idle");
    const source = readFileSync(resolve(here, "CompareWorkspace.tsx"), "utf8");
    expect(source).toMatch(/setSelected\(model\?\.changes\[0\]\?\.id\)/);
  });

  it("fark yoksa panel tek satıra iner", () => {
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={{ total: 0, added: 0, removed: 0, modified: 0, addedPct: 0, removedPct: 0, modifiedPct: 0 }}
        filter="all" onFilterChange={() => undefined} filteredChanges={[]}
        selectedChange={undefined} onSelect={() => undefined}
      />,
    );
    expect(markup).toContain("Fark bulunmadı");
    expect(markup).not.toContain("summary-legend");
    expect(markup).not.toContain("change-list");
  });

  it("filtre toplamı değiştirmez, panel görünen alt kümeyi bildirir", () => {
    const { changes, summary } = model();
    const added = filterChanges(changes, "added");
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={summary} filter="added"
        onFilterChange={() => undefined} filteredChanges={added}
        selectedChange={undefined} onSelect={() => undefined}
      />,
    );
    expect(markup).toContain(`${added.length} / ${summary.total} gösteriliyor`);
    expect(summary.total).toBe(changes.length);
  });

  it("rapor güncel karşılaştırmadan üretilir ve bütün farkları içerir", () => {
    const { changes, summary } = model();
    const input = {
      baseName: "sozlesme_v3.docx",
      revisedName: "sozlesme_v4.docx",
      generatedAt: new Date(2026, 7, 21, 9, 30),
      summary,
      changes,
    };
    const html = buildReport(input, "html");
    for (const change of changes) {
      expect(html).toContain(change.displayIndex);
      expect(html).toContain(change.sectionLabel);
    }
    const json = JSON.parse(buildReportJson(input));
    expect(json.summary.total).toBe(changes.length);
    expect(json.changes).toHaveLength(changes.length);
    expect(reportFileName(input, "html")).toBe("GolgeDosya-Karsilastirma-20260821-0930.html");
  });

  it("rapor kaydetme yerel komuta gider, ağ kullanılmaz", async () => {
    const invoker = vi.fn(async () => "/Users/x/Downloads/DegisikIs-Rapor.html");
    const saved = await saveReport("DegisikIs-Rapor.html", new TextEncoder().encode("<html></html>"), "html", invoker);
    expect(saved.path).toContain("Downloads");
    expect(invoker).toHaveBeenCalledTimes(1);
  });
});
