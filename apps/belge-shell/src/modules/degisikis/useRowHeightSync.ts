import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import type { FilledRef, NullableRef } from "./reactCompat";

type RowMap = FilledRef<Map<number, HTMLDivElement>>;

/**
 * Karşılıklı satırların gerçek yüksekliğini ölçüp her satır çiftine
 * max(sol, sağ) yüksekliğini uygular. Yalnız presentation katmanında çalışır:
 * karşılaştırma motorundan veri okumaz, ona DOM bilgisi taşımaz.
 *
 * Ölçüm hedefi `.row-body`; bu öğeye hiçbir zaman yükseklik yazılmaz, dolayısıyla
 * ölçüm her zaman doğal yüksekliği verir ve yazma → ölçme geri beslemesi oluşmaz.
 * Yazma hedefi `.row-content`.
 *
 * Yapısal gruplarda bir taraftaki placeholder satırı tek satıra indirgenip diğer
 * satırlar gizlenir (`grouped-placeholder-continuation`). Bu durumda görünür
 * placeholder, karşı taraftaki bütün grubun toplam yüksekliğini üstlenir; aksi
 * hâlde belge boyunca kayma birikir.
 */
/**
 * Satır yükseklikleri eşitlendikten sonra panelde yayınlanır. Fark rayı bu olayı
 * dinleyerek düğümlerini yeniden yerleştirir; böylece React yeniden render'ı
 * gerekmeden iki mekanizma senkron kalır.
 */
export const ROW_SYNC_EVENT = "degisikis:rowsync";

export function useRowHeightSync(
  baseRows: RowMap,
  revisedRows: RowMap,
  basePane: NullableRef<HTMLDivElement>,
  revisedPane: NullableRef<HTMLDivElement>,
  signature: string,
) {
  const frame = useRef(0);
  const observedWidths = useRef<[number, number]>([0, 0]);

  const sync = useCallback(() => {
    const base = baseRows.current;
    const revised = revisedRows.current;
    if (!base.size || !revised.size) return;

    const rowExtra = (row: HTMLElement) => {
      const style = getComputedStyle(row);
      return (
        parseFloat(style.paddingTop) + parseFloat(style.paddingBottom) +
        parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth)
      );
    };

    interface Cell { content: HTMLElement; body: number; }
    const read = (row: HTMLDivElement | undefined): Cell | undefined => {
      // offsetParent === null → satır display:none (gizlenmiş grup devamı).
      if (!row || row.offsetParent === null) return undefined;
      const content = row.querySelector<HTMLElement>(".row-content");
      const body = content?.querySelector<HTMLElement>(".row-body");
      if (!content || !body) return undefined;
      return { content, body: body.offsetHeight };
    };

    const firstRow = base.get(0) ?? revised.get(0);
    if (!firstRow) return;
    const extra = rowExtra(firstRow);

    // Okuma evresi — hiçbir yazma yapılmadan bütün ölçümler toplanır.
    const indices = Array.from(new Set([...base.keys(), ...revised.keys()])).sort((a, b) => a - b);
    interface Group { baseAnchor?: HTMLElement; revisedAnchor?: HTMLElement; baseTotal: number; revisedTotal: number; baseAnchorBody: number; revisedAnchorBody: number; }
    const groups: Group[] = [];
    let current: Group | undefined;

    for (const index of indices) {
      const left = read(base.get(index));
      const right = read(revised.get(index));
      if (left && right) {
        current = {
          baseAnchor: left.content,
          revisedAnchor: right.content,
          baseAnchorBody: left.body,
          revisedAnchorBody: right.body,
          baseTotal: left.body + extra,
          revisedTotal: right.body + extra,
        };
        groups.push(current);
        continue;
      }
      if (!current) continue;
      if (left) current.baseTotal += left.body + extra;
      if (right) current.revisedTotal += right.body + extra;
    }

    // Yazma evresi — ölçümler bittikten sonra tek seferde uygulanır.
    let changed = false;
    for (const group of groups) {
      if (!group.baseAnchor || !group.revisedAnchor) continue;
      const target = Math.max(group.baseTotal, group.revisedTotal);
      const baseRest = group.baseTotal - group.baseAnchorBody - extra;
      const revisedRest = group.revisedTotal - group.revisedAnchorBody - extra;
      const baseMin = Math.max(0, Math.round(target - extra - baseRest));
      const revisedMin = Math.max(0, Math.round(target - extra - revisedRest));
      const basePx = `${baseMin}px`;
      const revisedPx = `${revisedMin}px`;
      if (group.baseAnchor.style.minHeight !== basePx) { group.baseAnchor.style.minHeight = basePx; changed = true; }
      if (group.revisedAnchor.style.minHeight !== revisedPx) { group.revisedAnchor.style.minHeight = revisedPx; changed = true; }
    }
    if (changed) basePane.current?.dispatchEvent(new CustomEvent(ROW_SYNC_EVENT));
  }, [baseRows, revisedRows, basePane]);

  const schedule = useCallback(() => {
    if (frame.current) return;
    frame.current = window.requestAnimationFrame(() => {
      frame.current = 0;
      sync();
    });
  }, [sync]);

  // Karşılaştırma değiştiğinde: boyayı beklemeden hizala ve iki paneli de
  // aynı konumdan başlat.
  useLayoutEffect(() => {
    sync();
    if (basePane.current) basePane.current.scrollTop = 0;
    if (revisedPane.current) revisedPane.current.scrollTop = 0;
  }, [signature, sync, basePane, revisedPane]);

  // Yalnız genişlik değiştiğinde yeniden ölç: satır sarımı yalnız genişliğe bağlı.
  // Yükseklik değişimleri bu gözlemcinin kendi yazmalarından doğduğu için
  // yok sayılır; böylece sonsuz döngü oluşmaz.
  useEffect(() => {
    const panes = [basePane.current, revisedPane.current].filter((pane): pane is HTMLDivElement => !!pane);
    if (!panes.length || typeof ResizeObserver === "undefined") return;
    observedWidths.current = [panes[0]?.clientWidth ?? 0, panes[1]?.clientWidth ?? 0];
    const observer = new ResizeObserver(() => {
      const widths: [number, number] = [panes[0]?.clientWidth ?? 0, panes[1]?.clientWidth ?? 0];
      if (widths[0] === observedWidths.current[0] && widths[1] === observedWidths.current[1]) return;
      observedWidths.current = widths;
      schedule();
    });
    panes.forEach((pane) => observer.observe(pane));
    return () => {
      observer.disconnect();
      if (frame.current) window.cancelAnimationFrame(frame.current);
      frame.current = 0;
    };
  }, [signature, schedule, basePane, revisedPane]);
}
