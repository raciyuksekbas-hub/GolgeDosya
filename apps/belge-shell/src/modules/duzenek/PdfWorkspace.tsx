/**
 * Düzenle — PDF çalışma alanı.
 *
 * Bağımsız DüzenEk'ten taşındı. **Davranış** değişmedi: komut adları, seçim /
 * sıralama / döndürme mantığı, önizleme kuyruğu, imza onayı ve durum metinleri
 * aynı. Değişen, bu davranışların çizildiği yerdir — migration dondurması
 * görsel yeniden kompozisyon turunda kalktı (bkz. docs/DESIGN.md).
 *
 * Öncelik sırası: PDF > sayfalar > araçlar > ayarlar.
 *   - Belge alanı ortada ve en geniş; solunda sayfa şeridi.
 *   - Günlük iş olan SAYFALAR araçları YALNIZ belgenin üstündeki şeritte,
 *     segment olarak; döndürme ve yakınlaştırma da orada.
 *   - BELGE ve katlı DİĞER grupları, seçili aracın ayarı ve kaynak listesi
 *     sağ panelde.
 *   - Kaydetme eylemleri yardımcı barda; sonuç metni workspace'te kalır ki
 *     panel kapalıyken de görünsün.
 *
 * Kabuk yokken (sunucu tarafı render) bar ve panel içeriği olduğu yerde satır
 * içi çizilir; hiçbir yüzey kaybolmaz.
 *
 * Yüzeyler belgenin durumuna bağlıdır (`workspaceSurfaces`): belge açılamadıysa
 * ya da henüz yoksa ne araç paneli ne de sayfa şeridi çizilir — kullanılamayan
 * araçları soluk göstermek yerine hata/boş durum tam genişliği alır.
 */
import { rotatePages, previewGeometry, previewStageSize, nextViewport, previewDpi, previewRetryDpi, workspaceSurfaces, appendSources, appendNote, shiftSource, removeSource, mergeShortfall, dragHint, type DocumentState, type PreviewMode } from './pdfWorkspaceState';
import { noBenefitStatus } from './noBenefitMessage';
import { savedStatus } from './savedMessage';
import { humanSize } from '../../shared-ui/sizeText';
import { copyDestination } from './copyDestination';
import { SavedPlace, startFolder } from '../../shared-ui/savePlace';
import { describeOpenFailure, describePreviewFailure, describeSaveFailure, logFailure, OPEN_FAILURE_FALLBACK, OPEN_FAILURE_TITLE } from '../../shared-ui/failure';
import React, { useState, useEffect, useMemo, useRef } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { ScanBatchResult, SourceFile } from './types';
import { InspectorPanel, InspectorSection, ToolbarActions, ToolbarStatus, useRevealInspector } from '../../shell/chrome';
import { Button, EmptyState, IconButton, Status } from '../../shared-ui/primitives';
import { announce } from '../../shared-ui/Announcer';
import { useShortcuts } from '../../shared-ui/useShortcuts';
import { isToggleChord, shortcutLabel } from '../../shell/platform';
import { describeMove, isReordered, moveKeys, pushHistory, sameOrder, selectPages, shiftKeys, stripStep, type MoveKind } from './pageOrder';
import { useStripDrag } from './useStripDrag';
import './pdf.css';
const tools = {
    merge: ['Birleştir', 'Açık belgeye eklediğiniz PDF’leri listedeki sırayla tek dosyada birleştirir.'],
    select: ['Seçili sayfalar → yeni PDF', 'İşaretlediğiniz sayfalardan yeni bir PDF kopyası oluşturur.'],
    reorder: ['Sayfa sırasını değiştir', 'Sayfaları şeritte sürükleyerek ya da taşıma düğmeleriyle yeniden sıralayıp yeni bir kopya oluşturur.'],
    delete: ['Sayfa sil', 'İşaretlediğiniz sayfaları çıkararak yeni bir PDF oluşturur.'],
    rotate: ['Döndür', 'Önizlediğiniz sayfayı her tıklamada 90° saat yönünde döndürür.'],
    compress: ['Sıkıştır', 'Belgeyi küçültür ve kalite kontrolünü geçen yeni bir kopya kaydeder.'],
    images: ['Görseller → PDF', 'Bir veya daha fazla görselden PDF oluşturur.'],
    crop: ['Kırp', 'Sayfanın görünür alanını daraltarak yeni kopya üretir; içerik silinmez.'],
    watermark: ['Filigran', 'Yeni kopyaya bir metin işareti ekler.'],
    number: ['Sayfa numarası', 'Yeni kopyaya sıralı sayfa numarası ekler.'],
    raster: ['PDF → PNG/JPG', 'PDF sayfalarını ayrı görsel dosyalara dönüştürür.'],
};

type Kind = keyof typeof tools;

/** Şerit dar: segment etiketleri kısadır, tam ad ipucunda ve panelde durur. */
const SHORT: Record<Kind, string> = {
    merge: 'Birleştir', select: 'Seç', reorder: 'Sırala', delete: 'Sil', rotate: 'Döndür',
    compress: 'Sıkıştır', images: 'Görseller', crop: 'Kırp', watermark: 'Filigran',
    number: 'Numara', raster: 'Görsele',
};

/**
 * Araç hiyerarşisi.
 *
 * On bir araç eşit görsel ağırlıkta duruyordu; kullanıcı hangisinin günlük iş,
 * hangisinin nadir ayarlı işlem olduğunu ayırt edemiyordu. Gruplama gerçek
 * kullanım modelinden çıkarıldı, keyfi değil:
 *
 *   Sayfalar — açık belgenin SAYFA SEÇİMİ üzerinde çalışır. Bu dört araç
 *              PANELDE DEĞİL, belgenin üstündeki şeritte durur (`PAGE_TOOLS`):
 *              sayfa işi sayfaların yanında yapılır ve ancak sayfa varken
 *              anlamlıdır. İkisinde birden çizilince aynı dört komut aynı
 *              ekranda iki ayrı evde görünüyordu.
 *   Belge    — belgenin BÜTÜNÜ üzerinde çalışır, sayfa seçimi gerektirmez.
 *   Diğer    — ek AYAR ister (kenar boşluğu, metin, başlangıç numarası) ya da
 *              farklı bir girdi türü alır. Seyrek; katlanmış durur.
 *
 * `kind` modeli, komut adları ve motor sözleşmesi DEĞİŞMEDİ — yalnız sunum.
 */
const TOOL_GROUPS: { title: string; keys: Kind[]; collapsed?: boolean }[] = [
    { title: 'Belge', keys: ['merge', 'compress', 'raster'] },
    { title: 'Ek Araçlar', keys: ['crop', 'watermark', 'number', 'images'], collapsed: true },
];

/**
 * Seçili aracın panelde cevaplaması gereken sorular.
 *
 * Panel eskiden yalnız "ne yapar"ı söylüyordu ve grubun adı "Diğer"di —
 * hiçbir şey anlatmayan bir ad. Kullanıcı aracın NEYİ etkilediğini ve ÇIKTININ
 * ne olacağını ekrandan okuyamıyordu; panel geliştirici notu gibi duruyordu.
 *
 * Bu tablo dokümantasyon değildir: her araç için iki kısa satır.
 *   affects — bu araç neyin üzerinde çalışır?
 *   output  — sonunda ne oluşur?
 */
const TOOL_FACTS: Record<Kind, { affects: string; output: string }> = {
    merge: { affects: 'Açık belge ilk sırada; eklediğiniz belgeler sonuna eklenir, sırayı oklarla değiştirebilirsiniz.', output: 'Tek yeni PDF.' },
    select: { affects: 'Şeritte işaretlediğiniz sayfalar.', output: 'Yalnız o sayfalardan yeni PDF.' },
    reorder: { affects: 'Belgenin tamamı; sıra şeritten değişir.', output: 'Aynı sayfalar, yeni sırayla yeni PDF.' },
    delete: { affects: 'Şeritte işaretlediğiniz sayfalar.', output: 'O sayfalar olmadan yeni PDF.' },
    rotate: { affects: 'Yalnız önizlediğiniz sayfa.', output: 'Döndürülmüş sayfayla yeni PDF.' },
    compress: { affects: 'Belgenin tamamı.', output: 'Daha küçük yeni PDF. Küçülme çok azsa yeni dosya oluşturulmaz.' },
    images: { affects: 'Kendi seçicisinden seçtiğiniz görseller.', output: 'Görsellerden oluşan yeni PDF.' },
    crop: { affects: 'Belgenin bütün sayfaları.', output: 'Kenarları daraltılmış yeni PDF; içerik silinmez.' },
    watermark: { affects: 'Belgenin bütün sayfaları.', output: 'Metin işareti eklenmiş yeni PDF.' },
    number: { affects: 'Belgenin bütün sayfaları.', output: 'Numaralandırılmış yeni PDF.' },
    raster: { affects: 'Belgenin bütün sayfaları.', output: 'Seçtiğiniz klasöre sayfa başına bir görsel dosyası.' },
};

/**
 * Sıkıştırma ön ayarları. `value`, motorun `OptimizationLevel` adıdır ve
 * değişmez. Etiket ne elde edileceğini söyler; piksel ve JPEG kalitesi
 * avukata bir şey anlatmıyordu, yalnız ipucunda durur.
 */
export const COMPRESSION_PRESETS = [
    { value: 'gentle_compression', label: 'Hafif — görüntü kalitesi en yüksek', detail: 'En fazla 2400 px, JPEG kalitesi 80' },
    { value: 'balanced_compression', label: 'Dengeli — çoğu belge için önerilir', detail: 'En fazla 2000 px, JPEG kalitesi 72' },
    { value: 'aggressive_compression', label: 'Güçlü — en küçük dosya', detail: 'En fazla 1600 px, JPEG kalitesi 65' },
] as const;

/** Kaynak listesindeki satır: ad, sayfa ve boyut ("dilekce.pdf · 30 sayfa · 9,8 MB"). */
export function sourceLine(s: Pick<SourceFile, 'file_name' | 'page_count' | 'size_bytes'>): string {
    return [s.file_name, `${s.page_count} sayfa`, s.size_bytes > 0 ? humanSize(s.size_bytes) : ''].filter(Boolean).join(' · ');
}

type Page = {
    source: SourceFile;
    page: number;
    key: string;
};
/** Sırala şeridinin klavye yolunu ekran okuyucuya söyleyen gizli metnin kimliği. */
const STRIP_KEYS_ID = 'sirala-serit-tuslari';
// Bound native render requests; thumbnails render only when near the viewport.
// Etkin sayfanın büyük önizlemesi küçük resim sütununun ARKASINDA beklemez:
// kendi kuyruğu vardır. Aynı anda en fazla iki render (biri etkin sayfa) —
// bellek sınırlı kalır, sayfa geçişi küçük resimlere takılmaz.
let queue: Promise<unknown> = Promise.resolve();
let activeQueue: Promise<unknown> = Promise.resolve();
/** Yol yerine dosya adı: kullanıcı belgesini adıyla tanır, yoluyla değil. */
export function baseNameOf(path: string): string {
    return path.split(/[\\/]/).pop() || path;
}

function Preview({ item, large = false, rotation = 0, mode = 'fit-page' }: {
    item: Page; large?: boolean; rotation?: number; mode?: PreviewMode;
}) {
    const [bitmap, setBitmap] = useState<{url: string; width: number; height: number} | null>(null);
    const [error, setError] = useState('');
    const deferredUrls = useRef(new Set<string>()).current;
    const [viewport, setViewport] = useState({width: 100, height: 130});
    const ref = useRef<HTMLDivElement>(null);
    /** Görünen bitmap'in dpi'si: aynı dpi için ikinci, birebir aynı render istenmez. */
    const renderedDpi = useRef(0);
    const geometry = bitmap ? previewGeometry(bitmap.width, bitmap.height, rotation, viewport.width, viewport.height, large ? mode : 'fit-page') : null;
    // Sayfanın boyu (bitmap) biliniyorsa dpi yerel 40 MP sınırını aşmaz.
    const dpi = large ? previewDpi(geometry?.scale || 1, typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1, bitmap ?? undefined) : 40;
    useEffect(() => {
        const target = large ? ref.current?.parentElement : ref.current;
        if (!target) return;
        // Yalnız gerçekten değişen ölçü durumu günceller: aynı ölçü yeni bir
        // geometri, yeni bir dpi ve yeni bir render demek değildir.
        const measure = () => setViewport(previous => nextViewport(previous, target));
        const observer = new ResizeObserver(measure);
        observer.observe(target);
        measure();
        return () => observer.disconnect();
    }, [large]);
    useEffect(() => {
        // Görünen bitmap zaten bu dpi'de çizildiyse (ilk render 96 dpi ile yeniden
        // denendiğinde dpi oraya iner) ve hata yoksa aynı render yeniden istenmez.
        if (renderedDpi.current === dpi && !error) return;
        let alive = true, started = false;
        const hadBitmap = bitmap !== null;
        const urls: string[] = [];
        const load = () => {
            if (started) return;
            started = true;
            const render = async (at: number): Promise<void> => {
                if (!alive) return;
                try {
                    // Rotation is visual and immediate; both views share the same unrotated bitmap geometry.
                    const bytes = await invoke<number[]>('duzenek_preview_pdf_page', {path: item.source.path, page: item.page, dpi: at, rotation: 0});
                    if (!alive) return;
                    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], {type: 'image/png'}));
                    urls.push(url);
                    const img = new Image(); img.src = url; await img.decode();
                    if (alive) { renderedDpi.current = at; setBitmap(previous => ({url, width: previous?.width || img.naturalWidth * 96 / at, height: previous?.height || img.naturalHeight * 96 / at})); setError(''); }
                } catch (e) {
                    // İlk render sayfanın boyunu bilmeden ekran ölçeğiyle ister. A0 ya da
                    // taranmış büyük sayfa Retina'da yerel 40 MP sınırını aşar: sayfa hiç
                    // görünmüyordu. Bitmap yokken bir kez, bütçeye sığan dpi ile denenir;
                    // ilk BAŞARILI render bitmap'in CSS boyunu belirler (yukarıdaki kural).
                    const retry = previewRetryDpi(at, hadBitmap);
                    if (retry !== null && alive) return render(retry);
                    if (alive) setError(String(e));
                }
            };
            if (large) activeQueue = activeQueue.catch(() => {}).then(() => render(dpi));
            else queue = queue.catch(() => {}).then(() => render(dpi));
        };
        const observer = new IntersectionObserver(entries => { if (entries.some(e => e.isIntersecting)) load(); }, {rootMargin: '100px'});
        if (ref.current) observer.observe(ref.current);
        return () => { alive = false; observer.disconnect(); /* URL remains visible while a sharper bitmap loads. */ urls.forEach(url => deferredUrls.add(url)); };
    }, [item.key, dpi]);
    useEffect(() => () => { deferredUrls.forEach(url => URL.revokeObjectURL(url)); deferredUrls.clear(); }, []);
    return <PreviewStage ref={ref} large={large} geometry={geometry} bitmap={bitmap} error={error} rotation={rotation} label={`${item.source.file_name}, sayfa ${item.page}`}/>;
}
type PreviewGeometry = ReturnType<typeof previewGeometry>;
/**
 * Önizleme sahnesi — saf sunum. Büyük önizleme de küçük resim de bunu çizer;
 * ölçü, render ve kuyruk `Preview`'dadır.
 *
 * Sahne ölçülen kutunun sayılarıyla BOYUTLANMAZ. Eskiden genişlik/yükseklik
 * `max(ölçülen kutu, sayfa)` idi: ölçüm bir kare gecikmeliydi ve sayfanın
 * kendi yarattığı kaydırma çubuğu ölçümü küçültüyordu. Büyük ölçüde taşan,
 * küçük ölçüde sığan bir sayfa her karede iki durum arasında gidip geldi
 * (sahada "tir tir titriyor"). Artık kutuyu CSS doldurur (min-width/
 * min-height: 100%, gerçek kesirli boyutla); burada yalnız sayfanın kendi
 * boyu verilir. Sığdırma kiplerinde sayfa kutudan büyük olamaz, dolayısıyla
 * sahne hiçbir zaman kendi başına kaydırma çubuğu doğurmaz.
 */
export const PreviewStage = React.forwardRef<HTMLDivElement, {
    large: boolean; geometry: PreviewGeometry | null; bitmap: {url: string} | null; error: string; rotation: number; label: string;
}>(function PreviewStage({ large, geometry, bitmap, error, rotation, label }, ref) {
    const message = describePreviewFailure(error, 'Bu sayfanın önizlemesi oluşturulamadı.');
    // Bekleme, dev bir kutunun ortasındaki tek kelime değil: küçük bir
    // döner ve kısa bir etiket. Hata olursa kategorisinin tek cümlesi
    // gösterilir; ham motor metni kullanıcı yüzeyine çıkmaz.
    const wait = error
        ? <p className="pdf-wait" data-tone="error" role="status" title={message}>{message}</p>
        : <p className="pdf-wait" role="status"><span className="spinner" aria-hidden="true"/>Önizleme hazırlanıyor</p>;
    return <div ref={ref} className={large ? 'pdf-preview-stage' : 'pdf-thumbnail-stage'} style={large && geometry ? previewStageSize(geometry) : undefined}>
        {bitmap && geometry ? <div className="pdf-page-surface" style={{width: geometry.width, height: geometry.height}}><img src={bitmap.url} alt={label} draggable={false} style={{width: geometry.imageWidth, height: geometry.imageHeight, transform: `translate(-50%, -50%) rotate(${rotation}deg)`}}/></div> : wait}
        {/* Sayfa çizilmişken sonraki (daha keskin) render başarısız olursa hata
            sayfanın YANINA değil üstüne, yerleşim dışı bir şerit olarak gelir:
            yanına konan cümle genişliği taşırıp kaydırma çubuğu döngüsünü
            başlatıyordu. */}
        {bitmap && error && <p className="pdf-wait pdf-wait-overlay" data-tone="error" role="status" title={message}>{message}</p>}
    </div>;
});
/**
 * Şeritteki tek sayfa — saf sunum; seçim, sıra ve sürükleme durumu çağıranda.
 *
 * Sayfa bir liste öğesidir; içindeki denetimler (görüntüle düğmesi, Seç/Dahil
 * et/Çıkar kutusu, ↑/↓) kardeştir, iç içe etkileşimli öğe yoktur. Seçim
 * çerçevesi yalnız seçimi olan araçlarda çizilir: Birleştir ve Döndür'de ilk
 * sayfa, kullanıcının değiştiremediği bir seçimle işaretli duruyordu.
 *
 * `tabbable` false iken sayfanın denetimleri sekme sırasından çıkar (tabindex
 * -1) ama odaklanabilir ve ekran okuyucuda erişilebilir kalır: Sırala'da
 * şerit tek sekme durağıdır (bkz. `stripKeys`).
 */
export function PageThumbnail({ item, index, count, kind, isCurrent, isSelected, rotation, busy, drop, dragging, tabbable = true, onView, onToggle, onShift, onPointerDown }: {
    item: Page; index: number; count: number; kind: Kind; isCurrent: boolean; isSelected: boolean; rotation: number; busy: boolean;
    /** Sürüklenen sayfaların bu sayfanın önüne mi arkasına mı ineceği. */
    drop?: 'before' | 'after'; dragging?: boolean;
    /** Bu sayfanın denetimleri sekme sırasında mı? */
    tabbable?: boolean;
    onView: (event: React.MouseEvent) => void; onToggle: () => void; onShift: (delta: -1 | 1) => void;
    onPointerDown?: (event: React.PointerEvent<HTMLElement>) => void;
}) {
    const tab = tabbable ? undefined : -1;
    const selectable = kind === 'select' || kind === 'delete' || kind === 'reorder';
    const pickVerb = kind === 'delete' ? 'Çıkar' : kind === 'reorder' ? 'Seç' : 'Dahil et';
    const pick = selectable && <label className="thumbnail-pick"><input type="checkbox" aria-label={`${index + 1}. sayfa (kaynak s. ${item.page}) — ${pickVerb}`} checked={isSelected} onChange={onToggle} tabIndex={tab}/>{pickVerb}</label>;
    return <div role="listitem" className={`thumbnail ${isCurrent ? 'current' : ''} ${selectable && isSelected ? 'selected' : ''} ${kind === 'delete' && isSelected ? 'removed' : ''}`} data-drop={drop} data-dragging={dragging || undefined} onPointerDown={onPointerDown}>
        {/* Önizleme motoru PDF SAYFASI çizer. Görsel kaynaklarda (Görseller
            → PDF aracı) bu çağrı zorunlu olarak düşüyor ve her küçük resim
            "önizleme oluşturulamadı" hatası gösteriyordu: araç çalışıyor
            ama tamamen bozukmuş gibi görünüyordu. Görselde hata yerine
            dosyanın kendisi söylenir. */}
        <button className="thumbnail-image-button" tabIndex={tab} aria-label={`${item.source.file_name} sayfa ${item.page} görüntüle`} aria-describedby={kind === 'reorder' ? STRIP_KEYS_ID : undefined} onClick={onView}>
            {item.source.format === 'pdf'
                ? <Preview item={item} rotation={rotation}/>
                : <span className="thumbnail-placeholder">{item.source.file_name}</span>}
        </button>
        <span className="thumbnail-no">{index + 1}<span className="sr-only">. çıktı sırası · Kaynak s. {item.page}</span></span>
        {/* Erişilebilir ad sayfayı TANIMLAR. Eskiden ad yalnız görünen
            sözcüktü ("Dahil et"); 30 sayfalık bir belgede ekran okuyucu
            30 kez aynı şeyi söylüyor, kullanıcı hangi kutuyu
            işaretlediğini ayırt edemiyordu — yanlış sayfayı çıkarma
            riski. Görünen sözcük adın İÇİNDE kalır (WCAG 2.5.3). */}
        {kind !== 'reorder' && pick}
        {/* Sırala: Seç kutusu klavyeyle birden fazla sayfa seçmenin yoludur;
            ↑/↓ tek sayfayı bir sıra taşır (sürüklemenin erişilebilir eşi). */}
        {/* Kenardaki ok `aria-disabled`: odakta kalır, basılınca bir şey
            olmaz. Odak öbür yönün okuna geçseydi, fazladan bir Enter sayfayı
            geldiği yöne geri götürürdü. */}
        {kind === 'reorder' && <div className="thumbnail-move">
            {pick}
            <span className="thumbnail-step">
                <IconButton label={`${index + 1}. sayfayı yukarı taşı (kaynak s. ${item.page})`} tabIndex={tab} aria-disabled={index === 0 || busy || undefined} onClick={() => index > 0 && !busy && onShift(-1)}>
                    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 12.5V4M4.5 7.5 8 4l3.5 3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                </IconButton>
                <IconButton label={`${index + 1}. sayfayı aşağı taşı (kaynak s. ${item.page})`} tabIndex={tab} aria-disabled={index === count - 1 || busy || undefined} onClick={() => index < count - 1 && !busy && onShift(1)}>
                    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 3.5V12m-3.5-3.5L8 12l3.5-3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                </IconButton>
            </span>
        </div>}
    </div>;
}
/**
 * Araç grupları — panelin görsel kütlesi.
 *
 * Saf sunum: hangi aracın seçili olduğunu ve tıklamayı çağıran tutar.
 */
export function ToolGroups({ kind, busy, onPick }: {
    kind: Kind; busy?: boolean; onPick: (key: Kind) => void;
}) {
    return <>{TOOL_GROUPS.map(group => {
        const buttons = group.keys.map(key =>
            <button key={key} className={`btn tool ${kind === key ? 'is-current' : ''}`} title={tools[key][1]} aria-pressed={kind === key} disabled={busy} onClick={() => onPick(key)}>{tools[key][0]}</button>);
        // Nadir araçlar katlı gelir ama içinde seçili bir araç varsa açık açılır:
        // kullanıcı seçtiği aracı kaybolmuş sanmamalı.
        if (group.collapsed) {
            return <details key={group.title} className="tool-group" open={group.keys.includes(kind)}>
                <summary>{group.title}</summary>
                <div className="tool-grid">{buttons}</div>
            </details>;
        }
        return <div key={group.title} className="tool-group">
            <h3 className="inspector-label">{group.title}</h3>
            <div className="tool-grid">{buttons}</div>
        </div>;
    })}</>;
}

/**
 * Belge yüzeyi yerine çizilen üç DIŞLAYICI durum.
 *
 * Eskiden hata, genel boş durum davetinin üstüne ekleniyordu: kullanıcı hem
 * "açılamadı" hem "PDF seçtiğinizde sayfaları burada göreceksiniz" okuyordu.
 * Artık her durumun tek karşılığı var ve hata durumu kendi kurtarma eylemini
 * taşıyor. Kart, dev panel, çizim ve teknik ayrıntı alanı yok.
 */
export function PreviewPlaceholder({ state, detail, onOpenAnother, forImages = false }: {
    state: DocumentState; detail: string; onOpenAnother: () => void;
    /** "Görseller → PDF" aracında ana alan PDF değil GÖRSEL ister. */
    forImages?: boolean;
}) {
    if (state === 'failed') {
        return <EmptyState
            alert
            title={OPEN_FAILURE_TITLE}
            note={detail || OPEN_FAILURE_FALLBACK}
            action={<Button onClick={onOpenAnother}>Başka Belge Aç</Button>}
        />;
    }
    if (state === 'loading') return <Status tone="busy">Belgeler inceleniyor…</Status>;
    // Görseller → PDF aracı GÖRSELLE çalışır; burada "PDF seçtiğinizde…"
    // demek hem yanlış türü ister hem de yüklenmiş görselleri yok sayar.
    if (forImages)
        return <EmptyState title="Görseller → PDF" note="Seçtiğiniz görseller soldaki şeritte listelenir; sırayı orada görebilirsiniz. Önizleme yalnız PDF sayfaları için çizilir." />;
    return <EmptyState title="Belge önizlemesi" note="PDF seçtiğinizde sayfaları burada göreceksiniz." />;
}

/**
 * Kaydedilmemiş düzenleme var mı? (§54)
 *
 * Sayfa sırası ya da döndürme, kaynağa değil yeni bir kopyaya uygulanır. Bu
 * düzen bir kopyaya kaydedilene dek yalnız bellektedir; kip değiştirmek, Kapat
 * ya da pencereyi kapatmak onu uyarısız siliyordu. Kaydedilen düzenin imzası
 * tutulur: aynı düzen kaydedildiyse kaybolacak bir şey yoktur.
 *
 * Sıra, KAYNAK sırasıyla karşılaştırılır. Eskiden "sıra elle değişti mi"
 * bayrağına bakılıyordu: sayfayı taşıyıp yerine geri getiren kullanıcı
 * kaybedecek bir şeyi olmadığı hâlde kapatırken uyarılıyordu.
 */
export function editSignature(order: { key: string }[], rotations: Record<string, number>, source: { key: string }[]): string {
    const turned = Object.entries(rotations).filter(([, d]) => d % 360 !== 0).sort(([a], [b]) => a.localeCompare(b));
    const reordered = isReordered(order, source);
    if (!reordered && turned.length === 0) return '';
    return JSON.stringify([reordered ? order.map(p => p.key) : [], turned]);
}

export function hasUnsavedEdits(signature: string, saved: string): boolean {
    return signature !== '' && signature !== saved;
}

export const PdfWorkspace: React.FC<{ paths?: string[]; onOpenDocument?: (paths: string[]) => void; onDirtyChange?: (dirty: boolean) => void }> = ({ paths: initialPaths, onOpenDocument, onDirtyChange }) => {
    const [kind, setKind] = useState<Kind>('merge'), [sources, setSources] = useState<SourceFile[]>([]), [order, setOrder] = useState<Page[]>([]), [selected, setSelected] = useState<string[]>([]), [current, setCurrent] = useState(''), [zoom, setZoom] = useState<PreviewMode>('fit-page');
    const [rotations, setRotations] = useState<Record<string, number>>({});
    // Kaynağın sayfa sırası: sıranın gerçekten değişip değişmediği buna göre
    // ölçülür (bkz. editSignature). Sırala'nın Geri Al geçmişi ve seçim çapası.
    const [baseline, setBaseline] = useState<Page[]>([]);
    const [history, setHistory] = useState<Page[][]>([]);
    const [anchor, setAnchor] = useState('');
    // Son başarılı kaydın düzen imzası (bkz. editSignature).
    const [savedSignature, setSavedSignature] = useState('');
    // "Sayfa yok" ile "belge açılamadı" aynı şey değildir: araç paneli yalnız
    // açık bir belge oturumunda çizilir, sayfa şeridi ise gerçekten sayfa varken.
    const [docState, setDocState] = useState<DocumentState>(initialPaths?.length ? 'loading' : 'none');
    const [failure, setFailure] = useState('');
    const [statusTone, setStatusTone] = useState<'info' | 'busy' | 'success' | 'error'>('info');
    const [margin, setMargin] = useState(10), [text, setText] = useState('KOPYA'), [imageFormat, setImageFormat] = useState('png'), [start, setStart] = useState(1), [level, setLevel] = useState('balanced_compression'), [approved, setApproved] = useState(false), [busy, setBusy] = useState(false), [status, setStatus] = useState('');
    // Son kaydın yeri. Yalnız kendi durum metni ekrandayken gösterilir: durum
    // başka bir cümleye geçince (yeni araç, hata, yeni kayıt) satır da gider.
    const [saved, setSaved] = useState<{ text: string; path: string; isFolder: boolean } | null>(null);
    const build = (list: SourceFile[]) => { setRotations({}); const next = list.flatMap(source => Array.from({ length: source.page_count }, (_, i) => ({ source, page: i + 1, key: source.path + '#' + (i + 1) }))); setOrder(next); setBaseline(next); setHistory([]); setAnchor(next[0]?.key || ''); setCurrent(next[0]?.key || ''); setSelected(next.length ? [next[0].key] : []); };
    // Her tarama bir nesil alır. Kabuk tarama sürerken başka bir belge açarsa
    // (bar, son kullanılanlar) eski taramanın sonucu yenisinin üstüne yazılmaz;
    // yalnız en son başlatılan tarama listeyi ve meşgul durumunu değiştirir.
    const loadGeneration = useRef(0);
    // Ekleme, tıklama anındaki kopyaya değil SON işlenen listeye yapılır.
    const sourcesRef = useRef(sources);
    sourcesRef.current = sources;
    // Tarama gövdesi, kendi seçicisi ile kabuğun açtığı belgeler arasında ortak.
    // `replace` oturumu verilen belgelerle yeniden kurar; `append` Birleştir'de
    // açık belgenin ÜSTÜNE ekler (açık belge ilk sırada kalır). `note`,
    // başarılı bir yeniden kurulumdan sonra durum satırında kalacak cümledir.
    const load = async (paths: string[], mode: 'replace' | 'append' = 'replace', note = '') => {
        if (!paths.length)
            return;
        const generation = ++loadGeneration.current;
        try {
            setBusy(true);
            if (mode === 'replace') {
                setDocState(previous => previous === 'ready' ? previous : 'loading');
                setFailure('');
            }
            setStatus(mode === 'append' ? 'Belgeler ekleniyor…' : 'Belgeler inceleniyor…'); setStatusTone('busy');
            const result = await invoke<ScanBatchResult>('duzenek_scan_source_files', { paths });
            if (generation !== loadGeneration.current)
                return;
            // TEK bozuk dosya, sağlam olanları da çöpe atmamalı. Eskiden
            // `errors.length` doluysa hata fırlatılıyor ve `result.sources` hiç
            // kullanılmıyordu: 20 taranmış ek seçip birini bozuk bulan kullanıcı
            // 19 sağlam belgeyi de kaybediyor, üstelik HANGİSİNİN bozuk olduğu
            // söylenmiyordu (yalnız `reason` gösteriliyordu, `path` değil).
            // Eklemede de aynı: hiçbiri taranamadıysa hata yolu eklenemeyenleri adlandırır.
            if (result.errors.length && !result.sources.length)
                throw new Error(result.errors.map(e => `${baseNameOf(e.path)}: ${e.reason}`).join('\n'));
            const base = mode === 'append' ? sourcesRef.current : [];
            const { next, skipped } = appendSources(base, result.sources);
            const failed = result.errors.map(e => baseNameOf(e.path));
            if (mode === 'append') {
                // Ekleme açma değildir: açık belge yerinde durur, söylenen
                // yalnız eklenen ve eklenemeyen dosyalardır.
                const added = next.length - base.length;
                if (added > 0) {
                    setSources(next);
                    build(next);
                    // Yeni eklenen belge imzalıysa onay o belge için yeniden istenir.
                    if (next.slice(base.length).some(s => s.is_signed))
                        setApproved(false);
                }
                const { text, tone } = appendNote(added, skipped, failed);
                setStatus(text); setStatusTone(tone);
                announce(text || (added > 1 ? `${added} belge eklendi.` : 'Belge eklendi.'));
                return;
            }
            setSources(next);
            build(next);
            setApproved(false);
            // Belge ekranda: "açıldı" diyen yeşil bir kutu yeni bir bilgi taşımaz.
            // Ekran okuyucu için sonuç yine bildirilir.
            const repeated = skipped.length ? appendNote(0, skipped, []).text : '';
            if (result.errors.length) {
                const message = [`${next.length} belge açıldı. ${result.errors.length} dosya açılamadı: ${failed.join(', ')}`, repeated].filter(Boolean).join(' ');
                setStatus(message); setStatusTone('error');
                announce(message);
            } else {
                const message = [note, repeated].filter(Boolean).join(' ');
                setStatus(message); setStatusTone('info');
                announce(next.length > 1 ? 'Belgeler açıldı.' : 'Belge açıldı.');
            }
            setDocState('ready');
        }
        catch (e) {
            if (generation !== loadGeneration.current)
                return;
            // Kullanıcıya kategorisinin tek cümlesi gider; teknik metin yalnız
            // geliştirme derlemesinde, üretimde redakte edilmiş tek satır olarak.
            logFailure('duzenek open failure', e);
            const detail = describeOpenFailure(e);
            // Açık bir oturum sırasında yapılan ekleme başarısız olursa oturum ayakta
            // kalır; kullanıcı panelden yeniden deneyebilir. Kabuğun verdiği belge
            // açılamadıysa oturum hiç kurulmamıştır.
            setDocState(previous => previous === 'ready' ? previous : 'failed');
            // Ekleme hatası açık belgenin değil, seçilen dosyanındır: "Belge
            // açılamadı" demek açık belgenin bozulduğu izlenimini verirdi.
            const message = mode === 'append' ? appendNote(0, [], paths.map(baseNameOf), detail).text : `${OPEN_FAILURE_TITLE}. ${detail}`;
            if (mode === 'replace')
                setFailure(detail);
            setStatus(message); setStatusTone('error');
            if (mode === 'append')
                announce(message);
        }
        finally {
            if (generation === loadGeneration.current)
                setBusy(false);
        }
    };
    // Seçici VAZGEÇMEDE null döner (sessizlik doğrudur) ama BAŞARISIZLIKTA
    // reddeder. `.catch(() => null)` ikisini aynı şeye indiriyordu: gerçek bir
    // hata da "vazgeçildi" gibi görünüyor, düğmeye basan kullanıcı hiçbir şey
    // olmadığını görüp nedenini öğrenemiyordu. İkisi artık ayrı.
    const choose = async () => {
        // Birleştir'de açık bir belge varken seçilenler ONA eklenir; seçici
        // başlığı tabanı adlandırır ki kullanıcı açık belgeyi yeniden aramasın.
        const appending = kind === 'merge' && sources.length > 0;
        let paths: string | string[] | null;
        try {
            paths = await open({ multiple: kind === 'merge' || kind === 'images', title: appending ? `${sources[0].file_name} ile birleştirilecek PDF’ler` : undefined, filters: [{ name: kind === 'images' ? 'Görseller' : 'PDF', extensions: kind === 'images' ? ['jpg', 'jpeg', 'png', 'tif', 'tiff', 'heic'] : ['pdf'] }] });
        } catch (e) {
            logFailure('duzenek picker failure', e);
            setStatus('Dosya seçici açılamadı. Lütfen yeniden deneyin.'); setStatusTone('error');
            announce('Dosya seçici açılamadı.');
            return;
        }
        if (!paths)
            return;
        const picked = Array.isArray(paths) ? paths : [paths];
        await load(picked, appending ? 'append' : 'replace');
    };
    // Açma başarısız olduğunda tek kurtarma yolu. Seçim kabuğa bildirilir ki
    // bardaki belge adı, son kullanılanlar ve çalışma alanı aynı belgeyi
    // göstersin; kabuk yoksa belge burada açılır.
    const openAnother = async () => {
        let picked: string | string[] | null;
        try {
            picked = await open({ multiple: false, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
        } catch (e) {
            logFailure('duzenek picker failure', e);
            setStatus('Dosya seçici açılamadı. Lütfen yeniden deneyin.'); setStatusTone('error');
            announce('Dosya seçici açılamadı.');
            return;
        }
        if (typeof picked !== 'string')
            return;
        if (onOpenDocument)
            onOpenDocument([picked]);
        else
            await load([picked]);
    };
    // Kabuk belgeyi belge yüzeyinde açtırdı; aynı yolları burada tara.
    const opened = initialPaths?.join('\u0000') ?? '';
    useEffect(() => { if (opened) void load(opened.split('\u0000')); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [opened]);
    // Sırala: her taşıma Geri Al geçmişine girer ve ekran okuyucuya söylenir.
    // Taşınan ilk sayfa şeritte görünür kalır: "Başa"/"Sona" ile giden sayfalar
    // şeridin görünmeyen ucuna gidiyor, kullanıcı sonucu göremiyordu.
    const revealPage = useRef('');
    const applyMove = (next: Page[], moved: Page[], how: MoveKind) => {
        if (!moved.length || sameOrder(next, order)) return;
        setHistory(previous => pushHistory(previous, order));
        setOrder(next);
        revealPage.current = moved[0].key;
        announce(describeMove(moved, how, next.findIndex(p => p.key === moved[0].key) + 1));
    };
    const shiftPage = (index: number, delta: -1 | 1) => applyMove(shiftKeys(order, [order[index].key], delta), [order[index]], delta < 0 ? 'up' : 'down');
    const selectedInOrder = order.filter(p => selected.includes(p.key));
    const movedSelection = (how: Exclude<MoveKind, 'drop'>) => {
        const keys = selectedInOrder.map(p => p.key);
        return how === 'start' ? moveKeys(order, keys, 0)
            : how === 'end' ? moveKeys(order, keys, order.length)
            : shiftKeys(order, keys, how === 'up' ? -1 : 1);
    };
    const canMove = (how: Exclude<MoveKind, 'drop'>) => !busy && selectedInOrder.length > 0 && !sameOrder(movedSelection(how), order);
    const moveSelection = (how: Exclude<MoveKind, 'drop'>) => { if (canMove(how)) applyMove(movedSelection(how), selectedInOrder, how); };
    const undo = () => {
        if (busy || !history.length) return;
        setOrder(history[history.length - 1]);
        setHistory(history.slice(0, -1));
        announce('Son taşıma geri alındı.');
    };
    // ⌘Z / Ctrl+Z: sistemin öğrettiği anlamıyla, yalnız Sırala'da ve geri
    // alınacak taşıma varken. Kısayol Geri Al düğmesinin ipucunda yazılı.
    useShortcuts(useMemo(() => [{ key: 'z', enabled: !busy && history.length > 0, run: () => { if (!document.querySelector('.sheet-backdrop')) undo(); } }],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [busy, history]), kind === 'reorder');
    // Sırala'da tıklama masaüstü listesi gibi seçer: düz tıklama tek sayfa,
    // Shift aralık, ⌘ (macOS) / Ctrl (Windows) ekle-çıkar. Klavyeyle
    // etkinleştirme (Enter/Boşluk, `detail === 0`) ve diğer araçlarda tıklama
    // yalnız önizlenen sayfayı değiştirir; klavyenin seçimi Seç kutularındadır.
    const viewPage = (event: React.MouseEvent, key: string) => {
        if (kind === 'reorder') {
            const next = selectPages(order, selected, anchor, key, { range: event.shiftKey, toggle: isToggleChord(event), keyboard: event.detail === 0 });
            setSelected(next.selected);
            setAnchor(next.anchor);
        }
        setCurrent(key);
    };
    const stripRef = useRef<HTMLElement>(null);
    // Sayfanın kendi ↑/↓ düğmesiyle taşıyan klavye kullanıcısının odağı sayfayla
    // gider: aynı okta kalır. React sayfayı listede yeniden yerleştirince odak
    // belgenin başına düşüyordu; her adımda yerini yeniden aramak gerekiyordu.
    const refocus = useRef<{ key: string; delta: -1 | 1 } | null>(null);
    useEffect(() => {
        const want = refocus.current, shown = revealPage.current;
        refocus.current = null;
        revealPage.current = '';
        const item = (key: string) => stripRef.current?.querySelector('[role="list"]')?.children[order.findIndex(p => p.key === key)];
        if (shown) item(shown)?.scrollIntoView?.({ block: 'nearest' });
        if (!want) return;
        const target = item(want.key)?.querySelectorAll<HTMLButtonElement>('.thumbnail-step button')[want.delta < 0 ? 0 : 1];
        if (target && document.activeElement !== target) target.focus();
    }, [order]);
    const shiftFromStrip = (item: Page, index: number, delta: -1 | 1) => {
        // Fareyle tıklanan düğme (macOS'ta odak almaz) için odak taşınmaz.
        if (stripRef.current?.contains(document.activeElement)) refocus.current = { key: item.key, delta };
        shiftPage(index, delta);
    };
    // Sırala'da şerit TEK sekme durağıdır (roving tabindex). Her sayfa dört
    // denetim taşır (görüntüle, Seç, ↑, ↓); hepsi sekme sırasındayken 60
    // sayfalık bir ekte 3. sayfayı seçip "Sona"ya ulaşmak ~228 Tab istiyordu.
    // Yalnız son odaklanan (yoksa önizlenen) sayfanın denetimleri sekme
    // sırasındadır; ↑/↓ komşu sayfanın AYNI denetimine, Home/End uçlara gider.
    // Taşıma düğmeleri ve Geri Al şeritten birkaç Tab ötededir. Öteki
    // araçlarda şerit değişmedi.
    const [roving, setRoving] = useState('');
    const rovingKey = order.some(p => p.key === roving) ? roving : order.some(p => p.key === current) ? current : order[0]?.key;
    const pageAt = (target: EventTarget) => {
        const list = stripRef.current?.querySelector('[role="list"]');
        const item = (target as HTMLElement).closest?.('[role="listitem"]');
        return list && item ? Array.prototype.indexOf.call(list.children, item) as number : -1;
    };
    const stripFocus = (event: React.FocusEvent) => { const index = pageAt(event.target); if (order[index]) setRoving(order[index].key); };
    const stripKeys = (event: React.KeyboardEvent<HTMLElement>) => {
        if (kind !== 'reorder' || event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return;
        const index = pageAt(event.target);
        const next = index < 0 ? null : stripStep(event.key, index, order.length);
        if (next === null) return;
        event.preventDefault();
        const controls = (at: number) => Array.from(stripRef.current?.querySelector('[role="list"]')?.children[at]?.querySelectorAll<HTMLElement>('button, input') ?? []);
        const slot = controls(index).indexOf(event.target as HTMLElement);
        controls(next)[Math.max(0, slot)]?.focus();
    };
    const strip = useStripDrag({
        listRef: stripRef,
        enabled: kind === 'reorder' && !busy,
        begin: key => {
            if (selected.includes(key)) return selectedInOrder.map(p => p.key);
            setSelected([key]);
            setAnchor(key);
            return [key];
        },
        changes: (keys, gap) => !sameOrder(moveKeys(order, keys, gap), order),
        onDrop: (keys, gap) => applyMove(moveKeys(order, keys, gap), order.filter(p => keys.includes(p.key)), 'drop'),
        // Belge Birleştir'de açılır; sayfayı sürükleyen kullanıcıya sıralamanın
        // nerede olduğu söylenir — yalnız Sırala açık sayfaları aynen tutacaksa
        // (bkz. dragHint). Ekrandaki bir sonucun üstüne yazılmaz.
        onRefused: () => {
            const hint = dragHint(kind, sources.length);
            if (!hint || busy || status) return;
            setStatus(hint); setStatusTone('info');
            announce(hint);
        },
    });
    // Liste değişince son ekleme/kaydetme notu artık bu listeyi anlatmaz
    // ("A.pdf zaten listede" dendikten sonra A çıkarılmış olabilir): susar.
    const relist = (next: SourceFile[]) => { setSources(next); build(next); setStatus(''); setStatusTone('info'); };
    const moveSource = (index: number, delta: -1 | 1) => relist(shiftSource(sources, index, delta));
    // Yanlış eklenen belgeyi geri almanın yolu. Kaynak dosyaya dokunulmaz.
    const dropSource = (index: number) => relist(removeSource(sources, index));
    const signature = editSignature(order, rotations, baseline);
    const dirty = hasUnsavedEdits(signature, savedSignature);
    useEffect(() => { onDirtyChange?.(dirty); }, [dirty, onDirtyChange]);
    useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);
    const toggle = (key: string) => { setSelected(prev => prev.includes(key) ? prev.filter(p => p !== key) : [...prev, key]); setAnchor(key); };
    const selectedPages = order.filter(p => selected.includes(p.key)).map(p => p.page);
    const active = order.find(p => p.key === current) || order[0];
    const outputCount = kind === 'select' ? selected.length : kind === 'delete' ? order.length - selected.length : order.length;
    const run = async (folderOnly = false) => {
        try {
            const operation: Record<string, unknown> = { kind };
            if (['select', 'delete'].includes(kind))
                operation.pages = selectedPages;
            if (kind === 'reorder')
                operation.pages = order.map(p => p.page);
            if (kind === 'rotate') {
                operation.kind = 'rotate_pages';
                operation.rotations = order.filter(p => rotations[p.key]).map(p => ({ page: p.page, degrees: rotations[p.key] }));
            }
            if (kind === 'crop')
                operation.margin_pt = margin * 72 / 25.4;
            if (kind === 'compress')
                operation.level = level;
            if (kind === 'watermark')
                operation.text = text;
            if (kind === 'number')
                operation.start = start;
            if (kind === 'raster') {
                // Pencere belgenin klasöründe açılır (bkz. shared-ui/savePlace).
                const outputDir = await open({ directory: true, multiple: false, title: 'Görsel paketinin kaydedileceği klasör', defaultPath: (await startFolder(sources[0]?.path)) ?? undefined });
                if (typeof outputDir !== 'string')
                    return;
                setBusy(true);
                setStatus('Görseller hazırlanıyor…'); setStatusTone('busy');
                const result = await invoke<string>('duzenek_pdf_to_images', { path: sources[0].path, outputDir, format: imageFormat, dpi: 150, approved });
                // Paket bir klasördür: adı "Klasör" satırında, tam yolu ipucunda.
                const done = 'Görsel paketi kaydedildi.';
                setStatus(done); setStatusTone('success');
                setSaved({ text: done, path: result, isFolder: true });
                return;
            }
            const outputPath = await copyDestination(`GolgeDosya-${kind}`, folderOnly, sources[0]?.path);
            if (!outputPath)
                return;
            setBusy(true);
            setStatus('PDF hazırlanıyor…'); setStatusTone('busy');
            const outcome = await invoke<{ status: 'published' | 'compressed' | 'no_benefit' | 'failed'; reason?: string; source_bytes?: number; body_bytes?: number; output_bytes?: number; candidate_bytes?: number; images_found?: number; images_recompressed?: number; previously_processed?: boolean }>('duzenek_run_pdf_tool', { paths: sources.map(s => s.path), operation, outputPath, approved });
            if (outcome.status === 'failed') throw new Error(outcome.reason);
            if (outcome.status === 'no_benefit') {
                // Gerçek bayt sonucu söylenir; motorun bulamadığı küçülmeden
                // belgenin "zaten optimize" olduğu sonucu çıkarılmaz. Yeniden
                // kodlanmayan görselin SEBEBİ de iddia edilmez: kapsam dışı mı,
                // yoksa yeniden kodlayınca küçülmedi mi, sonuç bunu taşımaz.
                setStatus(noBenefitStatus(outcome));
                setStatusTone('info');
                return;
            }
            // A receipt is derived from the published file, not just a resolved command promise.
            const receipt = await invoke<ScanBatchResult>('duzenek_scan_source_files', { paths: [outputPath] });
            if (receipt.errors.length || receipt.sources.length !== 1 || receipt.sources[0].size_bytes === 0)
                throw new Error('Kaydedilen PDF yeniden doğrulanamadı.');
            const done = savedStatus({
                kind,
                outputPath,
                pageCount: receipt.sources[0].page_count,
                outputBytes: receipt.sources[0].size_bytes,
                sourceBytes: outcome.source_bytes || sources.reduce((n, s) => n + s.size_bytes, 0),
            });
            setStatus(done);
            setStatusTone('success');
            setSaved({ text: done, path: outputPath, isFolder: false });
            // Bu düzen artık bir kopyada: kip değiştirmek bir şey kaybettirmez.
            setSavedSignature(signature);
        }
        catch (e) {
            // Kaydetme yolu da açma yolu gibi: motorun cümlesi log'da kalır,
            // kullanıcı kategorisinin tek cümlesini görür. Bu yoldaki güvenlik
            // denetimleri (bütünlük, sayfa sayımı, boyut sınırı) kategori olarak
            // korunur; sınıf adları ve dosya yolları çıkmaz.
            logFailure('duzenek save failure', e);
            setStatus((kind === 'compress' ? 'Sıkıştırma tamamlanamadı. ' : 'İşlem tamamlanamadı. ') + describeSaveFailure(e));
            setStatusTone('error');
        }
        finally {
            setBusy(false);
        }
    };
    // Döndürme SEÇİME bağlı değildir; bu yüzden döndürme kipinde sayfa
    // kutusu da çizilmez. Eskiden kullanıcı önce kutuyu işaretlemek, sonra
    // yön seçmek zorundaydı ve birden fazla kutu işaretliyse hepsi dönüyordu.
    const pageSelection = ['select', 'delete'].includes(kind);
    const surfaces = workspaceSurfaces(docState, order.length);
    const cannotSave = busy || !sources.length || (sources.some(s => s.is_signed) && !approved) ||
        (['select', 'delete'].includes(kind) && !selected.length) || outputCount === 0 || (kind === 'rotate' && !Object.values(rotations).some(Boolean)) ||
        mergeShortfall(kind, sources.length) !== '';
    // Kaydet kapalıysa nedeni görünür: tek belgeyle Birleştir. Neden hem
    // panelde hem kaydetmenin yanındadır: panel dar pencerede kapalı başlar ve
    // varsayılan araç Birleştir'dir; düğmeler nedensiz sönük kalmamalı.
    const needMore = mergeShortfall(kind, sources.length);
    // İmzalı belgede kaydetme, Araçlar panelindeki onaya bağlıdır. Panel
    // kapalıyken düğmeler nedensiz sönük duruyordu; neden kaydetmenin yanında
    // söylenir ve tıklanınca panel açılıp onay kutusuna gidilir.
    const needsApproval = sources.some(s => s.is_signed) && !approved;
    const revealTools = useRevealInspector();
    const approvalRef = useRef<HTMLInputElement>(null);
    const showApproval = () => { revealTools(); requestAnimationFrame(() => approvalRef.current?.focus()); };
    const mergeAddRef = useRef<HTMLButtonElement>(null);
    const showMergeAdd = () => { revealTools(); requestAnimationFrame(() => mergeAddRef.current?.focus()); };
    const pick = (key: Kind) => {
        setKind(key);
        const kept = key === 'images' || kind === 'images' ? [] : (key === 'merge' ? sources : sources.slice(0, 1));
        // Araçlar BİLİNÇLİ olarak bağımsızdır: her biri kaynaktan tek bir işlem
        // uygulayıp kendi kopyasını üretir, düzenlemeler birikmez. Ama bu
        // sessizce oluyordu — kullanıcı üç belge açıp ya da sayfaları sıralayıp
        // araç değiştirince emeği uyarısız siliniyordu. Kayıp artık SÖYLENİYOR.
        const droppedDocs = sources.length - kept.length;
        const lostEdits = Object.values(rotations).some(Boolean) || isReordered(order, baseline);
        const notes = [
            droppedDocs > 0 && (kept.length
                ? `bu araç tek belgeyle çalışır, ${droppedDocs} belge kapatıldı`
                : `${droppedDocs} belge kapatıldı`),
            lostEdits && 'sayfa sırası ve dönüşler sıfırlandı',
        ].filter(Boolean) as string[];
        setSources(kept);
        build(kept);
        // `build` ilk sayfayı işaretler; Sil aracında bu, sayfa 1'i daha
        // kullanıcı bir şey yapmadan çıkarılacak sayfa yapıyordu ve kaydet
        // düğmesini açıyordu. Silme her zaman boş bir seçimle başlar.
        if (key === 'delete')
            setSelected([]);
        const message = notes.length ? `${tools[key][0]} aracına geçildi — ${notes.join('; ')}.` : '';
        setStatus(message);
        setStatusTone('info');
        if (message)
            announce(message);
        setApproved(false);
        // Görseller → PDF görsellerle çalışır ve açık belgeyi bırakır. PDF
        // aracına dönülünce oturum boş kalıyordu, bar ise hâlâ açık belgeyi
        // gösteriyordu; Birleştir'de eklenenler o belgesiz birleşiyordu.
        // Kabuğun açık belgesi yeniden taban olur.
        if (kind === 'images' && key !== 'images' && initialPaths?.length)
            void load(initialPaths, 'replace', message);
    };
    const PAGE_TOOLS: Kind[] = ['select', 'reorder', 'delete', 'rotate'];
    const settingField = kind === 'compress'
        ? <label>Görsel kalitesi<select value={level} onChange={e => setLevel(e.target.value)}>{COMPRESSION_PRESETS.map(p => <option key={p.value} value={p.value} title={p.detail}>{p.label}</option>)}</select></label>
        : kind === 'crop'
        ? <label>Her kenardan kırpılacak mesafe (mm)<input type="number" min={0} value={margin} onChange={e => setMargin(Number(e.target.value))}/></label>
        : kind === 'watermark'
        ? <label>Filigran (en fazla 60 karakter)<input type="text" maxLength={60} value={text} onChange={e => setText(e.target.value)}/></label>
        : kind === 'raster'
        ? <label>Görsel biçimi (150 DPI)<select value={imageFormat} onChange={e => setImageFormat(e.target.value === 'jpg' ? 'jpg' : 'png')}><option value="png">PNG</option><option value="jpg">JPG</option></select></label>
        : kind === 'number'
        ? <label>İlk sayfa numarası<input type="number" min={1} value={start} onChange={e => setStart(Number(e.target.value))}/></label>
        : null;

    return <section className="pdf-root" aria-busy={busy}>
        {/* Barın ortası: açık belgenin ölçüsü. "8 sayfa · 2 işaretli" —
            kullanıcının "şu an neye bakıyorum" sorusunun ikinci yarısı. */}
        {order.length > 0 && <ToolbarStatus>
            {order.length} sayfa{pageSelection && selected.length > 0 ? ` · ${selected.length} işaretli` : ''}{kind === 'reorder' && selected.length > 0 ? ` · ${selected.length} seçili` : ''}
        </ToolbarStatus>}

        {surfaces.tools && <ToolbarActions>
            {needsApproval && !busy && <Button variant="quiet" className="save-reason" onClick={showApproval}>İmza Onayı Gerekli</Button>}
            {needMore && !busy && <Button variant="quiet" className="save-reason" title={needMore} onClick={showMergeAdd}>En Az İki Belge Gerekli</Button>}
            {kind !== 'raster' && <Button disabled={cannotSave} onClick={() => run(true)}>Klasör Seçerek Kaydet</Button>}
            <Button variant="primary" disabled={cannotSave} onClick={() => run()}>
                {busy ? 'PDF hazırlanıyor…' : kind === 'raster' ? 'Görsel Paketi Kaydet' : 'Yeni PDF Kaydet'}
            </Button>
        </ToolbarActions>}

        {surfaces.tools && <InspectorPanel title="Araçlar" scope="pdf-root" kind="tools">
            <div className="pdf-controls" aria-label="PDF işlem kontrolleri">
                <ToolGroups kind={kind} busy={busy} onPick={pick}/>

                <InspectorSection title="Seçili araç">
                    <p className="tool-name">{tools[kind][0]}</p>
                    <p className="tool-hint">{tools[kind][1]}</p>
                    {/* Araç ne yaptığını söylüyordu ama NEYİ etkilediğini ve
                        ÇIKTININ ne olacağını söylemiyordu; kullanıcı bunu ancak
                        deneyerek öğreniyordu. İki kısa satır, dokümantasyon
                        değil. */}
                    <dl className="tool-facts">
                        <dt>Etkilenen</dt>
                        <dd>{TOOL_FACTS[kind].affects}</dd>
                        <dt>Çıktı</dt>
                        <dd>{TOOL_FACTS[kind].output}</dd>
                    </dl>
                    {settingField}
                    {/* "Belge Ekle" bağlamsızdı: hangi işe belge eklendiği
                        araca göre değişir. */}
                    {(kind === 'merge' || kind === 'images') &&
                        <Button ref={mergeAddRef} onClick={choose} disabled={busy}>
                            {kind === 'merge' ? 'Birleştirilecek Belge Ekle' : 'Görsel Ekle'}
                        </Button>}
                    {needMore && <p className="tool-hint">{needMore}</p>}
                    {sources.length > 0 && <ol className="source-list">{sources.map((s, i) => <li key={s.path}>
                        <span>{sourceLine(s)}</span>
                        {sources.length > 1 && <div className="source-actions">
                            <IconButton label={`${i + 1}. belgeyi yukarı taşı`} disabled={busy || i === 0} onClick={() => moveSource(i, -1)}>
                                <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 12.5V4M4.5 7.5 8 4l3.5 3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                            </IconButton>
                            <IconButton label={`${i + 1}. belgeyi aşağı taşı`} disabled={busy || i === sources.length - 1} onClick={() => moveSource(i, 1)}>
                                <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 3.5V12m-3.5-3.5L8 12l3.5-3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                            </IconButton>
                            <IconButton label={`${i + 1}. belgeyi listeden çıkar`} disabled={busy} onClick={() => dropSource(i)}>
                                <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M4.5 4.5l7 7m0-7-7 7" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/></svg>
                            </IconButton>
                        </div>}
                    </li>)}</ol>}
                    {order.length > 0 && <>
                        <p className="tool-count">{order.length} kaynak sayfası{pageSelection ? ` · ${selected.length} işaretli` : ''} · Çıktı: {outputCount} sayfa</p>
                        {pageSelection && <div className="row">
                            <Button className="btn-sm" disabled={busy} onClick={() => setSelected(order.map(p => p.key))}>Tümünü İşaretle</Button>
                            <Button className="btn-sm" variant="quiet" disabled={busy} onClick={() => setSelected([])}>Seçimi Temizle</Button>
                        </div>}
                    </>}
                    {sources.some(s => s.is_signed) && <label className="approval"><input ref={approvalRef} type="checkbox" checked={approved} onChange={e => setApproved(e.target.checked)}/>İmza işareti bulundu. Türetilmiş PDF kaynak elektronik imzanın doğrulanabilirliğini taşımaz; onaylıyorum.</label>}
                    <p className="tool-safe">Yeni bir kopya oluşturulur; kaynak belgeleriniz korunur.</p>
                </InspectorSection>
            </div>
        </InspectorPanel>}

        {/* Şerit yalnız gösterilecek sayfa varken açılır; belge yokken 132 px'lik
            boş bir bant çizmez ve hata/boş durum tam genişliği alır. */}
        <div className="pdf-workspace-layout" data-pages={surfaces.strip}>
            {surfaces.strip && <aside className="thumbnail-list" aria-label="Sayfa önizlemeleri" ref={stripRef} data-reorder={kind === 'reorder' || undefined} data-dragging={strip.drag ? '' : undefined} onClickCapture={strip.onClickCapture} onKeyDown={stripKeys} onFocus={stripFocus}>
                <div role="list">{order.map((item, index) => {
                    const gap = strip.drag?.gap;
                    const drop = gap === index ? 'before' : gap === order.length && index === order.length - 1 ? 'after' : undefined;
                    return <PageThumbnail key={item.key} item={item} index={index} count={order.length} kind={kind} isCurrent={item.key === current} isSelected={selected.includes(item.key)}
                        rotation={rotations[item.key] || 0} busy={busy} drop={gap == null ? undefined : drop} dragging={strip.drag?.keys.includes(item.key)} tabbable={kind !== 'reorder' || item.key === rovingKey}
                        onView={event => viewPage(event, item.key)} onToggle={() => toggle(item.key)} onShift={delta => shiftFromStrip(item, index, delta)}
                        onPointerDown={event => strip.onPointerDown(event, item.key)}/>;
                })}</div>
                {kind === 'reorder' && <p id={STRIP_KEYS_ID} className="sr-only">Sayfalar arasında ↑ ve ↓ tuşlarıyla geçilir; Home ilk, End son sayfaya götürür.</p>}
            </aside>}

            <aside className="pdf-preview-panel" aria-label="PDF önizleme çalışma alanı">
                {/* Durum satırı HER durumda çizilir. Eskiden yalnız
                    `surfaces.strip && kind !== 'images'` dalının içindeydi:
                    "Görseller → PDF" aracında kaydetme sonucu (başarı, yol ya da
                    hata) HİÇ görünmüyordu ve belge açılamadığında seçici hatası
                    da kayboluyordu — kullanıcı düğmeye basıp hiçbir şey
                    olmadığını sanıyordu. */}
                <Status tone={statusTone}>{status}</Status>
                {saved && statusTone === 'success' && saved.text === status && <SavedPlace path={saved.path} isFolder={saved.isFolder}/>}
                {surfaces.strip && kind !== 'images' ? <>
                <div className="pdf-strip">
                    <div className="tool-segment" role="group" aria-label="Sayfa araçları">
                        {PAGE_TOOLS.map(key => <button key={key} type="button" data-tool={key} className={`segment ${kind === key ? 'is-current' : ''}`} title={tools[key][1]} aria-label={tools[key][0]} aria-pressed={kind === key} disabled={busy} onClick={() => pick(key)}>{SHORT[key]}</button>)}
                    </div>
                    {/* TEK kontrol, TEK yön: her tıklama +90°. İki tıklama 180°,
                        üç tıklama 270°, dört tıklama başa döner. Kapsam
                        ÖNİZLENEN sayfadır — şeritteki çoklu seçim değil. */}
                    {kind === 'rotate' && <div className="row">
                        <Button className="btn-sm" disabled={busy || !active} onClick={() => active && setRotations(previous => rotatePages(previous, [active.key], 90))} title="Önizlenen sayfayı 90° döndür" aria-label={active ? `${order.findIndex(p => p.key === active.key) + 1}. sayfayı 90° döndür` : 'Döndür'}>↷ Döndür</Button>
                    </div>}
                    {/* Sırala: seçili sayfaları taşıyan görünür denetimler — fare
                        gerekmez. Seçim şeritteki Seç kutularıyla ya da tıklamayla
                        (Shift aralık, ⌘/Ctrl ekle-çıkar) yapılır. Kullanılamayan
                        düğme `disabled` değil `aria-disabled`: odakta kalır.
                        Yoksa "Başa"ya basan klavye kullanıcısı, sayfalar başa
                        varınca sönen düğmeyle birlikte odağını kaybederdi. */}
                    {kind === 'reorder' && <div className="row reorder-controls">
                        <div className="row" role="group" aria-label="Seçili sayfaları taşı">
                            <Button className="btn-sm" aria-disabled={!canMove('start') || undefined} title="Seçili sayfaları başa taşı" onClick={() => moveSelection('start')}>Başa</Button>
                            <Button className="btn-sm" aria-disabled={!canMove('up') || undefined} title="Seçili sayfaları birer sıra yukarı taşı" onClick={() => moveSelection('up')}>Yukarı</Button>
                            <Button className="btn-sm" aria-disabled={!canMove('down') || undefined} title="Seçili sayfaları birer sıra aşağı taşı" onClick={() => moveSelection('down')}>Aşağı</Button>
                            <Button className="btn-sm" aria-disabled={!canMove('end') || undefined} title="Seçili sayfaları sona taşı" onClick={() => moveSelection('end')}>Sona</Button>
                        </div>
                        <Button className="btn-sm" variant="quiet" aria-disabled={busy || !history.length || undefined} title={`Son taşımayı geri al  ${shortcutLabel('Z')}`} onClick={undo}>Geri Al</Button>
                    </div>}
                    <div className="toolbar-spacer"/>
                    <label className="zoom">Yakınlaştır<select value={zoom} onChange={e => setZoom(e.target.value.startsWith('fit-') ? e.target.value as PreviewMode : Number(e.target.value))}><option value="fit-page">Sayfaya sığdır</option><option value="fit-width">Genişliğe sığdır</option>{[75, 100, 125, 150, 200].map(z => <option key={z} value={z}>{z}%</option>)}</select></label>
                </div>
                <div className="pdf-canvas">
                    <p className="page-line">{active?.source.file_name} · Kaynak sayfa {active?.page} / {active?.source.page_count}{kind === 'delete' && selected.includes(active?.key) ? ' — Çıktıdan çıkarılacak' : ''}{kind === 'rotate' ? ` · Dönüş: ${rotations[active?.key] || 0}°` : ''}{pageSelection && selected.includes(active?.key) ? ' · İşaretli' : ''}{kind === 'reorder' && selected.includes(active?.key) ? ' · Seçili' : ''}</p>
                    <div className="pdf-page-viewport" data-fit={typeof zoom === 'string' ? '' : undefined} tabIndex={0} role="region" aria-label="Kaydırılabilir PDF sayfası">{active && <Preview key={active.key} item={active} large mode={zoom} rotation={rotations[active.key] || 0}/>}</div>
                    <p className="preview-note">Kaynak sayfanın önizlemesi. Yeni PDF’nin her sayfasının sağ alt köşesine, sayfa boyutu değişmeden, saydam GölgeDosya işareti eklenir.</p>
                </div>
            </> : <PreviewPlaceholder state={docState} detail={failure} onOpenAnother={openAnother} forImages={kind === 'images'}/>}</aside>
        </div>
    </section>;
};
