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
 * Yerel önizlemenin piksel bütçesi: crates/ekler-core/src/raster.rs genişlik ×
 * yükseklik 40 000 000'u aşan bitmap'i "Görsel boyutu güvenli sınırı aşıyor"
 * diye reddeder. Sınır bir güvenlik denetimidir; burada yalnız aşılmaz.
 */
const PREVIEW_PIXEL_BUDGET = 40_000_000;
/** Büyük önizlemenin en düşük dpi'si: bir CSS pikseline bir görüntü pikseli. */
const PREVIEW_BASE_DPI = 96;
/**
 * Büyük önizlemenin dpi'si: ekran ölçeği × yakınlaştırma, 96..300 arası.
 *
 * Sayfanın boyu biliniyorsa (`pageCss`: ilk başarılı render'ın bitmap'i, 96
 * dpi CSS pikseli) istek yerel 40 MP sınırını da aşmaz. A1 Retina'da %150'de
 * 288 dpi sınırı aşıyor, sayfanın üstünde hata şeridi kalıyordu; artık
 * bütçeye sığan en keskin dpi istenir. A4 gibi olağan sayfalarda 300 dpi bile
 * bütçenin çok altındadır: sonuç bugünküyle aynıdır.
 */
export function previewDpi(scale: number, dpr: number, pageCss?: { width: number; height: number }): number {
    const wanted = Math.min(300, Math.max(PREVIEW_BASE_DPI, Math.ceil(PREVIEW_BASE_DPI * scale * dpr)));
    if (!pageCss || !(pageCss.width > 0 && pageCss.height > 0)) return wanted;
    // Motor her kenarı yukarı yuvarlar (ceil(pt · dpi / 72)); karekökten gelen
    // sınır o yuvarlamayla bütçeyi bir satır aşabilir, sığana dek inilir.
    const pixels = (dpi: number) => Math.ceil(pageCss.width * dpi / PREVIEW_BASE_DPI) * Math.ceil(pageCss.height * dpi / PREVIEW_BASE_DPI);
    let dpi = Math.min(wanted, Math.floor(PREVIEW_BASE_DPI * Math.sqrt(PREVIEW_PIXEL_BUDGET / (pageCss.width * pageCss.height))));
    while (dpi > PREVIEW_BASE_DPI && pixels(dpi) > PREVIEW_PIXEL_BUDGET) dpi--;
    return Math.max(PREVIEW_BASE_DPI, dpi);
}
/**
 * Başarısız büyük render bir kez daha hangi dpi'de denensin? `null`: denenmez.
 *
 * İlk render sayfanın boyunu bilmeden ekran ölçeğiyle ister (Retina'da 192).
 * A0 ve piksel birimiyle yazılmış taranmış sayfalar bu dpi'de yerel sınırı
 * aşar, 96 dpi'de sığar. Yalnız bitmap yokken ve yalnız bir kez: sayfa zaten
 * çizilmişse hata yerleşim dışı şerittir; 96 dpi de başarısızsa hata gösterilir.
 */
export function previewRetryDpi(failedDpi: number, hasBitmap: boolean): number | null {
    return !hasBitmap && failedDpi > PREVIEW_BASE_DPI ? PREVIEW_BASE_DPI : null;
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

/**
 * Birleştir listesine yeni taranan belgeleri ekler.
 *
 * Kabuk Düzenle'yi her zaman TEK açık belgeyle açar; Birleştir'in "Belge Ekle"
 * düğmesi o belgenin ÜSTÜNE eklemelidir. Eskiden seçilen dosyalar listenin
 * yerine geçiyordu: yalnız B'yi seçen kullanıcının açık A'sı uyarısız düşüyor,
 * kaydedilen "birleştirme" yalnız B'den oluşuyordu.
 *
 * Açık belge ilk sırada kalır (taban); yeniler seçim sırasıyla sona eklenir.
 * Listede zaten olan bir yol yeniden eklenmez, adı `skipped` ile döner ki
 * kullanıcıya tek satırla söylenebilsin. Girdi dizileri değiştirilmez.
 */
export function appendSources<T extends { path: string; file_name: string }>(existing: readonly T[], scanned: readonly T[]): { next: T[]; skipped: string[] } {
    const seen = new Set(existing.map(source => source.path));
    const next = [...existing];
    const skipped: string[] = [];
    for (const source of scanned) {
        if (seen.has(source.path)) {
            skipped.push(source.file_name);
            continue;
        }
        seen.add(source.path);
        next.push(source);
    }
    return { next, skipped };
}

/** Listede bir belgeyi bir adım yukarı (-1) ya da aşağı (+1) taşır; sınırda liste aynen döner. */
export function shiftSource<T>(list: readonly T[], index: number, delta: -1 | 1): T[] {
    const target = index + delta;
    if (index < 0 || index >= list.length || target < 0 || target >= list.length) return [...list];
    const next = [...list];
    [next[index], next[target]] = [next[target], next[index]];
    return next;
}

/** Listeden bir belgeyi çıkarır. Kaynak dosyaya dokunulmaz; yalnız bu birleştirmeye girmez. */
export function removeSource<T>(list: readonly T[], index: number): T[] {
    return list.filter((_, i) => i !== index);
}

export const MERGE_NEEDS_MORE = 'Birleştirmek için en az bir belge daha ekleyin.';

/**
 * Birleştir neden kaydedemiyor? Boş dize: engel yok.
 *
 * Tek belgeyle "Birleştir" markalı bir kopya üretiyor ve "PDF kaydedildi"
 * diyordu — birleştirilmiş hiçbir şey yokken. Kaydet artık kapalı ve nedeni
 * görünür: düğme sebepsiz sönük kalırsa yeni bir "hiçbir şey olmuyor"
 * şikâyeti doğar.
 */
export function mergeShortfall(kind: string, sourceCount: number): string {
    return kind === 'merge' && sourceCount < 2 ? MERGE_NEEDS_MORE : '';
}

/**
 * Araç değişince listede kalan belgeler.
 *
 * Görseller → PDF'e girip çıkarken liste boşalır (görsel ile PDF karışmaz);
 * Birleştir bütün listeyi tutar; öteki araçlar tek belgeyle çalışır. O tek
 * belge kabuğun açtığı belgedir (`openedPath`; bardaki ad onu gösterir), liste
 * yeniden sıralanmış olsa da. Eskiden listenin ilki kalıyordu: B başa
 * taşındıktan sonra Sıkıştır'a geçmek A'yı kapatıp B'yi tutuyor, bar ise hâlâ
 * "A.pdf" diyordu. Kabuğun belgesi listeden çıkarıldıysa listenin ilki kalır.
 */
export function keptSources<T extends { path: string }>(from: string, to: string, sources: readonly T[], openedPath?: string): T[] {
    if (to === 'images' || from === 'images') return [];
    if (to === 'merge') return [...sources];
    const opened = sources.find(source => source.path === openedPath);
    return opened ? [opened] : sources.slice(0, 1);
}

/**
 * Sırala dışında sayfa sürükleyen kullanıcıya verilecek ipucu; boş dize: ipucu yok.
 *
 * Sırala tek belgeyle çalışır. Birleştir'de birden çok belge varken ona geçmek
 * eklenen belgeleri kapatır, Görseller → PDF'te seçilen görselleri bırakır:
 * ipucuna uyan kullanıcı düzenlemeye çalıştığı işi kaybediyordu. İpucu yalnız
 * Sırala'nın açık sayfaları aynen tutacağı yerde verilir.
 */
export function dragHint(kind: string, sourceCount: number): string {
    return kind !== 'reorder' && kind !== 'images' && sourceCount === 1
        ? 'Sayfaları sürükleyerek sıralamak için Sırala aracını seçin.'
        : '';
}

/**
 * Ekleme sonucunun tek satırı.
 *
 * Ekleme AÇMA değildir: "açıldı" ya da "Belge açılamadı" demek, açık belgenin
 * bozulduğu izlenimini verir. Burada yalnız eklenen/eklenemeyen dosyalar
 * adlandırılır. `detail`, hiçbir dosya eklenemediğinde kategorinin tek
 * cümlesidir (bkz. describeOpenFailure). Boş metin: söylenecek ek bir şey yok.
 */
export function appendNote(added: number, skipped: readonly string[], failed: readonly string[], detail = ''): { text: string; tone: 'info' | 'error' } {
    const parts: string[] = [];
    if (failed.length)
        parts.push(added
            ? `${added} belge eklendi. ${failed.length} dosya eklenemedi: ${failed.join(', ')}.`
            : `${failed.join(', ')} eklenemedi.${detail ? ` ${detail}` : ''}`);
    if (skipped.length)
        parts.push(`${skipped.join(', ')} zaten listede; yeniden eklenmedi.`);
    return { text: parts.join(' '), tone: failed.length ? 'error' : 'info' };
}
