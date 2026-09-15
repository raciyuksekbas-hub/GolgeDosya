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
import { rotatePages, previewGeometry, workspaceSurfaces, type DocumentState, type PreviewMode } from './pdfWorkspaceState';
import { copyDestination } from './copyDestination';
import { describeOpenFailure, describeSaveFailure, logFailure, safeMessage, OPEN_FAILURE_FALLBACK, OPEN_FAILURE_TITLE } from '../../shared-ui/failure';
import React, { useState, useEffect, useRef } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { ScanBatchResult, SourceFile } from './types';
import { InspectorPanel, InspectorSection, ToolbarActions, ToolbarStatus } from '../../shell/chrome';
import { Button, EmptyState, IconButton, Status } from '../../shared-ui/primitives';
import { announce } from '../../shared-ui/Announcer';
import './pdf.css';
const tools = {
    merge: ['Birleştir', 'Birden fazla PDF’yi seçtiğiniz sırayla tek dosyada birleştirir.'],
    select: ['Seçili sayfalar → yeni PDF', 'İşaretlediğiniz sayfalardan yeni bir PDF kopyası oluşturur.'],
    reorder: ['Sayfa sırasını değiştir', 'Sayfaları görsel olarak yeniden sıralayıp yeni bir kopya oluşturur.'],
    delete: ['Sayfa sil', 'İşaretlediğiniz sayfaları çıkararak yeni bir PDF oluşturur.'],
    rotate: ['Döndür', 'İşaretlediğiniz sayfaları 90° adımlarla döndürür.'],
    compress: ['Sıkıştır', 'Kalite kontrolünü geçen ve en az %3 küçülen PDF kopyasını kaydeder.'],
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
    { title: 'Diğer', keys: ['crop', 'watermark', 'number', 'images'], collapsed: true },
];

type Page = {
    source: SourceFile;
    page: number;
    key: string;
};
// Bound native render requests; thumbnails render only when near the viewport.
let queue: Promise<unknown> = Promise.resolve();
function Preview({ item, large = false, rotation = 0, mode = 'fit-page' }: {
    item: Page; large?: boolean; rotation?: number; mode?: PreviewMode;
}) {
    const [bitmap, setBitmap] = useState<{url: string; width: number; height: number} | null>(null);
    const [error, setError] = useState('');
    const deferredUrls = useRef(new Set<string>()).current;
    const [viewport, setViewport] = useState({width: 100, height: 130});
    const ref = useRef<HTMLDivElement>(null);
    const geometry = bitmap ? previewGeometry(bitmap.width, bitmap.height, rotation, viewport.width, viewport.height, large ? mode : 'fit-page') : null;
    const dpi = large ? Math.min(300, Math.max(96, Math.ceil(96 * (geometry?.scale || 1) * (typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1)))) : 40;
    useEffect(() => {
        const target = large ? ref.current?.parentElement : ref.current;
        if (!target) return;
        const observer = new ResizeObserver(() => setViewport({width: target.clientWidth, height: target.clientHeight}));
        observer.observe(target);
        setViewport({width: target.clientWidth, height: target.clientHeight});
        return () => observer.disconnect();
    }, [large]);
    useEffect(() => {
        let alive = true, started = false;
        const urls: string[] = [];
        const load = () => {
            if (started) return;
            started = true;
            queue = queue.catch(() => {}).then(async () => {
                if (!alive) return;
                try {
                    // Rotation is visual and immediate; both views share the same unrotated bitmap geometry.
                    const bytes = await invoke<number[]>('duzenek_preview_pdf_page', {path: item.source.path, page: item.page, dpi, rotation: 0});
                    if (!alive) return;
                    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], {type: 'image/png'}));
                    urls.push(url);
                    const img = new Image(); img.src = url; await img.decode();
                    if (alive) { setBitmap(previous => ({url, width: previous?.width || img.naturalWidth * 96 / dpi, height: previous?.height || img.naturalHeight * 96 / dpi})); setError(''); }
                } catch (e) { if (alive) setError(String(e)); }
            });
        };
        const observer = new IntersectionObserver(entries => { if (entries.some(e => e.isIntersecting)) load(); }, {rootMargin: '100px'});
        if (ref.current) observer.observe(ref.current);
        return () => { alive = false; observer.disconnect(); /* URL remains visible while a sharper bitmap loads. */ urls.forEach(url => deferredUrls.add(url)); };
    }, [item.key, dpi]);
    useEffect(() => () => { deferredUrls.forEach(url => URL.revokeObjectURL(url)); deferredUrls.clear(); }, []);
    // Bekleme, dev bir kutunun ortasındaki tek kelime değil: küçük bir döner
    // ve kısa bir etiket. Hata olursa kategorisinin tek cümlesi gösterilir;
    // ham motor metni kullanıcı yüzeyine çıkmaz.
    const wait = error
        ? <p className="pdf-wait" data-tone="error" role="status">{safeMessage(error, 'Bu sayfanın önizlemesi oluşturulamadı.')}</p>
        : <p className="pdf-wait" role="status"><span className="spinner" aria-hidden="true"/>Önizleme hazırlanıyor</p>;
    return <div ref={ref} className={large ? 'pdf-preview-stage' : 'pdf-thumbnail-stage'} style={large && geometry ? {width: Math.max(viewport.width, geometry.width + 16), minHeight: Math.max(viewport.height, geometry.height + 16)} : undefined}>
        {bitmap && geometry ? <div className="pdf-page-surface" style={{width: geometry.width, height: geometry.height}}><img src={bitmap.url} alt={`${item.source.file_name}, sayfa ${item.page}`} style={{width: geometry.imageWidth, height: geometry.imageHeight, transform: `translate(-50%, -50%) rotate(${rotation}deg)`}}/></div> : wait}
        {bitmap && error && wait}
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
export function PreviewPlaceholder({ state, detail, onOpenAnother }: {
    state: DocumentState; detail: string; onOpenAnother: () => void;
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
    return <EmptyState title="Belge önizlemesi" note="PDF seçtiğinizde sayfaları burada göreceksiniz." />;
}

export const PdfWorkspace: React.FC<{ paths?: string[]; onOpenDocument?: (paths: string[]) => void }> = ({ paths: initialPaths, onOpenDocument }) => {
    const [kind, setKind] = useState<Kind>('merge'), [sources, setSources] = useState<SourceFile[]>([]), [order, setOrder] = useState<Page[]>([]), [selected, setSelected] = useState<string[]>([]), [current, setCurrent] = useState(''), [zoom, setZoom] = useState<PreviewMode>('fit-page');
    const [rotations, setRotations] = useState<Record<string, number>>({});
    // Araç değiştirirken neyin kaybolacağını söyleyebilmek için: sıra elle değişti mi?
    const [reordered, setReordered] = useState(false);
    // "Sayfa yok" ile "belge açılamadı" aynı şey değildir: araç paneli yalnız
    // açık bir belge oturumunda çizilir, sayfa şeridi ise gerçekten sayfa varken.
    const [docState, setDocState] = useState<DocumentState>(initialPaths?.length ? 'loading' : 'none');
    const [failure, setFailure] = useState('');
    const [margin, setMargin] = useState(10), [text, setText] = useState('KOPYA'), [imageFormat, setImageFormat] = useState('png'), [start, setStart] = useState(1), [level, setLevel] = useState('balanced_compression'), [approved, setApproved] = useState(false), [busy, setBusy] = useState(false), [status, setStatus] = useState('');
    const build = (list: SourceFile[]) => { setRotations({}); setReordered(false); const next = list.flatMap(source => Array.from({ length: source.page_count }, (_, i) => ({ source, page: i + 1, key: source.path + '#' + (i + 1) }))); setOrder(next); setCurrent(next[0]?.key || ''); setSelected(next.length ? [next[0].key] : []); };
    // Tarama gövdesi, kendi seçicisi ile kabuğun açtığı belgeler arasında ortak.
    const load = async (paths: string[]) => { try {
        if (!paths.length)
            return;
        setBusy(true);
        setDocState(previous => previous === 'ready' ? previous : 'loading');
        setFailure('');
        setStatus('Belgeler inceleniyor…');
        const result = await invoke<ScanBatchResult>('duzenek_scan_source_files', { paths });
        if (result.errors.length)
            throw new Error(result.errors.map(e => e.reason).join('\n'));
        setSources(result.sources);
        build(result.sources);
        setApproved(false);
        // Belge ekranda: "açıldı" diyen yeşil bir kutu yeni bir bilgi taşımaz.
        // Ekran okuyucu için sonuç yine bildirilir.
        setStatus('');
        announce(result.sources.length > 1 ? 'Belgeler açıldı.' : 'Belge açıldı.');
        setDocState('ready');
    }
    catch (e) {
        // Kullanıcıya kategorisinin tek cümlesi gider; teknik metin yalnız
        // geliştirme derlemesinde, üretimde redakte edilmiş tek satır olarak.
        logFailure('duzenek open failure', e);
        const detail = describeOpenFailure(e);
        setFailure(detail);
        setStatus(`${OPEN_FAILURE_TITLE}. ${detail}`);
        // Açık bir oturum sırasında yapılan ekleme başarısız olursa oturum ayakta
        // kalır; kullanıcı panelden yeniden deneyebilir. Kabuğun verdiği belge
        // açılamadıysa oturum hiç kurulmamıştır.
        setDocState(previous => previous === 'ready' ? previous : 'failed');
    }
    finally {
        setBusy(false);
    } };
    const choose = async () => {
        const paths = await open({ multiple: kind === 'merge' || kind === 'images', filters: [{ name: kind === 'images' ? 'Görseller' : 'PDF', extensions: kind === 'images' ? ['jpg', 'jpeg', 'png', 'tif', 'tiff', 'heic'] : ['pdf'] }] }).catch(() => null);
        if (!paths)
            return;
        await load(Array.isArray(paths) ? paths : [paths]);
    };
    // Açma başarısız olduğunda tek kurtarma yolu. Seçim kabuğa bildirilir ki
    // bardaki belge adı, son kullanılanlar ve çalışma alanı aynı belgeyi
    // göstersin; kabuk yoksa belge burada açılır.
    const openAnother = async () => {
        const picked = await open({ multiple: false, filters: [{ name: 'PDF', extensions: ['pdf'] }] }).catch(() => null);
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
    const shiftPage = (index: number, delta: number) => { const next = [...order]; [next[index], next[index + delta]] = [next[index + delta], next[index]]; setOrder(next); setReordered(true); };
    const moveSource = (index: number) => { const next = [...sources]; [next[index - 1], next[index]] = [next[index], next[index - 1]]; setSources(next); build(next); };
    const toggle = (key: string) => setSelected(prev => prev.includes(key) ? prev.filter(p => p !== key) : [...prev, key]);
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
                const outputDir = await open({ directory: true });
                if (typeof outputDir !== 'string')
                    return;
                setBusy(true);
                setStatus('Görseller hazırlanıyor…');
                const result = await invoke<string>('duzenek_pdf_to_images', { path: sources[0].path, outputDir, format: imageFormat, dpi: 150, approved });
                setStatus('Görsel paketi kaydedildi: ' + result);
                return;
            }
            const outputPath = await copyDestination(`GolgeDosya-${kind}`, folderOnly);
            if (!outputPath)
                return;
            setBusy(true);
            setStatus('PDF hazırlanıyor…');
            const outcome = await invoke<{ status: 'published' | 'compressed' | 'no_benefit' | 'failed'; reason?: string; source_bytes?: number; body_bytes?: number; output_bytes?: number; candidate_bytes?: number; images_found?: number; images_recompressed?: number }>('duzenek_run_pdf_tool', { paths: sources.map(s => s.path), operation, outputPath, approved });
            if (outcome.status === 'failed') throw new Error(outcome.reason);
            const metrics = `Sıkıştırılmış belge: ${((outcome.body_bytes || 0) / 1024).toFixed(1)} KB · Marka dahil: ${((outcome.output_bytes || outcome.candidate_bytes || 0) / 1024).toFixed(1)} KB`;
            if (outcome.status === 'no_benefit') {
                // Görsel var ama hiçbiri yeniden kodlanmadıysa belge "zaten optimize"
                // değildir; görseller güvenli sıkıştırma kapsamının dışındadır
                // (CMYK, maske, paletli). Kullanıcıya doğru sebep söylenir.
                const outOfScope = (outcome.images_found || 0) > 0 && !(outcome.images_recompressed || 0);
                const why = outOfScope
                    ? `${outcome.images_found} görsel bulundu; renk uzayı veya maskesi nedeniyle güvenle yeniden kodlanamadı. Anlamlı bir küçülme sağlanamadı. Çıktı kaydedilmedi.`
                    : 'Bu belge zaten yeterince optimize. Anlamlı bir küçülme sağlanamadı. Çıktı kaydedilmedi.';
                setStatus(`${((outcome.source_bytes || 0) / 1024).toFixed(1)} KB\n${why}\n${metrics}\nEn az %3 küçülme gerekir.`);
                return;
            }
            // A receipt is derived from the published file, not just a resolved command promise.
            const receipt = await invoke<ScanBatchResult>('duzenek_scan_source_files', { paths: [outputPath] });
            if (receipt.errors.length || receipt.sources.length !== 1 || receipt.sources[0].size_bytes === 0)
                throw new Error('Kaydedilen PDF yeniden doğrulanamadı.');
            const size = receipt.sources[0].size_bytes, before = sources.reduce((n, s) => n + s.size_bytes, 0);
            setStatus(`PDF kaydedildi ve yeniden açılarak doğrulandı: ${outputPath}\n${receipt.sources[0].page_count} sayfa · ${(size / 1024).toFixed(1)} KB` + (kind === 'compress' ? `\n${(before / 1024).toFixed(1)} KB → ${(size / 1024).toFixed(1)} KB · %${((1 - size / before) * 100).toFixed(1)} küçültüldü.\n${metrics}` : ''));
        }
        catch (e) {
            // Kaydetme yolu da açma yolu gibi: motorun cümlesi log'da kalır,
            // kullanıcı kategorisinin tek cümlesini görür. Bu yoldaki güvenlik
            // denetimleri (bütünlük, sayfa sayımı, boyut sınırı) kategori olarak
            // korunur; sınıf adları ve dosya yolları çıkmaz.
            logFailure('duzenek save failure', e);
            setStatus((kind === 'compress' ? 'Sıkıştırma tamamlanamadı. ' : 'İşlem tamamlanamadı. ') + describeSaveFailure(e));
        }
        finally {
            setBusy(false);
        }
    };
    const pageSelection = ['select', 'delete', 'rotate'].includes(kind);
    const surfaces = workspaceSurfaces(docState, order.length);
    const cannotSave = busy || !sources.length || (sources.some(s => s.is_signed) && !approved) ||
        (['select', 'delete'].includes(kind) && !selected.length) || outputCount === 0 || (kind === 'rotate' && !Object.values(rotations).some(Boolean));
    const pick = (key: Kind) => {
        setKind(key);
        const kept = key === 'images' || kind === 'images' ? [] : (key === 'merge' ? sources : sources.slice(0, 1));
        // Araçlar BİLİNÇLİ olarak bağımsızdır: her biri kaynaktan tek bir işlem
        // uygulayıp kendi kopyasını üretir, düzenlemeler birikmez. Ama bu
        // sessizce oluyordu — kullanıcı üç belge açıp ya da sayfaları sıralayıp
        // araç değiştirince emeği uyarısız siliniyordu. Kayıp artık SÖYLENİYOR.
        const droppedDocs = sources.length - kept.length;
        const lostEdits = Object.values(rotations).some(Boolean) || reordered;
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
        if (message)
            announce(message);
        setApproved(false);
    };
    const PAGE_TOOLS: Kind[] = ['select', 'reorder', 'delete', 'rotate'];
    const settingField = kind === 'compress'
        ? <label>Görsel kalitesi<select value={level} onChange={e => setLevel(e.target.value)}><option value="gentle_compression">Nazik — 2400 px / kalite 80</option><option value="balanced_compression">Dengeli — 2000 px / kalite 72</option><option value="aggressive_compression">Güçlü — 1600 px / kalite 65</option></select></label>
        : kind === 'crop'
        ? <label>Her kenardan kırpılacak mesafe (mm)<input type="number" min={0} value={margin} onChange={e => setMargin(Number(e.target.value))}/></label>
        : kind === 'watermark'
        ? <label>Filigran (en fazla 60 karakter)<input maxLength={60} value={text} onChange={e => setText(e.target.value)}/></label>
        : kind === 'raster'
        ? <label>Görsel biçimi (150 DPI)<select value={imageFormat} onChange={e => setImageFormat(e.target.value === 'jpg' ? 'jpg' : 'png')}><option value="png">PNG</option><option value="jpg">JPG</option></select></label>
        : kind === 'number'
        ? <label>İlk sayfa numarası<input type="number" min={1} value={start} onChange={e => setStart(Number(e.target.value))}/></label>
        : null;

    return <section className="pdf-root" aria-busy={busy}>
        {/* Barın ortası: açık belgenin ölçüsü. "8 sayfa · 2 işaretli" —
            kullanıcının "şu an neye bakıyorum" sorusunun ikinci yarısı. */}
        {order.length > 0 && <ToolbarStatus>
            {order.length} sayfa{pageSelection && selected.length > 0 ? ` · ${selected.length} işaretli` : ''}
        </ToolbarStatus>}

        {surfaces.tools && <ToolbarActions>
            {kind !== 'raster' && <Button disabled={cannotSave} onClick={() => run(true)}>Klasör Seçerek Kaydet</Button>}
            <Button variant="primary" disabled={cannotSave} onClick={() => run()}>
                {busy ? 'PDF hazırlanıyor…' : kind === 'raster' ? 'Görsel Paketi Kaydet' : 'Yeni PDF Kaydet'}
            </Button>
        </ToolbarActions>}

        {surfaces.tools && <InspectorPanel title="Araçlar" scope="pdf-root">
            <div className="pdf-controls" aria-label="PDF işlem kontrolleri">
                <ToolGroups kind={kind} busy={busy} onPick={pick}/>

                <InspectorSection title="Seçili araç">
                    <p className="tool-name">{tools[kind][0]}</p>
                    <p className="tool-hint">{tools[kind][1]}</p>
                    {settingField}
                    {(kind === 'merge' || kind === 'images') &&
                        <Button onClick={choose} disabled={busy}>Belge Ekle</Button>}
                    {sources.length > 0 && <ol className="source-list">{sources.map((s, i) => <li key={s.path}>
                        <span>{s.file_name} · {s.page_count} sayfa</span>
                        {sources.length > 1 && <IconButton label={`${i + 1}. belgeyi yukarı taşı`} disabled={busy || i === 0} onClick={() => moveSource(i)}>
                            <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 12.5V4M4.5 7.5 8 4l3.5 3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                        </IconButton>}
                    </li>)}</ol>}
                    {order.length > 0 && <>
                        <p className="tool-count">{order.length} kaynak sayfası{pageSelection ? ` · ${selected.length} işaretli` : ''} · Çıktı: {outputCount} sayfa</p>
                        {pageSelection && <div className="row">
                            <Button className="btn-sm" disabled={busy} onClick={() => setSelected(order.map(p => p.key))}>Tümünü İşaretle</Button>
                            <Button className="btn-sm" variant="quiet" disabled={busy} onClick={() => setSelected([])}>Seçimi Temizle</Button>
                        </div>}
                    </>}
                    {sources.some(s => s.is_signed) && <label className="approval"><input type="checkbox" checked={approved} onChange={e => setApproved(e.target.checked)}/>İmza işareti bulundu. Türetilmiş PDF kaynak elektronik imzanın doğrulanabilirliğini taşımaz; onaylıyorum.</label>}
                    <p className="tool-safe">Yeni bir kopya oluşturulur; kaynak belgeleriniz korunur.</p>
                </InspectorSection>
            </div>
        </InspectorPanel>}

        {/* Şerit yalnız gösterilecek sayfa varken açılır; belge yokken 132 px'lik
            boş bir bant çizmez ve hata/boş durum tam genişliği alır. */}
        <div className="pdf-workspace-layout" data-pages={surfaces.strip}>
            {surfaces.strip && <aside className="thumbnail-list" aria-label="Sayfa önizlemeleri">{order.map((item, index) => <div key={item.key} className={`thumbnail ${item.key === current ? 'current' : ''} ${selected.includes(item.key) ? 'selected' : ''} ${kind === 'delete' && selected.includes(item.key) ? 'removed' : ''}`}>
                {/* Önizleme motoru PDF SAYFASI çizer. Görsel kaynaklarda (Görseller
                    → PDF aracı) bu çağrı zorunlu olarak düşüyor ve her küçük resim
                    "önizleme oluşturulamadı" hatası gösteriyordu: araç çalışıyor
                    ama tamamen bozukmuş gibi görünüyordu. Görselde hata yerine
                    dosyanın kendisi söylenir. */}
                <button className="thumbnail-image-button" aria-label={`${item.source.file_name} sayfa ${item.page} görüntüle`} onClick={() => setCurrent(item.key)}>
                    {item.source.format === 'pdf'
                        ? <Preview item={item} rotation={rotations[item.key] || 0}/>
                        : <span className="thumbnail-placeholder">{item.source.file_name}</span>}
                </button>
                <span className="thumbnail-no">{index + 1}<span className="sr-only">. çıktı sırası · Kaynak s. {item.page}</span></span>
                {pageSelection && <label className="thumbnail-pick"><input type="checkbox" checked={selected.includes(item.key)} onChange={() => toggle(item.key)}/>{kind === 'delete' ? 'Çıkar' : kind === 'rotate' ? 'Döndür' : 'Dahil et'}</label>}
                {kind === 'reorder' && <div className="thumbnail-move">
                    <IconButton label={`${item.page}. sayfayı yukarı taşı`} disabled={index === 0 || busy} onClick={() => shiftPage(index, -1)}>
                        <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 12.5V4M4.5 7.5 8 4l3.5 3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                    </IconButton>
                    <IconButton label={`${item.page}. sayfayı aşağı taşı`} disabled={index === order.length - 1 || busy} onClick={() => shiftPage(index, 1)}>
                        <svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 3.5V12m-3.5-3.5L8 12l3.5-3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>
                    </IconButton>
                </div>}
            </div>)}</aside>}

            <aside className="pdf-preview-panel" aria-label="PDF önizleme çalışma alanı">{surfaces.strip && kind !== 'images' ? <>
                <div className="pdf-strip">
                    <div className="tool-segment" role="group" aria-label="Sayfa araçları">
                        {PAGE_TOOLS.map(key => <button key={key} type="button" data-tool={key} className={`segment ${kind === key ? 'is-current' : ''}`} title={tools[key][1]} aria-label={tools[key][0]} aria-pressed={kind === key} disabled={busy} onClick={() => pick(key)}>{SHORT[key]}</button>)}
                    </div>
                    {kind === 'rotate' && <div className="row">
                        <Button className="btn-sm" disabled={busy || !selected.length} onClick={() => setRotations(previous => rotatePages(previous, selected, -90))}>↶ Sola 90°</Button>
                        <Button className="btn-sm" disabled={busy || !selected.length} onClick={() => setRotations(previous => rotatePages(previous, selected, 90))}>↷ Sağa 90°</Button>
                    </div>}
                    <div className="toolbar-spacer"/>
                    <label className="zoom">Yakınlaştır<select value={zoom} onChange={e => setZoom(e.target.value.startsWith('fit-') ? e.target.value as PreviewMode : Number(e.target.value))}><option value="fit-page">Sayfaya sığdır</option><option value="fit-width">Genişliğe sığdır</option>{[75, 100, 125, 150, 200].map(z => <option key={z} value={z}>{z}%</option>)}</select></label>
                </div>
                {status && <Status tone={status.includes('tamamlanamadı') || status.includes('açılamadı') ? 'error' : 'success'}>{status}</Status>}
                <div className="pdf-canvas">
                    <p className="page-line">{active?.source.file_name} · Kaynak sayfa {active?.page} / {active?.source.page_count}{kind === 'delete' && selected.includes(active?.key) ? ' — Çıktıdan çıkarılacak' : ''}{kind === 'rotate' ? ` · Dönüş: ${rotations[active?.key] || 0}°` : ''}{selected.includes(active?.key) ? ' · İşaretli' : ''}</p>
                    <div className="pdf-page-viewport" tabIndex={0} role="region" aria-label="Kaydırılabilir PDF sayfası">{active && <Preview key={active.key} item={active} large mode={zoom} rotation={rotations[active.key] || 0}/>}</div>
                    <p className="preview-note">Kaynak sayfanın önizlemesi. Yeni PDF’nin her sayfasına içerik dışında sağ alt logo payı eklenir. Açıklama/form görünümleri bu önizlemede eksik olabilir; son kopyayı ayrıca inceleyin.</p>
                </div>
            </> : <PreviewPlaceholder state={docState} detail={failure} onOpenAnother={openAnother}/>}</aside>
        </div>
    </section>;
};
