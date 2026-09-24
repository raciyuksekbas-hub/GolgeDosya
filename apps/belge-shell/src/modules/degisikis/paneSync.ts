/**
 * Eşzamanlı kaydırma — bağımsız Değişikİş'in davranışı (App.tsx:219-242).
 *
 * Taşımada (b9a5ca4) bu işleyici yeniden yazılmıştı: `scrollTop` birebir
 * kopyalanıyor, yankı koruması bir sonraki `requestAnimationFrame`'de
 * kalkıyordu. Chromium'da (WebView2) bir kaydırma işleyicisinin içinde yapılan
 * `scrollTop` yazısı, hedefin kaydırma olayını BİR SONRAKİ kareye bırakır; rAF
 * ise aynı karede çalışır. Yankı korumasız geliyor ve hedefin bir kare eski
 * konumunu KULLANICININ kaydırdığı panele geri yazıyordu: yumuşak tekerlek ve
 * dokunmatik yüzeyde panel her iki karede bir geri sekiyor, kaydırma yarı
 * hızda ve titreyerek ilerliyordu (saha maddesi 22). Ölçüm: 311 satırda
 * kullanıcının paneline 62 geri yazma, 61'i geriye doğru; eski işleyicide 0.
 *
 * Geri gelen iki şey:
 *  * Yan başına ZAMAN DAMGALI koruma: hedefe yazılan konumun yankısı 120 ms
 *    boyunca yok sayılır.
 *  * SATIRA BAĞLI eşleme: kaynağın üst bandındaki satır ve satır içi ilerleme,
 *    hedefte aynı satırın aynı noktasına getirilir. `scrollTop` kopyalamak iki
 *    panelin yükseklik farkını (yuvarlama kayması, yalnız bir paneldeki yatay
 *    kaydırma çubuğu) belgenin sonuna doğru hizasızlığa çeviriyordu.
 */
export type Side = "base" | "revised";

export interface RowLike {
  offsetTop: number;
  offsetHeight: number;
}

export interface PaneLike {
  scrollTop: number;
  clientHeight: number;
}

/** Yankının yok sayıldığı süre (eski uygulamanın değeri). */
export const ECHO_MS = 120;

/** Eşlemenin çapası: panelin üst bandı, en çok 80 px ya da yüksekliğin beşte biri. */
const anchorOffset = (pane: PaneLike) => Math.min(80, pane.clientHeight * 0.2);

/**
 * Kaynağın konumunu hedefte karşılık gelen konuma çevirir; satır yoksa `null`.
 *
 * Satırlar sıra numarasıyla eşleşir (iki panelde aynı satır dizisi çizilir).
 * Çapa satırı ikili aramayla bulunur: uzun belgede her olayda doğrusal tarama
 * yapılmaz.
 */
export function mappedScrollTop(
  from: PaneLike,
  to: PaneLike,
  sourceRows: ReadonlyMap<number, RowLike>,
  targetRows: ReadonlyMap<number, RowLike>,
): number | null {
  const ordered = Array.from(sourceRows.entries()).sort(([a], [b]) => a - b);
  if (ordered.length === 0) return null;
  const anchor = from.scrollTop + anchorOffset(from);
  let lo = 0;
  let hi = ordered.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (ordered[mid][1].offsetTop <= anchor) lo = mid;
    else hi = mid - 1;
  }
  const [rowIndex, sourceRow] = ordered[lo];
  const targetRow = targetRows.get(rowIndex);
  if (!targetRow) return null;
  const progress = Math.max(0, Math.min(1, (anchor - sourceRow.offsetTop) / Math.max(1, sourceRow.offsetHeight)));
  return Math.max(0, targetRow.offsetTop + progress * targetRow.offsetHeight - anchorOffset(to));
}

/** Bir karşılaştırma görünümünün kaydırma eşleyicisi. */
export function createPaneSync(now: () => number = Date.now) {
  const until: Record<Side, number> = { base: 0, revised: 0 };
  return {
    /**
     * Kaynak kaydırıldı: hedefi izlet. Kaynağın kendi olayı az önce bizim
     * yazdığımız konumun yankısıysa hiçbir şey yapılmaz.
     */
    follow(
      source: Side,
      panes: Record<Side, PaneLike | null>,
      rows: Record<Side, ReadonlyMap<number, RowLike>>,
    ): void {
      if (now() < until[source]) return;
      const target: Side = source === "base" ? "revised" : "base";
      const from = panes[source];
      const to = panes[target];
      if (!from || !to) return;
      const top = mappedScrollTop(from, to, rows[source], rows[target]);
      if (top === null) return;
      until[target] = now() + ECHO_MS;
      to.scrollTop = top;
    },
    /** Programın kendisi iki paneli de kaydırıyor (seçili farka gitmek): yankıları yok say. */
    quiet(ms = ECHO_MS): void {
      const t = now() + ms;
      until.base = t;
      until.revised = t;
    },
  };
}
