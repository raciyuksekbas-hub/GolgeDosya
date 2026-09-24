import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import type { FilledRef, NullableRef } from "./reactCompat";
import type { ComparisonChange } from "./viewModels/comparisonViewModel";
import { Icon } from "./Icon";
import { ROW_SYNC_EVENT } from "./useRowHeightSync";

const KIND_LABEL: Record<ComparisonChange["kind"], string> = {
  added: "Eklenen",
  removed: "Silinen",
  modified: "Değiştirilen",
};

/**
 * İki belge arasındaki merkezî fark rayı — logodaki karşılaştırma hattının
 * arayüzdeki karşılığı ve gerçek gezinme yüzeyi. Her düğüm bir farkın sıra
 * numarasını taşır, ilgili satırın görünür alandaki dikey konumuna oturur ve
 * tıklandığında o farkı seçer. Görünür alanın dışındaki farklar ray uçlarına
 * yaslanıp numarasız birer işarete iner.
 *
 * Konumlandırma React state'i üzerinden değil doğrudan transform yazımıyla
 * yapılır; kaydırma sırasında yeniden render tetiklenmez ve iş rAF ile bir kare
 * başına tek sefere indirgenir.
 */
/**
 * Düğümün rayda durabileceği en alt/üst nokta.
 *
 * Düğüm 20 px yüksekliğinde ve merkezinden konumlanır. Görünür alanın dışındaki
 * farklar rayın ucuna iğnelenirken merkez TAM uca konuyordu: düğümün yarısı
 * (10 px) rayın altına taşıyor ve kabuğun kaydırıcısını büyütüyordu. Bütün
 * Karşılaştır görünümü, sağ panelin yanında ikinci bir dış kaydırma çubuğu
 * kazanıyordu (madde 20).
 */
export function clampRailY(y: number, trackHeight: number, half = 10): number {
  if (trackHeight <= half * 2) return trackHeight / 2;
  return Math.min(trackHeight - half, Math.max(half, y));
}

export function ChangeRail({ changes, selectedIndex, onSelect, onMove, onSwap, canSwap, paneRef, rowRefs, syncToken }: {
  changes: ComparisonChange[];
  selectedIndex: number;
  onSelect: (change: ComparisonChange) => void;
  onMove: (direction: -1 | 1) => void;
  onSwap: () => void;
  canSwap: boolean;
  paneRef: NullableRef<HTMLDivElement>;
  rowRefs: FilledRef<Map<number, HTMLDivElement>>;
  syncToken: string;
}) {
  const trackRef = useRef<HTMLDivElement>(null);
  const nodeRefs = useRef(new Map<string, HTMLButtonElement>());
  const frame = useRef(0);

  const place = useCallback(() => {
    const pane = paneRef.current;
    const track = trackRef.current;
    if (!pane || !track) return;
    const trackHeight = track.clientHeight;
    const viewport = Math.max(1, pane.clientHeight);
    const scrollTop = pane.scrollTop;
    // Ray gövdesi normalde belge kaydırma alanıyla aynı dikey aralığı kaplar.
    // Uyarı şeridi gibi durumlarda kayabileceği için fark ve ölçek her karede
    // iki dikdörtgen okumasıyla düzeltilir.
    const offset = pane.getBoundingClientRect().top - track.getBoundingClientRect().top;
    const scale = trackHeight / viewport;
    const placements: Array<[HTMLButtonElement, number, boolean]> = [];
    for (const change of changes) {
      const node = nodeRefs.current.get(change.id);
      if (!node) continue;
      const row = rowRefs.current.get(change.rowIndices[0] ?? 0);
      if (!row || row.offsetHeight === 0) {
        placements.push([node, trackHeight, true]);
        continue;
      }
      const center = row.offsetTop + row.offsetHeight / 2 - scrollTop;
      placements.push([node, offset + center * scale, center < 0 || center > viewport]);
    }
    for (const [node, y, offscreen] of placements) {
      node.classList.toggle("is-offscreen", offscreen);
      node.style.transform = `translate(-50%, -50%) translateY(${clampRailY(y, trackHeight).toFixed(1)}px)`;
    }
  }, [changes, paneRef, rowRefs]);

  const schedule = useCallback(() => {
    if (frame.current) return;
    frame.current = window.requestAnimationFrame(() => {
      frame.current = 0;
      place();
    });
  }, [place]);

  useLayoutEffect(() => {
    place();
    const settle = window.requestAnimationFrame(place);
    return () => window.cancelAnimationFrame(settle);
  }, [place, syncToken]);

  useEffect(() => {
    const pane = paneRef.current;
    if (!pane) return;
    pane.addEventListener("scroll", schedule, { passive: true });
    // Satır yükseklikleri eşitlendiğinde düğümler yeniden yerleşmeli.
    pane.addEventListener(ROW_SYNC_EVENT, schedule);
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(schedule) : undefined;
    observer?.observe(pane);
    // Satır yüksekliği eşitlemesi belge yüzeyini büyüttüğünde düğümler yeniden
    // yerleşmeli; panelin kendi ölçüsü değişmediği için içerik de gözlenir.
    if (pane.firstElementChild) observer?.observe(pane.firstElementChild);
    return () => {
      pane.removeEventListener("scroll", schedule);
      pane.removeEventListener(ROW_SYNC_EVENT, schedule);
      observer?.disconnect();
      if (frame.current) window.cancelAnimationFrame(frame.current);
      frame.current = 0;
    };
  }, [schedule, paneRef, syncToken]);

  const total = changes.length;
  return (
    <div className="change-rail">
      {/* Sürüm değiştirme iki belge arasındaki ilişkiye ait bir komuttur;
          bu yüzden merkez eksende, iki belge başlığının arasında durur. */}
      <div className="rail-head">
        <button
          className="rail-swap"
          onClick={onSwap}
          disabled={!canSwap}
          title="Sürümleri değiştir"
          aria-label="Sürümleri değiştir"
        >
          <Icon name="swap" size={15} />
        </button>
      </div>
      <div className="rail-body">
        {/* Sayaç yardımcı barda ("2 / 18 fark"); ray yalnız KONUM gösterir.
            Rayın dibindeki "2/4" aynı sayının üçüncü evi oluyordu. */}
        <button
          className="rail-step up"
          onClick={() => onMove(-1)}
          disabled={!total || selectedIndex === 0}
          aria-label="Önceki değişiklik"
          title="Önceki değişiklik"
        >
          <Icon name="chevron-up" size={14} />
        </button>
        <div className="rail-track" ref={trackRef}>
          <span className="rail-axis" aria-hidden="true" />
          {changes.map((change, index) => {
            const active = index === selectedIndex;
            return (
              <button
                key={change.id}
                ref={(node) => { if (node) nodeRefs.current.set(change.id, node); else nodeRefs.current.delete(change.id); }}
                className={`rail-node ${change.kind} ${active ? "is-active" : ""}`}
                onClick={() => onSelect(change)}
                title={`${change.displayIndex} · ${KIND_LABEL[change.kind]} · ${change.sectionLabel}`}
                aria-label={`${change.index}. değişiklik: ${KIND_LABEL[change.kind]} · ${change.sectionLabel}`}
                aria-current={active || undefined}
                data-row={change.rowIndices[0] ?? 0}
              >
                <span className="rail-node-index">{change.displayIndex}</span>
                <i aria-hidden="true" />
              </button>
            );
          })}
        </div>
        <button
          className="rail-step down"
          onClick={() => onMove(1)}
          disabled={!total || selectedIndex === total - 1}
          aria-label="Sonraki değişiklik"
          title="Sonraki değişiklik"
        >
          <Icon name="chevron-down" size={14} />
        </button>
      </div>
    </div>
  );
}
