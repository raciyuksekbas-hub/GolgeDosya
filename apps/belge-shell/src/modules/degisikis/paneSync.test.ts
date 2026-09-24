// Eşzamanlı kaydırma (saha maddesi 22): kullanıcının kaydırdığı panele geri
// yazılmaz, karşı panel aynı satırı gösterir.
//
// Kare döngüsü Chromium'un sırasını izler: bekleyen kaydırma olayları karenin
// başında, konumların DEĞİŞTİĞİ SIRAYLA dağıtılır; dağıtım sırasında yapılan
// `scrollTop` yazıları bir SONRAKİ kareye olay bırakır; ardından rAF geri
// çağrıları çalışır; en son kullanıcının o karedeki kaydırması uygulanır. Bu
// yüzden bir önceki karede karşı panele yazılan konumun yankısı, kullanıcının
// yeni kaydırmasından ÖNCE gelir.
import { describe, expect, it } from "vitest";
import { createPaneSync, mappedScrollTop, type RowLike, type Side } from "./paneSync";

const FRAME = 16;

function rows(heights: number[]): Map<number, RowLike> {
  const map = new Map<number, RowLike>();
  let top = 0;
  heights.forEach((h, i) => {
    map.set(i, { offsetTop: top, offsetHeight: h });
    top += h;
  });
  return map;
}

/** Satır yükseklikleri eşitlenmiş iki panel (useRowHeightSync), 311 satır. */
const heights = Array.from({ length: 311 }, (_, i) => (i % 7 === 0 ? 66 : i % 3 === 0 ? 44 : 22));
const ROWS: Record<Side, Map<number, RowLike>> = { base: rows(heights), revised: rows(heights) };

interface Probe {
  writesBack: number;
  backwards: number;
  frames: number;
  gap: number;
}

type Handler = (source: Side, panes: Record<Side, Pane>, frame: { raf: (cb: () => void) => void; now: () => number }) => void;

class Pane {
  top = 0;
  readonly clientHeight = 600;
  constructor(
    private readonly side: Side,
    private readonly pending: Side[],
    private readonly log: (value: number, previous: number) => void,
  ) {}
  private queue() {
    if (!this.pending.includes(this.side)) this.pending.push(this.side);
  }
  get scrollTop() {
    return this.top;
  }
  set scrollTop(value: number) {
    if (value === this.top) return;
    const previous = this.top;
    this.top = value;
    this.queue();
    this.log(value, previous);
  }
  userScroll(value: number) {
    if (value === this.top) return;
    this.top = value;
    this.queue();
  }
}

/**
 * Kullanıcı Temel paneli yumuşak (ease-out) tekerlek/dokunmatik yüzey
 * hareketiyle 3000 px aşağı kaydırır. Girdi bir FARKTIR: her kare, panelin o
 * anki konumuna eklenir (tekerlek böyle çalışır). Hedefe varılana dek sürer.
 */
function run(handler: Handler): Probe {
  let writesBack = 0;
  let backwards = 0;
  let pending: Side[] = [];
  const queue: Side[] = [];
  const panes = {
    base: new Pane("base", queue, (value, previous) => {
      writesBack++;
      if (value < previous) backwards++;
    }),
    revised: new Pane("revised", queue, () => {}),
  };
  let raf: Array<() => void> = [];
  let frame = 0;
  const now = () => frame * FRAME;
  const target = 3000;
  let arrived = -1;
  for (frame = 0; frame < 400; frame++) {
    // 1) bekleyen olaylar, oluşma sırasıyla; dağıtım sırasındaki yazılar
    //    sonraki kareye kalır.
    pending = queue.splice(0);
    for (const side of pending) handler(side, panes, { raf: (cb) => raf.push(cb), now });
    // 2) rAF
    const due = raf;
    raf = [];
    due.forEach((cb) => cb());
    // 3) kullanıcı girdisi: ilk 60 karede ease-out farkları, sonra kalan
    //    mesafe için son hızda devam.
    if (panes.base.top < target) {
      const eased = (k: number) => target * (1 - (1 - Math.min(k, 60) / 60) ** 3);
      const delta = frame < 60 ? eased(frame + 1) - eased(frame) : 12;
      panes.base.userScroll(Math.min(target, Math.round(panes.base.top + Math.max(delta, 12))));
    }
    if (arrived < 0 && panes.base.top >= target) arrived = frame + 1;
  }
  const anchorRow = (pane: Pane) => {
    const anchor = pane.top + Math.min(80, pane.clientHeight * 0.2);
    let hit = 0;
    for (const [i, r] of ROWS.base) if (r.offsetTop <= anchor) hit = i;
    return hit;
  };
  return { writesBack, backwards, frames: arrived, gap: Math.abs(anchorRow(panes.base) - anchorRow(panes.revised)) };
}

describe("eşzamanlı kaydırma kullanıcıyla kavga etmez", () => {
  it("taşımadaki işleyici (rAF ile kalkan koruma) kullanıcının paneline geri yazıyordu", () => {
    // Testin ayırt edici olduğunun kanıtı: HEAD'deki mantığın birebir kopyası.
    let syncing = false;
    const legacy: Handler = (source, panes, frame) => {
      if (syncing) return;
      const dst = source === "base" ? panes.revised : panes.base;
      syncing = true;
      dst.scrollTop = panes[source].scrollTop;
      frame.raf(() => {
        syncing = false;
      });
    };
    const probe = run(legacy);
    expect(probe.writesBack).toBeGreaterThan(20);
    expect(probe.backwards).toBeGreaterThan(20);
    // Geri yazılan her konum kullanıcının yolunu uzatır: varış gecikir.
    expect(probe.frames).toBeGreaterThan(70);
  });

  it("zaman damgalı koruma: sıfır geri yazma, gecikme yok, aynı satır", () => {
    let sync: ReturnType<typeof createPaneSync> | null = null;
    const restored: Handler = (source, panes, frame) => {
      sync ??= createPaneSync(frame.now);
      sync.follow(source, panes, ROWS);
    };
    const probe = run(restored);
    expect(probe.writesBack).toBe(0);
    expect(probe.backwards).toBe(0);
    expect(probe.frames).toBeLessThanOrEqual(61);
    expect(probe.gap).toBe(0);
  });
});

describe("satıra bağlı eşleme", () => {
  it("panel yükseklikleri farklıysa da aynı satırın aynı noktası", () => {
    // Karşı panelde her satır 1.5 kat uzun (ör. yalnız bir tarafta sarma).
    const target = rows(heights.map((h) => h * 1.5));
    const from = { scrollTop: 5000, clientHeight: 600 };
    const to = { scrollTop: 0, clientHeight: 600 };
    const top = mappedScrollTop(from, to, ROWS.base, target)!;
    const anchorBase = 5000 + 80;
    const anchorTarget = top + 80;
    const rowAt = (m: Map<number, RowLike>, y: number) => {
      let hit = 0;
      for (const [i, r] of m) if (r.offsetTop <= y) hit = i;
      return hit;
    };
    expect(rowAt(target, anchorTarget)).toBe(rowAt(ROWS.base, anchorBase));
    // scrollTop kopyalamak burada 4 satırdan fazla kayardı.
    expect(Math.abs(rowAt(target, 5000 + 80) - rowAt(ROWS.base, anchorBase))).toBeGreaterThan(4);
  });

  it("satır yoksa yazmaz", () => {
    expect(mappedScrollTop({ scrollTop: 0, clientHeight: 1 }, { scrollTop: 0, clientHeight: 1 }, new Map(), new Map())).toBeNull();
  });

  it("programın kendi kaydırmasının yankısı izletilmez", () => {
    let t = 0;
    const sync = createPaneSync(() => t);
    const panes = { base: { scrollTop: 900, clientHeight: 600 }, revised: { scrollTop: 900, clientHeight: 600 } };
    sync.quiet();
    sync.follow("revised", panes, ROWS);
    expect(panes.base.scrollTop).toBe(900);
    t = 200;
    panes.revised.scrollTop = 2000;
    sync.follow("revised", panes, ROWS);
    expect(panes.base.scrollTop).not.toBe(900);
  });
});
