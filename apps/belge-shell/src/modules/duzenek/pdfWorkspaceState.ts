/** Cumulative per-page rotation; changing selection never rewrites prior edits. */
export function rotatePages(previous: Record<string, number>, selected: string[], delta: -90 | 90): Record<string, number> {
    const next = { ...previous };
    for (const key of selected) next[key] = ((next[key] || 0) + delta + 360) % 360;
    return next;
}

export type PreviewMode = number | 'fit-page' | 'fit-width';
/** Büyük önizleme sahnesinin iç boşluğu (pdf.css `.pdf-preview-stage`: `padding: var(--space-2)`). */
export const STAGE_PADDING = 8;
/** CSS geometry in pixels. One scale is applied to both axes, including rotation. */
export function previewGeometry(width: number, height: number, rotation: number, viewportWidth: number, viewportHeight: number, mode: PreviewMode) {
    const swapped = Math.abs(rotation % 180) === 90;
    const w = swapped ? height : width, h = swapped ? width : height;
    const availableWidth = Math.max(1, viewportWidth - 2 * STAGE_PADDING), availableHeight = Math.max(1, viewportHeight - 2 * STAGE_PADDING);
    const scale = typeof mode === 'number' ? mode / 100 : mode === 'fit-width' ? availableWidth / w : Math.min(availableWidth / w, availableHeight / h);
    return { width: w * scale, height: h * scale, imageWidth: width * scale, imageHeight: height * scale, scale };
}
/**
 * Büyük önizleme sahnesinin AÇIK boyutu: yalnız sayfa + iç boşluk. Ölçülen
 * kutuya bağlı DEĞİLDİR; kutuyu doldurmak CSS'in işidir (`min-width/min-height:
 * 100%`). Sığdırma kiplerinde bu boyut ölçülen kutuyu aşamaz, dolayısıyla sahne
 * kendi başına kaydırma çubuğu doğuramaz.
 */
export function previewStageSize(geometry: { width: number; height: number }) {
    return { width: geometry.width + 2 * STAGE_PADDING, height: geometry.height + 2 * STAGE_PADDING };
}
/**
 * Kaydırma kutusunun yeni ölçüsü. Ölçü değişmediyse ÖNCEKİ nesne döner: React
 * yeniden çizmez; geometri, dpi ve render zinciri başlamaz. Genişlik
 * `clientWidth`'tir ve pdf.css dikey çubuğun yerini her motorda hep ayırdığı
 * için çubuğun o an görünüp görünmemesine bağlı değildir.
 */
export function nextViewport(previous: { width: number; height: number }, box: { clientWidth: number; clientHeight: number }) {
    return previous.width === box.clientWidth && previous.height === box.clientHeight
        ? previous
        : { width: box.clientWidth, height: box.clientHeight };
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
