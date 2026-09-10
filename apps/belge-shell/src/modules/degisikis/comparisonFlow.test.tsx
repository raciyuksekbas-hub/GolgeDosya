import { renderToStaticMarkup } from "react-dom/server";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";
import { buildComparisonViewModel, filterChanges } from "./viewModels/comparisonViewModel";
import { buildReport, buildReportJson, reportFileName, saveReport } from "./report";
import { ChangeRail } from "./ChangeRail";
import { ChangeInspector } from "./ChangeInspector";

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
        changes={changes}
        filter="all"
        onFilterChange={() => undefined}
        filteredChanges={changes}
        selectedChange={selected}
        onSelect={() => undefined}
      />,
    );
    expect(markup).toContain("TEMEL SÜRÜM");
    expect(markup).toContain("DEĞİŞİK SÜRÜM");
    expect(markup).toContain(selected.sectionLabel);
    expect(markup).toContain(selected.displayIndex);
    expect(markup).toContain(`%${summary.addedPct}`);
    // Oran halkası kaldırıldı: 300 px'lik panelde üç sayı ve üç yüzde daha
    // hızlı okunur. Sayılar view model'den; panel kendi grafiğini çizmez.
    expect(markup).not.toContain("<svg");
    expect(markup).toContain(`>${summary.added}<`);
  });

  it("seçim yokken panel seçim çağrısı yapar, ayrıntı yerine yönlendirme gösterir", () => {
    const { changes, summary } = model();
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={summary} changes={changes} filter="all"
        onFilterChange={() => undefined} filteredChanges={changes}
        selectedChange={undefined} onSelect={() => undefined}
      />,
    );
    expect(markup).toContain("Rayda ya da listede bir fark seçin.");
  });

  it("filtre toplamı değiştirmez, panel görünen alt kümeyi bildirir", () => {
    const { changes, summary } = model();
    const added = filterChanges(changes, "added");
    const markup = renderToStaticMarkup(
      <ChangeInspector
        summary={summary} changes={changes} filter="added"
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
    expect(reportFileName(input, "html")).toBe("DegisikIs-Rapor-20260821-0930.html");
  });

  it("rapor kaydetme yerel komuta gider, ağ kullanılmaz", async () => {
    const invoker = vi.fn(async () => "/Users/x/Downloads/DegisikIs-Rapor.html");
    const saved = await saveReport("DegisikIs-Rapor.html", new TextEncoder().encode("<html></html>"), "html", invoker);
    expect(saved.path).toContain("Downloads");
    expect(invoker).toHaveBeenCalledTimes(1);
  });
});
