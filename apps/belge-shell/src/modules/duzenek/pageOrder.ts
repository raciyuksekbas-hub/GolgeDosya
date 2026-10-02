/**
 * Sırala — sayfa sırasının saf modeli.
 *
 * Sıralama eskiden yalnız komşu iki sayfanın yer değiştirmesiydi (↑/↓). 40
 * sayfalık bir dilekçede bir sayfayı başa almak 39 tıklama istiyordu; birden
 * fazla sayfa taşınamıyor, yanlış taşıma geri alınamıyordu. Buradaki işlevler
 * DOM bilmez: sürükleme, düğmeler ve klavye aynı işlemleri çağırır, testler de
 * davranışı tarayıcısız doğrular.
 *
 * Her işlem bir PERMÜTASYON üretir: hiçbir sayfa çoğalmaz ya da kaybolmaz.
 * Kaydetme yolu (Rust `ToolOperation::Reorder`) her sayfayı tam bir kez ister.
 */

type Keyed = { key: string };

/**
 * `keys` ile verilen sayfaları kendi aralarındaki sırayı koruyarak `insertion`
 * boşluğuna taşır. `insertion` taşımadan ÖNCEKİ listede bir boşluktur
 * (0 = ilk sayfanın üstü, `order.length` = son sayfanın altı); sürüklenen
 * çizginin gösterdiği yer tam olarak budur.
 */
export function moveKeys<T extends Keyed>(order: T[], keys: readonly string[], insertion: number): T[] {
    const moving = new Set(keys);
    const picked = order.filter(p => moving.has(p.key));
    if (picked.length === 0) return order;
    const gap = Math.max(0, Math.min(order.length, Math.round(insertion)));
    const rest = order.filter(p => !moving.has(p.key));
    // Boşluk, taşınan sayfalar çıkarıldıktan sonraki listeye çevrilir.
    const before = order.slice(0, gap).filter(p => moving.has(p.key)).length;
    const at = gap - before;
    return [...rest.slice(0, at), ...picked, ...rest.slice(at)];
}

/**
 * Seçili her sayfa bir sıra yukarı (-1) ya da aşağı (+1) gider; aradaki
 * boşluklar korunur. Kenara dayanmış sayfa yerinde kalır ve arkasındaki seçili
 * sayfa onun üstünden atlamaz. Böylece "Yukarı" ve "Aşağı" birbirini geri alır.
 */
export function shiftKeys<T extends Keyed>(order: T[], keys: readonly string[], delta: -1 | 1): T[] {
    const moving = new Set(keys);
    const next = [...order];
    const step = (i: number, j: number) => {
        if (moving.has(next[i].key) && !moving.has(next[j].key)) [next[i], next[j]] = [next[j], next[i]];
    };
    if (delta < 0) for (let i = 1; i < next.length; i++) step(i, i - 1);
    else for (let i = next.length - 2; i >= 0; i--) step(i, i + 1);
    return next;
}

export function sameOrder(a: readonly Keyed[], b: readonly Keyed[]): boolean {
    return a.length === b.length && a.every((p, i) => p.key === b[i].key);
}

/** Sıra kaynaktakinden farklı mı? Taşıyıp geri getirmek değişiklik değildir. */
export function isReordered(order: readonly Keyed[], source: readonly Keyed[]): boolean {
    return !sameOrder(order, source);
}

export type SelectGesture = {
    /** Shift: çapadan tıklanan sayfaya kadar aralık. */
    range?: boolean;
    /** macOS'ta ⌘, Windows'ta Ctrl (bkz. shell/platform.ts `isToggleChord`). */
    toggle?: boolean;
    /**
     * Klavyeden gelen etkinleştirme (görüntüle düğmesinde Enter/Boşluk;
     * tıklama olayında `detail === 0`). Klavye kullanıcısı sayfaları Seç
     * kutularıyla seçer; düğme ona yalnız sayfayı GÖSTERİR. Eskiden düz bir
     * tıklama sayılıyor, 2–4–6 seçimi duyurusuz tek sayfaya iniyor ve sonraki
     * "Başa" üç yerine bir sayfa taşıyordu.
     */
    keyboard?: boolean;
};

/**
 * Masaüstü listelerinin seçim kuralı: düz tıklama tek sayfa seçer, Shift
 * çapadan aralık seçer, ⌘/Ctrl tek sayfayı ekler ya da çıkarır. Sonuç her
 * zaman belgedeki sırayla döner.
 */
export function selectPages(
    order: readonly Keyed[], selected: readonly string[], anchor: string, key: string, gesture: SelectGesture = {},
): { selected: string[]; anchor: string } {
    if (gesture.keyboard) return { selected: [...selected], anchor };
    const inOrder = (set: Set<string>) => order.filter(p => set.has(p.key)).map(p => p.key);
    const from = order.findIndex(p => p.key === anchor), to = order.findIndex(p => p.key === key);
    if (gesture.range && from >= 0 && to >= 0) {
        const [lo, hi] = from <= to ? [from, to] : [to, from];
        return { selected: order.slice(lo, hi + 1).map(p => p.key), anchor };
    }
    if (gesture.toggle) {
        const set = new Set(selected);
        if (set.has(key)) set.delete(key); else set.add(key);
        return { selected: inOrder(set), anchor: key };
    }
    return { selected: [key], anchor: key };
}

/**
 * Sırala şeridinde klavyeyle sayfadan sayfaya: hedef sayfanın sırası, şeridin
 * işi olmayan tuşta `null`. Şerit tek sekme durağıdır (roving tabindex):
 * yalnız bir sayfanın denetimleri sekme sırasındadır, ↑/↓ komşu sayfaya,
 * Home/End uçlara gider; kenarda odak yerinde kalır.
 */
export function stripStep(key: string, index: number, count: number): number | null {
    if (key === 'ArrowUp') return Math.max(0, index - 1);
    if (key === 'ArrowDown') return Math.min(count - 1, index + 1);
    if (key === 'Home') return 0;
    if (key === 'End') return count - 1;
    return null;
}

/**
 * İşaretçinin konumundan bırakma boşluğu: sayfanın orta noktasının üstü
 * "önüne", altı "arkasına" demektir. `midpoints` küçük resimlerin ekrandaki
 * dikey orta noktalarıdır, yukarıdan aşağıya.
 */
export function dropGap(midpoints: readonly number[], y: number): number {
    const index = midpoints.findIndex(mid => y < mid);
    return index < 0 ? midpoints.length : index;
}

/**
 * Şerit kenarına yaklaşınca kendiliğinden kaydırma: kenara ne kadar
 * yakınsa o kadar hızlı, kenar bölgesinin dışında sıfır. Kare başına piksel.
 */
export function edgeScroll(y: number, top: number, bottom: number, zone = 40, max = 16): number {
    if (y < top + zone) return -Math.ceil(max * Math.min(1, (top + zone - y) / zone));
    if (y > bottom - zone) return Math.ceil(max * Math.min(1, (y - (bottom - zone)) / zone));
    return 0;
}

/** Geri Al geçmişi: en eski adım düşer, bellek sınırlı kalır. */
export const HISTORY_LIMIT = 50;
export function pushHistory<T>(history: readonly T[], entry: T, limit = HISTORY_LIMIT): T[] {
    return [...history, entry].slice(-limit);
}

export type MoveKind = 'start' | 'end' | 'up' | 'down' | 'drop';

/**
 * Ekran okuyucuya taşımanın sonucu. Tek sayfada sayfa kaynağıyla anılır ve
 * vardığı sıra söylenir; birden fazla sayfada sayısı.
 */
export function describeMove(moved: { page: number }[], kind: MoveKind, position: number): string {
    if (moved.length === 1) {
        const who = `Kaynak s. ${moved[0].page}`;
        if (kind === 'start') return `${who} başa taşındı.`;
        if (kind === 'end') return `${who} sona taşındı.`;
        return `${who} ${position}. sıraya taşındı.`;
    }
    const who = `${moved.length} sayfa`;
    if (kind === 'start') return `${who} başa taşındı.`;
    if (kind === 'end') return `${who} sona taşındı.`;
    if (kind === 'up') return `${who} birer sıra yukarı taşındı.`;
    if (kind === 'down') return `${who} birer sıra aşağı taşındı.`;
    return `${who} ${position}. sıraya taşındı.`;
}
