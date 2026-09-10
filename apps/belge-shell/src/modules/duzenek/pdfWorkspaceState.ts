/** Cumulative per-page rotation; changing selection never rewrites prior edits. */
export function rotatePages(previous: Record<string, number>, selected: string[], delta: -90 | 90): Record<string, number> {
    const next = { ...previous };
    for (const key of selected) next[key] = ((next[key] || 0) + delta + 360) % 360;
    return next;
}

export type PreviewMode = number | 'fit-page' | 'fit-width';
/** CSS geometry in pixels. One scale is applied to both axes, including rotation. */
export function previewGeometry(width: number, height: number, rotation: number, viewportWidth: number, viewportHeight: number, mode: PreviewMode) {
    const swapped = Math.abs(rotation % 180) === 90;
    const w = swapped ? height : width, h = swapped ? width : height;
    const availableWidth = Math.max(1, viewportWidth - 16), availableHeight = Math.max(1, viewportHeight - 16);
    const scale = typeof mode === 'number' ? mode / 100 : mode === 'fit-width' ? availableWidth / w : Math.min(availableWidth / w, availableHeight / h);
    return { width: w * scale, height: h * scale, imageWidth: width * scale, imageHeight: height * scale, scale };
}

/**
 * Düzenle'ye verilen belgenin durumu.
 *
 * `none` — kipe henüz belge verilmedi. `loading` — verilen belge taranıyor.
 * `ready` — belge tarandı, üzerinde çalışılabilir bir oturum var. `failed` —
 * belge açılamadı ya da işlenemedi.
 */
export type DocumentState = 'none' | 'loading' | 'ready' | 'failed';

/**
 * Hangi çalışma yüzeyleri çizilir?
 *
 * Belge açılamadıysa ya da henüz yoksa araç paneli çizilmez: on bir aracın
 * hiçbiri iş göremez, panel yalnız hata/boş durumdan yer çalar. Sayfa şeridi
 * bir adım daha dar koşula bağlıdır — gerçekten sayfa olmalı. Kullanıcı yeni
 * girdi bekleyen bir araca geçtiğinde (Görseller → PDF) oturum sürer, panel
 * kalır, yalnız şerit kapanır.
 */
export function workspaceSurfaces(state: DocumentState, pageCount: number): { tools: boolean; strip: boolean } {
    const open = state === 'ready';
    return { tools: open, strip: open && pageCount > 0 };
}
