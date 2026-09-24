import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { compareDocuments } from "./core/compare";
import { extractDocument, ExtractionError } from "./core/extractors";
import type { LocalDocument } from "./core/types";
import { buildComparisonViewModel, filterChanges, type ChangeFilter } from "./viewModels/comparisonViewModel";
import { DocumentPane, PaneHeader } from "./DocumentPane";
import { ChangeRail } from "./ChangeRail";
import { ChangeInspector } from "./ChangeInspector";
import { useRowHeightSync } from "./useRowHeightSync";
import { announce } from "../../shared-ui/Announcer";
import { logFailure } from "../../shared-ui/failure";
import { Status } from "../../shared-ui/primitives";
import { InspectorPanel, ToolbarStatus } from "../../shell/chrome";
import { fileNameOf as baseName } from "../../shell/modes";
import "./compare.css";

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
/**
 * Yardımcı bardaki fark sayacı.
 *
 * Süzgeç hiçbir şeyi tutmadığında `selectedIndex` -1 olur ve eski ifade
 * (`Math.max(-1, 0) + 1`) sayacı **"1 / 0 fark"** yazıyordu: hem aritmetik
 * olarak imkânsız, hem de belgede fark OLDUĞU hâlde olmadığını ima ediyor.
 * Süzgeç sonucu süzgeç diliyle söylenir; toplam kaybolmaz.
 */
export function diffCounterLabel(total: number, visible: number, selectedIndex: number): string {
  if (total === 0) return "Fark yok";
  if (visible === 0) return `Bu süzgeçte fark yok · toplam ${total}`;
  return `${Math.max(selectedIndex, 0) + 1} / ${visible} fark`;
}

export function CompareWorkspace({ paths, onPairChange }: {
  paths: string[];
  /**
   * Yürürlükteki Temel/Değişik sırasını kabuğa bildirir.
   *
   * Sürüm değiştirme bu çalışma alanının KENDİ durumunu çevirir; kabuğun
   * belge listesi değişmez. Bunu söylemezsek yardımcı bardaki çipler açılış
   * sırasında kalır ve "önce yazan Temel'dir" okumasıyla çelişir. Sıra
   * yeniden çıkarma yapılmadan bildirilir; kullanıcı seçili değişikliği
   * kaybetmez.
   */
  onPairChange?: (paths: string[]) => void;
}) {
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
        // Hata HANGİ belgede olduğunu söyler. Eskiden iki çıkarma tek
        // `Promise.all` içindeydi ve red olduğu gibi yukarı çıkıyordu:
        // kullanıcı "okunamadı" cümlesini görüyor ama iki dosyadan hangisini
        // değiştireceğini bilmiyordu.
        const read = async (path: string, role: string) => {
          try {
            return await extractDocument(await fileFromPath(path));
          } catch (e) {
            throw new ExtractionError(
              `${baseName(path)} (${role}) okunamadı. ${
                e instanceof ExtractionError ? e.message : "Dosyanın bütünlüğünü kontrol edin."
              }`,
            );
          }
        };
        const [first, second] = await Promise.all([
          read(a, "Temel Sürüm"),
          read(b, "Değişik Sürüm"),
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

  /**
   * Çıkarma uyarıları — okunAMAYAN içerik.
   *
   * `extractDocument` mammoth'un "desteklenmeyen öğe" uyarılarını ve eski
   * `.doc` yerel dönüşüm notunu topluyordu ama HİÇBİR YERDE gösterilmiyordu.
   * Bu uyarılar "bu içerik okunamadı" demektir: okunamayan içerik
   * KARŞILAŞTIRILMAMIŞ demektir de. Kullanıcı tertemiz bir fark listesi görüp
   * "burada fark yok" sonucuna varıyordu — oysa o kısım hiç okunmamıştı.
   * Hukuki belgede bu sessizlik kabul edilemez.
   */
  const extractionWarnings = useMemo(() => {
    if (!docs) return [];
    return Array.from(new Set(docs.flatMap((d) => d.doc.warnings)));
  }, [docs]);

  /**
   * Yalnız BİÇİM sadeleştirildi — metin karşılaştırmaya girdi.
   *
   * Bu ayrım sahada ortaya çıktı: tamamı okunmuş iki sözleşmede kullanıcı
   * "Belgenin bir bölümü okunamadı ve karşılaştırmaya girmedi" uyarısını
   * gördü, ardına da ham stil adları (`Gövde`, `Normal (Web)`,
   * `List Paragraph`…) döküldü. Hiçbiri kayıp değildi; hepsi "bu Word
   * stilini tanımadım, paragraf olarak aldım" demekti.
   */
  const simplifications = useMemo(() => {
    if (!docs) return [];
    return Array.from(new Set(docs.flatMap((d) => d.doc.notes)));
  }, [docs]);

  // Ham ayrıştırıcı metni ekrandan kalktı ama KAYBOLMADI: teşhis geliştirme
  // günlüğünde durur, yoksa bir sonraki saha hatası kör incelenir.
  useEffect(() => {
    if (extractionWarnings.length || simplifications.length)
      logFailure("degisikis extraction messages", { extractionWarnings, simplifications });
  }, [extractionWarnings, simplifications]);

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
  /**
   * Bir tarafı YERİNDE değiştir (§33–34).
   *
   * Karşılaştırma açıldıktan sonra iki kaynak kilitleniyordu: başka bir
   * belge denemek için çalışma alanını kapatıp baştan başlamak gerekiyordu.
   * Oysa `PaneHeader` ve `.pane-header` stilleri bu akış için zaten vardı —
   * bileşen hiç çizilmiyordu.
   *
   * Yalnız istenen taraf değişir; diğer taraf yeniden çıkarılmaz. Eski diff
   * durumu (seçili fark, süzgeç konumu) temizlenir ki ekranda eskimiş bir
   * seçim kalmasın.
   */
  const replaceSide = useCallback(
    async (side: 0 | 1, file: File) => {
      setBusy(true);
      setFailure(null);
      try {
        const extracted = await extractDocument(file);
        setDocs((current) => {
          if (!current) return current;
          const next: [Loaded, Loaded] = [...current] as [Loaded, Loaded];
          next[side] = { path: file.name, doc: extracted };
          onPairChange?.(next.map((d) => d.path));
          return next;
        });
        // Eski seçim yeni belgede anlamsız; sayaç ve süzgeç yeniden kurulur.
        setSelected(undefined);
        announce(
          `${side === 0 ? "Temel" : "Değişik"} sürüm ${file.name} ile değiştirildi. Karşılaştırma yenilendi.`,
        );
      } catch (e) {
        logFailure("degisikis replace side", e);
        const message =
          e instanceof ExtractionError
            ? e.message
            : "Belge okunamadı. Dosyanın bütünlüğünü kontrol edin.";
        setFailure(message);
        announce(message);
      } finally {
        setBusy(false);
      }
    },
    [onPairChange],
  );

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

  // Seçili fark GÖRÜNÜR ALANA getirilir.
  //
  // "Sonraki/Önceki değişiklik" düğmeleri ve ray düğümleri yalnız SEÇİMİ
  // değiştiriyordu: sayaç ilerliyor, ray hareket ediyor, ama BELGE yerinde
  // kalıyordu. Ekrandan uzun bir belgede kullanıcı ileri tıklayıp hiçbir şeyin
  // değişmediğini görüyordu — etiketi "değişikliğe git" diyen bir kontrol
  // gitmiyordu. Ray belgeyi İZLİYOR (scrollTop okuyor); ters yön eksikti.
  //
  // Zaten görünen bir fark için kaydırma yapılmaz: kullanıcı kendi kaydırmasını
  // yaptıysa onunla kavga edilmez. İki panel birbirine senkron olduğu için
  // yalnız biri sürülür.
  useEffect(() => {
    const change = model?.changes.find((c) => c.id === selected);
    const pane = basePane.current;
    if (!change || !pane) return;
    const row = baseRows.current.get(change.rowIndices[0] ?? 0);
    if (!row || row.offsetHeight === 0) return;
    const view = pane.clientHeight;
    const top = row.offsetTop;
    const bottom = top + row.offsetHeight;
    if (top >= pane.scrollTop && bottom <= pane.scrollTop + view) return;
    const next = Math.max(0, top - view / 2 + row.offsetHeight / 2);
    pane.scrollTop = next;
    if (revisedPane.current) revisedPane.current.scrollTop = next;
  }, [selected, model]);

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
      {/* Ham ayrıştırıcı mesajı KULLANICIYA ÇIKMAZ (stil adları, kimlikler).
          Kullanıcının bilmesi gereken tek şey: metin karşılaştırmaya girdi mi,
          girmediyse ne yapmalı. Teknik ayrıntı geliştirme günlüğünde kalır. */}
      {extractionWarnings.length > 0 ? (
        <p className="compare-warn" role="status">
          <span className="compare-warn-mark" aria-hidden="true">!</span>
          <span>
            Belgenin bir bölümü okunamadı ve karşılaştırmaya girmedi. Eksik
            kalan yerler için belgeyi kaynak uygulamada yeniden kaydedip
            deneyin.
          </span>
        </p>
      ) : simplifications.length > 0 ? (
        <p className="compare-warn" data-tone="note" role="status">
          <span className="compare-warn-mark" aria-hidden="true">i</span>
          <span>
            Belgedeki bazı özel biçimlendirmeler sadeleştirildi; metin
            karşılaştırmaya dahil edildi.
          </span>
        </p>
      ) : null}
      {/* Fark sayacı barda, panelde değil: aynı sayı iki evde durmaz.
          Ray KONUMU, liste İÇERİĞİ gösterir; sayı ikisinin de üstünde. */}
      <ToolbarStatus>{diffCounterLabel(model.summary.total, visible.length, selectedIndex)}</ToolbarStatus>

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
        <div className="pane">
        <PaneHeader doc={docs[0].doc} side="base" onReplace={(f) => void replaceSide(0, f)} />
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
        </div>
        <ChangeRail
          changes={visible}
          selectedIndex={selectedIndex}
          onSelect={(c) => setSelected(c.id)}
          onMove={move}
          onSwap={() => {
            if (!docs) return;
            // Yan etki durum güncelleyicinin İÇİNDE durmaz: React güncelleyiciyi
            // iki kez çağırabilir, duyuru da çift okunurdu.
            const swapped: [Loaded, Loaded] = [docs[1], docs[0]];
            setDocs(swapped);
            onPairChange?.(swapped.map((x) => x.path));
            announce(`Sürümler değiştirildi. Temel sürüm artık ${swapped[0].doc.name}.`);
          }}
          canSwap
          paneRef={basePane}
          rowRefs={baseRows}
          syncToken={signature}
        />
        <div className="pane">
        <PaneHeader doc={docs[1].doc} side="revised" onReplace={(f) => void replaceSide(1, f)} />
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
    </div>
  );
}
