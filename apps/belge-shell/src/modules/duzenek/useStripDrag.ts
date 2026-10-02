/**
 * Sayfa şeridinde sürükleyerek sıralama — işaretçi olaylarıyla.
 *
 * HTML5 sürükle-bırak KULLANILMAZ. Tauri'nin dosya bırakma işleyicisi
 * (`dragDropEnabled: true`, belgeyi Finder/Explorer'dan bırakarak açmak için
 * gerekli) yerel sürüklemeyi yutar: WebView2'de DOM `drop` hiç gelmez,
 * WKWebView'da da WebKit'e ulaşmaz. Saha belirtisi buydu: küçük resim
 * sürüklenince tarayıcı sayfa görselinin kendisini sürüklüyor, bırakınca
 * hiçbir şey olmuyordu. İşaretçi olayları bu yoldan geçmez; önizleme
 * görselleri de `draggable={false}` taşır ki yerel sürükleme hiç başlamasın.
 *
 * Kurallar: ~4 px eşiğin altı tıklamadır (bkz. `dragGesture`). Eşik aşılınca işaretçi yakalanır,
 * bırakma yeri sayfaların orta noktasından hesaplanır, şerit kenarında
 * kendiliğinden kaydırılır. Esc ya da `pointercancel` iptal eder; sürüklemeden
 * sonra gelen tıklama yutulur (yoksa seçim sürüklenen tek sayfaya çökerdi).
 * Dokunmatik sürükleme şeridi kaydırır; sıralama düğmelerle yapılır.
 */
import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent, type RefObject } from 'react';
import { dropGap, edgeScroll } from './pageOrder';

export const DRAG_THRESHOLD = 4;
/** Sıralama kapalıyken ipucu ancak bu kadar yol alan bir sürüklemede verilir. */
export const REFUSE_DISTANCE = 24;
/** Şeridin dışına bu kadar taşan işaretçi hâlâ şeritte sayılır. */
const SLACK = 24;

export type Gesture = 'pending' | 'drag' | 'refuse';

/**
 * Basılı işaretçinin aldığı yol ne demek? Sırala'da ~4 px'ten sonrası
 * sürüklemedir ve ardından gelen tıklama yutulur. Başka bir araçta hiçbir
 * hareket sürükleme değildir: tıklama tıklama olarak kalır (önizlenen sayfa
 * değişir), titrek bir el ipucu metni doğurmaz; yalnız belirgin bir sürükleme
 * sıralamanın nerede olduğunu söyletir.
 */
export function dragGesture(distance: number, enabled: boolean): Gesture {
    if (!enabled) return distance >= REFUSE_DISTANCE ? 'refuse' : 'pending';
    return distance >= DRAG_THRESHOLD ? 'drag' : 'pending';
}

export type StripDrag = {
    /** Sürüklenen sayfalar. */
    keys: string[];
    /** Bırakılırsa sayfaların gideceği boşluk; sıra değişmeyecekse null. */
    gap: number | null;
};

export type StripDragOptions = {
    listRef: RefObject<HTMLElement | null>;
    /** Sıralama şu an mümkün mü (Sırala aracı, meşgul değil)? */
    enabled: boolean;
    /** Sürükleme başlarken taşınacak sayfalar: basılan sayfa seçiliyse bütün seçim. */
    begin: (key: string) => string[];
    /** Bu boşluğa bırakmak sırayı değiştirir mi? */
    changes: (keys: string[], gap: number) => boolean;
    onDrop: (keys: string[], gap: number) => void;
    /** Sıralama kapalıyken sürüklenmeye çalışıldı. */
    onRefused?: () => void;
};

export function useStripDrag(options: StripDragOptions) {
    const [drag, setDrag] = useState<StripDrag | null>(null);
    const latest = useRef(options);
    latest.current = options;
    const suppressClick = useRef(false);
    const stop = useRef<(() => void) | null>(null);
    useEffect(() => () => stop.current?.(), []);

    const onPointerDown = (event: ReactPointerEvent<HTMLElement>, key: string) => {
        suppressClick.current = false;
        if (event.button !== 0 || event.pointerType === 'touch' || stop.current) return;
        // Sayfanın kendi denetimleri (Seç kutusu, ↑/↓) sürükleme tutamağı değildir.
        if ((event.target as Element).closest('input, label, .thumbnail-move')) return;
        const element = event.currentTarget, pointer = event.pointerId;
        const x0 = event.clientX, y0 = event.clientY;
        let x = x0, y = y0, started = false, cancelled = false, frame = 0;
        let keys: string[] = [], gap: number | null = null;

        const box = () => latest.current.listRef.current;
        const inStrip = (rect: DOMRect) => x >= rect.left - SLACK && x <= rect.right + SLACK;
        const locate = (): number | null => {
            const list = box();
            if (!list || !inStrip(list.getBoundingClientRect())) return null;
            const midpoints = Array.from(list.querySelectorAll<HTMLElement>('.thumbnail'), item => {
                const rect = item.getBoundingClientRect();
                return rect.top + rect.height / 2;
            });
            const next = dropGap(midpoints, y);
            return latest.current.changes(keys, next) ? next : null;
        };
        const update = () => {
            const next = locate();
            if (next !== gap) { gap = next; setDrag({ keys, gap }); }
        };
        const scroll = () => {
            frame = 0;
            const list = box();
            if (!started || cancelled || !list) return;
            const rect = list.getBoundingClientRect();
            const step = inStrip(rect) ? edgeScroll(y, rect.top, rect.bottom) : 0;
            if (!step) return;
            const before = list.scrollTop;
            list.scrollTop += step;
            if (list.scrollTop === before) return;
            update();
            frame = requestAnimationFrame(scroll);
        };
        const release = () => {
            window.removeEventListener('pointermove', move);
            window.removeEventListener('pointerup', up);
            window.removeEventListener('pointercancel', cancel);
            window.removeEventListener('keydown', escape, true);
            cancelAnimationFrame(frame);
            stop.current = null;
            setDrag(null);
        };
        const move = (e: PointerEvent) => {
            if (e.pointerId !== pointer) return;
            x = e.clientX; y = e.clientY;
            if (cancelled) return;
            if (!started) {
                const gesture = dragGesture(Math.hypot(x - x0, y - y0), latest.current.enabled);
                if (gesture === 'pending') return;
                if (gesture === 'refuse') {
                    cancelled = true;
                    latest.current.onRefused?.();
                    return;
                }
                started = true;
                suppressClick.current = true;
                keys = latest.current.begin(key);
                try { element.setPointerCapture(pointer); } catch { /* işaretçi zaten bırakılmış */ }
                setDrag({ keys, gap: null });
            }
            update();
            if (!frame) frame = requestAnimationFrame(scroll);
        };
        const up = (e: PointerEvent) => {
            if (e.pointerId !== pointer) return;
            const target = started && !cancelled ? locate() : null;
            release();
            if (target !== null) latest.current.onDrop(keys, target);
            // Bu işaretçinin tıklaması (varsa) aynı görevde gelir; sonra sıfırlanır
            // ki klavyeyle yapılan bir sonraki tıklama yutulmasın.
            window.setTimeout(() => { suppressClick.current = false; }, 0);
        };
        const cancel = (e: PointerEvent) => {
            if (e.pointerId !== pointer) return;
            release();
            window.setTimeout(() => { suppressClick.current = false; }, 0);
        };
        const escape = (e: KeyboardEvent) => {
            if (e.key !== 'Escape' || !started || cancelled) return;
            e.preventDefault();
            e.stopPropagation();
            cancelled = true;
            gap = null;
            cancelAnimationFrame(frame);
            setDrag(null);
        };
        window.addEventListener('pointermove', move);
        window.addEventListener('pointerup', up);
        window.addEventListener('pointercancel', cancel);
        window.addEventListener('keydown', escape, true);
        stop.current = release;
    };

    const onClickCapture = (event: ReactMouseEvent) => {
        if (!suppressClick.current) return;
        suppressClick.current = false;
        event.preventDefault();
        event.stopPropagation();
    };

    return { drag, onPointerDown, onClickCapture };
}
