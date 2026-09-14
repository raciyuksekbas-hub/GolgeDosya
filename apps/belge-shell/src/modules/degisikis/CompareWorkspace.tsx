import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { compareDocuments } from "./core/compare";
import { extractDocument, ExtractionError } from "./core/extractors";
import type { LocalDocument } from "./core/types";
import { buildComparisonViewModel, filterChanges, type ChangeFilter } from "./viewModels/comparisonViewModel";
import { DocumentPane } from "./DocumentPane";
import { ChangeRail } from "./ChangeRail";
import { ChangeInspector } from "./ChangeInspector";
import { useRowHeightSync } from "./useRowHeightSync";
import { announce } from "../../shared-ui/Announcer";
import { Status } from "../../shared-ui/primitives";
import { InspectorPanel, ToolbarStatus } from "../../shell/chrome";
import "./compare.css";

function baseName(path: string): string {
  return path.split("/").pop() || path;
}

/**
 * Yoldan `File` nesnesi üret.
 *
 * Bağımsız Değişikİş belgeyi webview'in dosya girdisinden `File` olarak
 * alıyordu; birleşik kabuk native seçiciyi kullandığı için elde yol var.
 * Motorun imzasını değiştirmemek adına baytlar Rust'tan okunup burada yeniden
 * `File`'a sarılıyor. `extractDocument` bu yüzden birebir aynı kalabiliyor.
 */
async function fileFromPath(path: string): Promise<File> {
  const bytes = await invoke<number[]>("degisikis_read_document", { path });
  return new File([new Uint8Array(bytes)], baseName(path));
}

type Loaded = { path: string; doc: LocalDocument };

/**
 * Karşılaştır — iki belge arasındaki değişiklikler.
 *
 * Motor değiştirilmedi: `extractDocument` (mammoth / pdfjs / fflate) ve
 * `compareDocuments` bağımsız uygulamadaki hâlleriyle çalışıyor. Satır
 * hizalama, fark rayı ve diff işaretleme de aynı.
 *
 * Değişen: kendi araç çubuğu kalktı. Belge adları yardımcı barın bağlamında
 * (kabuk çiziyor), özet ve filtre kabuğun sağ panelinde. Her kontrolün tek bir
 * evi var: sürüm değiştirme rayın başında, filtre panelde, paneli gizleme
 * bardaki "Ayrıntılar" düğmesinde.
 */
export function CompareWorkspace({ paths }: { paths: string[] }) {
  const [docs, setDocs] = useState<[Loaded, Loaded] | null>(null);
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState<string | null>(null);
  const [filter, setFilter] = useState<ChangeFilter>("all");
  const [selected, setSelected] = useState<string | undefined>(undefined);

  const basePane = useRef<HTMLDivElement | null>(null);
  const revisedPane = useRef<HTMLDivElement | null>(null);
  const baseRows = useRef<Map<number, HTMLDivElement>>(new Map());
  const revisedRows = useRef<Map<number, HTMLDivElement>>(new Map());

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setFailure(null);
    setDocs(null);
    (async () => {
      try {
        const [a, b] = paths.slice(0, 2);
        const [first, second] = await Promise.all([
          extractDocument(await fileFromPath(a)),
          extractDocument(await fileFromPath(b)),
        ]);
        if (cancelled) return;
        setDocs([{ path: a, doc: first }, { path: b, doc: second }]);
        announce("Belgeler karşılaştırıldı.");
      } catch (e) {
        if (cancelled) return;
        setFailure(
          e instanceof ExtractionError
            ? e.message
            : "Belgeler okunamadı. Dosyaların bütünlüğünü kontrol edin.",
        );
      } finally {
        if (!cancelled) setBusy(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [paths]);

  // Karşılaştırma sonucu ve ondan türetilen görünüm modeli birlikte tutulur:
  // satırlar sonuçta, değişiklik listesi ve özet görünüm modelindedir.
  const comparison = useMemo(
    () => (docs ? compareDocuments(docs[0].doc.blocks, docs[1].doc.blocks) : null),
    [docs],
  );
  const model = useMemo(
    () => (comparison ? buildComparisonViewModel(comparison) : null),
    [comparison],
  );

  // İlk fark açılışta seçilir. Panel boş bir yer tutucuyla ("bir fark seçin")
  // açılmaz ve kullanıcı ilk farkı görmek için tıklamak zorunda kalmaz;
  // Denetle de ilk bulguyu aynı şekilde açar.
  useEffect(() => {
    setSelected(model?.changes[0]?.id);
  }, [model]);

  const visible = useMemo(
    () => (model ? filterChanges(model.changes, filter) : []),
    [model, filter],
  );

  const signature = useMemo(
    () => (docs ? `${docs[0].path}|${docs[1].path}|${model?.rowCount ?? 0}` : ""),
    [docs, model],
  );

  useRowHeightSync(baseRows, revisedRows, basePane, revisedPane, signature);

  // Eşzamanlı kaydırma: standalone davranışın aynısı.
  const syncing = useRef(false);
  const scrollFrom = useCallback((from: "base" | "revised") => {
    if (syncing.current) return;
    const src = from === "base" ? basePane.current : revisedPane.current;
    const dst = from === "base" ? revisedPane.current : basePane.current;
    if (!src || !dst) return;
    syncing.current = true;
    dst.scrollTop = src.scrollTop;
    requestAnimationFrame(() => {
      syncing.current = false;
    });
  }, []);

  if (busy) {
    return (
      <div className="surface">
        <Status tone="busy">Belgeler karşılaştırılıyor…</Status>
      </div>
    );
  }

  if (failure) {
    return (
      <div className="surface">
        <Status tone="error">{failure}</Status>
      </div>
    );
  }

  if (!docs || !model || !comparison) return null;

  const selectedChange = model.changes.find((c) => c.id === selected);
  const selectedIndex = selectedChange ? visible.indexOf(selectedChange) : -1;
  const move = (direction: -1 | 1) => {
    if (visible.length === 0) return;
    const next = Math.min(Math.max(selectedIndex + direction, 0), visible.length - 1);
    setSelected(visible[next]?.id);
  };

  return (
    <div className="compare-root">
      {/* Fark sayacı barda, panelde değil: aynı sayı iki evde durmaz.
          Ray KONUMU, liste İÇERİĞİ gösterir; sayı ikisinin de üstünde. */}
      <ToolbarStatus>
        {model.summary.total === 0
          ? "Fark yok"
          : `${Math.max(selectedIndex, 0) + 1} / ${visible.length} fark`}
      </ToolbarStatus>

      <InspectorPanel title="Farklar" scope="compare-root">
        <ChangeInspector
          summary={model.summary}
          filter={filter}
          onFilterChange={setFilter}
          filteredChanges={visible}
          selectedChange={selectedChange}
          onSelect={(c) => setSelected(c.id)}
        />
      </InspectorPanel>

      {/* Ekran okuyucu için özet: panel kapalıyken de duyurulur. */}
      <p className="sr-only" role="status">
        {model.summary.total === 0
          ? "Fark bulunamadı."
          : `${model.summary.added} ekleme · ${model.summary.removed} silme · ${model.summary.modified} değişiklik`}
      </p>

      <div className="compare-panes">
        <DocumentPane
          side="base"
          doc={docs[0].doc}
          rows={comparison.rows}
          changes={visible}
          selectedRows={selectedChange?.rowIndices ?? []}
          paneRef={basePane}
          rowRefs={baseRows}
          onScroll={() => scrollFrom("base")}
        />
        <ChangeRail
          changes={visible}
          selectedIndex={selectedIndex}
          onSelect={(c) => setSelected(c.id)}
          onMove={move}
          onSwap={() => setDocs((d) => (d ? [d[1], d[0]] : d))}
          canSwap
          paneRef={basePane}
          rowRefs={baseRows}
          syncToken={signature}
        />
        <DocumentPane
          side="revised"
          doc={docs[1].doc}
          rows={comparison.rows}
          changes={visible}
          selectedRows={selectedChange?.rowIndices ?? []}
          paneRef={revisedPane}
          rowRefs={revisedRows}
          onScroll={() => scrollFrom("revised")}
        />
      </div>
    </div>
  );
}
