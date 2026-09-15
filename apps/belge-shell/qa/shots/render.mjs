// Tasarım değerlendirmesi için gerçek bileşenlerin statik çizimi.
//
// Kabuğun KENDİSİ üzerinden çizer: `Layout` + `DocumentSurface`. Eskiden
// kenar çubuğu, bar ve yüzey elle yan yana diziliyordu; kabuk değiştiğinde
// çıktı sessizce yalan söylüyordu.
//
// Bu bir ekran görüntüsü değildir ve GUI kabulü yerine geçmez: yalnız görsel
// dili tarayıcıda gözle kontrol etmek için HTML üretir. Gerçek kabul
// paketlenmiş pencereden alınır (bkz. docs/design/02-yerlesim-haritasi.md §8).
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { buildSync } from "esbuild";

const dir = mkdtempSync(resolve("qa/.shot-"));
let mod;
try {
  const out = resolve(dir, "r.cjs");
  buildSync({
    stdin: {
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { Layout } from './src/shell/Layout';
        import { DocumentSurface } from './src/features/DocumentSurface';
        import { PreferencesSheet } from './src/shell/Settings';
        import { Button } from './src/shared-ui/primitives';
        import { compareDocuments } from './src/modules/degisikis/core/compare';
        import { buildComparisonViewModel } from './src/modules/degisikis/viewModels/comparisonViewModel';
        import { DocumentPane } from './src/modules/degisikis/DocumentPane';
        import { ChangeRail } from './src/modules/degisikis/ChangeRail';
        import { ChangeInspector } from './src/modules/degisikis/ChangeInspector';
        import { ConvertFlow, ConvertDone } from './src/modules/tavzih/ConvertWorkspace';
        import { OutputFolderRow } from './src/modules/tavzih/OutputFolderField';
        import { PdfWorkspace, ToolGroups } from './src/modules/duzenek/PdfWorkspace';
        import { FindingList, ReviewChrome, ReviewClear } from './src/modules/ikincigoz/ReviewWorkspace';

        const features = [
          { key: 'duzenek', label: 'Düzenle', route: 'duzenek', compiled: true, enabled: true },
          { key: 'tavzih', label: 'Dönüştür', route: 'tavzih', compiled: true, enabled: true },
          { key: 'degisikis', label: 'Karşılaştır', route: 'degisikis', compiled: true, enabled: true },
          { key: 'ikincigoz', label: 'Denetle', route: 'ikincigoz', compiled: true, enabled: true },
        ];
        // Rust gibi Unix SANİYE.
        const now = Math.floor(Date.now() / 1000);
        const recents = [
          { path: '/Belgeler/Baris_Dogan_itiraz_dilekcesi.docx', openedAt: now - 6 * 3600 },
          { path: '/Belgeler/ek-3 bilirkişi raporu.pdf', openedAt: now - 30 * 3600 },
          { path: '/Belgeler/İŞ SÖZLEŞMESİ.udf', openedAt: now - 3 * 86400 },
          { path: '/Belgeler/kesif-tutanagi.pdf', openedAt: now - 5 * 86400 },
          { path: '/Belgeler/sozlesme-v2.docx', openedAt: now - 9 * 86400 },
        ];
        const settings = { theme:'system', textScale:100, highContrast:'system', reduceMotion:'system',
          respectReducedMotion:true, acceptedTerms:1, outputDir:null, rendererPath:null,
          disabledRules:[], includeReview:true, sourceReadOnly:true, linearResults:false,
          recentDocuments:recents, migratedFrom:[] };

        const el = React.createElement;
        const shell = (props, children) => el(Layout, {
          features, current: 'duzenek', onNavigate(){}, onOpenSettings(){}, ...props,
        }, children);

        // Kabuk sağ paneli ve bar durumunu gerçek pencerede PORTALLA doldurur;
        // statik çizimde portal çalışmaz. Kompozisyonu görebilmek için ikisi de
        // yuvalarına string olarak yerleştirilir.
        // scope, gerçek uygulamada InspectorPanel'in aside'a eklediği modül
        // kapsam sınıfıdır (pdf-root, compare-root). Olmadan modül CSS'i
        // panele hiç uygulanmaz ve çizim yalan söyler.
        const withPanel = (shellHtml, panelHtml, title, scope) =>
          shellHtml.replace(
            '<div class="inspector-slot" hidden=""></div>',
            '<div class="inspector-slot"><aside class="inspector' + (scope ? ' ' + scope : '') +
              '" aria-label="' + title + '">' +
              '<div class="inspector-head"><h2 class="inspector-title">' + title + '</h2></div>' +
              '<div class="inspector-body">' + panelHtml + '</div></aside></div>',
          );
        const withStatus = (shellHtml, text) =>
          shellHtml.replace('<div class="toolbar-status"></div>',
            '<div class="toolbar-status">' + text + '</div>');
        // Kipin birincil eylemi de portalla gelir; barın sağ ucunu boş
        // göstermek gerçek pencereyi yanlış anlatır.
        const withActions = (shellHtml, html) =>
          shellHtml.replace('<div class="toolbar-actions"></div>',
            '<div class="toolbar-actions">' + html + '</div>');
        const primary = (label) => renderToStaticMarkup(el(Button, { variant: 'primary' }, label));
        const secondary = (label) => renderToStaticMarkup(el(Button, null, label));

        const welcome = (i, withRecents, outcome) => renderToStaticMarkup(shell({
          current: features[i].route,
          context: el('span', { className: 'toolbar-mode' }, features[i].label),
        }, el(DocumentSurface, {
          feature: features[i], recents: withRecents ? recents : [],
          outcome: outcome ?? null, onDocuments(){}, onForget(){},
        })));
        export const home = welcome(0, true);
        export const welcomeConvert = welcome(1, true);
        export const welcomeCompare = welcome(2, true);
        export const welcomeReview = welcome(3, false);
        // İlk belge seçildi: A yuvası dolu, sıra B'de.
        export const welcomeCompareOne = welcome(2, true, {
          kind: 'needsMore', paths: ['/Belgeler/sozlesme-v1.docx'], missing: 1,
        });

        /**
         * Karşılaştır — GERÇEK motorla. Diff motoru saf TypeScript olduğu için
         * IPC olmadan çalışır: aşağıdaki satırlar, ray düğümleri, özet sayıları
         * ve yüzdeler compareDocuments + buildComparisonViewModel
         * çıktısıdır, elle yazılmış değil.
         */
        const makeBlocks = (texts, prefix) => texts.map((text, i) => ({
          id: prefix + '-' + i, kind: /^MADDE/.test(text) ? 'heading' : 'paragraph',
          text, label: /^MADDE/.test(text) ? text.split(' —')[0] : undefined,
        }));
        const BASE = [
          'MADDE 1 — TARAFLAR',
          'İşbu sözleşme, Yüksekbaş Ltd. Şti. ile Yüklenici arasında akdedilmiştir.',
          'MADDE 2 — KONU',
          'Sözleşmenin konusu, ekte belirtilen hizmetlerin ifasıdır.',
          'MADDE 3 — HİZMET BEDELİ',
          '3.1. Hizmet bedeli KDV dahil 118.000 TL olarak kararlaştırılmıştır.',
          '3.2. Ödeme, fatura tarihinden itibaren 30 gün içinde yapılır.',
        ];
        const REVISED = [
          'MADDE 1 — TARAFLAR',
          'İşbu sözleşme, Yüksekbaş Limited Şirketi ile Yüklenici arasında akdedilmiştir.',
          'MADDE 2 — KONU',
          'Sözleşmenin konusu, ekte belirtilen hizmetlerin ifasıdır.',
          'MADDE 3 — HİZMET BEDELİ',
          '3.1. Hizmet bedeli KDV dahil 142.000 TL olarak kararlaştırılmıştır.',
          '3.2. Ödeme, fatura tarihinden itibaren 45 gün içinde yapılır.',
          '3.3. Gecikme hâlinde aylık %2 gecikme faizi uygulanır.',
        ];
        const comparison = compareDocuments(makeBlocks(BASE, 'base'), makeBlocks(REVISED, 'revised'));
        const cmpModel = buildComparisonViewModel(comparison);
        const noop = () => undefined;
        const ref = { current: null };
        const mapRef = { current: new Map() };
        const cmpNames = el('span', { className: 'doc-chip' },
          el('span', { className: 'doc-chip-name' }, 'sozlesme-v1.docx'),
          el('span', { className: 'doc-chip-sep' }, '↔'),
          el('span', { className: 'doc-chip-name' }, 'sozlesme-v2.docx'));
        const doc = (name) => ({ name, extension: 'docx', size: 24576, blocks: [], warnings: [] });
        const comparePanelBody = renderToStaticMarkup(el(ChangeInspector, {
          summary: cmpModel.summary, filter: 'all', onFilterChange: noop,
          filteredChanges: cmpModel.changes, selectedChange: cmpModel.changes[1], onSelect: noop,
        }));
        export const compare = withPanel(withStatus(renderToStaticMarkup(shell({
          current: 'degisikis', context: cmpNames,
          actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el('div', { className: 'compare-root' },
          el('div', { className: 'compare-panes' },
            el(DocumentPane, { side: 'base', doc: doc('sozlesme-v1.docx'), rows: comparison.rows,
              changes: cmpModel.changes, selectedRows: cmpModel.changes[1]?.rowIndices ?? [],
              paneRef: ref, rowRefs: mapRef, onScroll: noop }),
            el(ChangeRail, { changes: cmpModel.changes, selectedIndex: 1, onSelect: noop, onMove: noop,
              onSwap: noop, canSwap: true, paneRef: ref, rowRefs: mapRef, syncToken: 'x' }),
            el(DocumentPane, { side: 'revised', doc: doc('sozlesme-v2.docx'), rows: comparison.rows,
              changes: cmpModel.changes, selectedRows: cmpModel.changes[1]?.rowIndices ?? [],
              paneRef: ref, rowRefs: mapRef, onScroll: noop }))))),
          '2 / ' + cmpModel.changes.length + ' fark'),
          comparePanelBody, 'Farklar', 'compare-root');

        /* --------------------------------------------------------- Dönüştür */
        const cnvChip = el('span', { className: 'doc-chip' },
          el('span', { className: 'doc-chip-name' }, 'Baris_Dogan_itiraz_dilekcesi.docx'));
        const cnvFolder = { path: '/Users/örnek/Belgeler/Dönüştürülen Belgeler', is_default: true };
        const cnvSelected = [{ path: '/Users/örnek/Belgeler/Baris_Dogan_itiraz_dilekcesi.docx',
          info: { name: 'Baris_Dogan_itiraz_dilekcesi.docx', source_format: 'Word (.docx)',
            size_label: '2,6 KB', target_format: 'UYAP (.udf)' } }];

        export const convert = withStatus(renderToStaticMarkup(shell({
          current: 'tavzih', context: cnvChip, actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el('div', { className: 'surface convert' }, el('div', { className: 'convert-body' },
          el(ConvertFlow, {
            selected: cnvSelected, target: 'UYAP (.udf)', folder: cnvFolder,
            onConvert: noop, onChooseFolder: noop, onResetFolder: noop,
          }))))), 'Word (.docx) → UYAP (.udf)');

        export const convertDone = withStatus(renderToStaticMarkup(shell({
          current: 'tavzih', context: cnvChip, actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el('div', { className: 'surface convert' }, el('div', { className: 'convert-body' },
          el(ConvertDone, {
            from: 'Word (.docx)', to: 'UYAP (.udf)', folder: cnvFolder,
            onReveal: noop, onAgain: noop,
            items: [{ source: '/a/x.docx', source_name: 'Baris_Dogan_itiraz_dilekcesi.docx',
              output_name: 'Baris_Dogan_itiraz_dilekcesi.udf', status: 'success', source_unchanged: true, warnings: [
                { code: 'W1', severity: 'APPROXIMATION', title: 'Tablo hücre kenarlıkları yaklaşık aktarıldı', location: 's. 2' },
              ] }],
          }))))), 'Word (.docx) → UYAP (.udf)');

        /* ---------------------------------------------------------- Düzenle */
        const editChip = el('span', { className: 'doc-chip' },
          el('span', { className: 'doc-chip-name' }, 'ek-3 bilirkişi raporu.pdf'));

        export const editEmpty = renderToStaticMarkup(shell({
          current: 'duzenek', context: editChip, actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el(PdfWorkspace, {})));

        // Sayfa şeridi ve tuvali gerçek motor önizlemesi olmadan çizilemez;
        // yalnız KOMPOZİSYONU görmek için bileşenin biçimlendirmesi birebir
        // yeniden yazıldı (sözleşme testleri bileşenin kendisini denetler).
        const thumb = (n, state) => el('div', { key: n, className: 'thumbnail ' + state },
          el('button', { className: 'thumbnail-image-button', 'aria-label': 'sayfa ' + n },
            el('div', { className: 'pdf-thumbnail-stage' },
              el('div', { className: 'pdf-page-surface', style: { width: 80, height: 106 } }))),
          el('span', { className: 'thumbnail-no' }, String(n)),
          el('label', { className: 'thumbnail-pick' },
            el('input', { type: 'checkbox', defaultChecked: state.includes('selected'), readOnly: true }), 'Dahil et'));

        const editBody = el('section', { className: 'pdf-root' },
          el('div', { className: 'pdf-workspace-layout', 'data-pages': true },
            el('aside', { className: 'thumbnail-list', 'aria-label': 'Sayfa önizlemeleri' },
              [1,2,3,4,5,6,7,8].map(n => thumb(n, n === 3 ? 'current selected' : n === 5 ? 'selected' : ''))),
            el('aside', { className: 'pdf-preview-panel', 'aria-label': 'PDF önizleme çalışma alanı' },
              el('div', { className: 'pdf-strip' },
                el('div', { className: 'tool-segment', role: 'group', 'aria-label': 'Sayfa araçları' },
                  [['select','Seç'],['reorder','Sırala'],['delete','Sil'],['rotate','Döndür']].map(([k, t], i) =>
                    el('button', { key: k, type: 'button', 'data-tool': k, className: 'segment', 'aria-pressed': i === 0 }, t))),
                el('div', { className: 'toolbar-spacer' }),
                el('label', { className: 'zoom' }, 'Yakınlaştır',
                  el('select', { defaultValue: 'fit', readOnly: true }, el('option', { value: 'fit' }, 'Sayfaya sığdır')))),
              el('div', { className: 'pdf-canvas' },
                el('p', { className: 'page-line' }, 'ek-3 bilirkişi raporu.pdf · Kaynak sayfa 3 / 8 · İşaretli'),
                el('div', { className: 'pdf-page-viewport', tabIndex: 0, role: 'region', 'aria-label': 'Kaydırılabilir PDF sayfası' },
                  el('div', { className: 'pdf-preview-stage' },
                    el('div', { className: 'pdf-page-surface', style: { width: 464, height: 600 } }))),
                el('p', { className: 'preview-note' }, 'Kaynak sayfanın önizlemesi. Yeni PDF’nin her sayfasına içerik dışında sağ alt logo payı eklenir. Açıklama/form görünümleri bu önizlemede eksik olabilir; son kopyayı ayrıca inceleyin.')))));

        const editPanel = renderToStaticMarkup(el('div', { className: 'pdf-controls', 'aria-label': 'PDF işlem kontrolleri' },
          el(ToolGroups, { kind: 'select', onPick(){} }),
          el('section', { className: 'inspector-section' },
            el('h3', { className: 'inspector-label' }, 'Seçili araç'),
            el('p', { className: 'tool-name' }, 'Seçili sayfalar → yeni PDF'),
            el('p', { className: 'tool-hint' }, 'İşaretlediğiniz sayfalardan yeni bir PDF kopyası oluşturur.'),
            el('ol', { className: 'source-list' }, el('li', null, el('span', null, 'ek-3 bilirkişi raporu.pdf · 8 sayfa'))),
            el('p', { className: 'tool-count' }, '8 kaynak sayfası · 2 işaretli · Çıktı: 2 sayfa'),
            el('div', { className: 'row' },
              el(Button, { className: 'btn-sm' }, 'Tümünü İşaretle'),
              el(Button, { className: 'btn-sm', variant: 'quiet' }, 'Seçimi Temizle')),
            el('p', { className: 'tool-safe' }, 'Yeni bir kopya oluşturulur; kaynak belgeleriniz korunur.'))));

        export const editPages = withPanel(withActions(withStatus(renderToStaticMarkup(shell({
          current: 'duzenek', context: editChip,
          actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, editBody)), '8 sayfa · 2 işaretli'),
          secondary('Klasör Seçerek Kaydet') + primary('Yeni PDF Kaydet')),
          editPanel, 'Araçlar', 'pdf-root');

        /* ---------------------------------------------------------- Denetle */
        const doc8 = { fileName: 'dava-dilekcesi.docx', format: 'docx', blockCount: 26, wordCount: 178, charCount: 1140 };
        const P = {
          p6: 'Dava, görevli ve yetkili mahkemede süresi içinde açılmış bulunmaktadır.',
          p7: 'Müvekkil şirket adına düzenlenen vekaletname Ek-1de sunulmuştur .',
          p11: 'Sözleşme bedeli 250.000,00 TL (İki Yüz Elli Bin Türk Lirası) olarak kararlaştırılmıştır.',
          p12: 'Taraflar arasında imzalanan sözleşme,, tarafların serbest iradeleriyle kurulmuştur.',
          p15: 'Bu husus 3. maddede açıkça düzenlenmiştir.',
          p19: 'Yukarıda açıklanan nedenlerle ve resen gözetilecek sebeplerle ; davanın kabulü gerekir.',
          p24: 'Ek-4te sunulan bilirkişi raporu bu hususu doğrulamaktadır.',
          p25: 'Yargılama giderlerinin karşı tarafa yükletilmesine karar verilmesini talep ederiz.',
        };
        const finding = (id, sev, title, message, why, block, range, fix) => ({
          rule_id: id, title, severity: sev, confidence: 1, message, explanation: why,
          block_id: block, location: { blockIndex: 0, runIndex: null, charStart: range && range[0], charEnd: range && range[1], containerPath: null },
          context: null, fix: fix || null,
        });
        const findings8 = [
          finding('r1', 'error', 'Noktalamadan önce boşluk', 'Nokta işaretinden önce boşluk var.',
            'Türkçe yazımda noktalama işaretinden önce boşluk bırakılmaz.', 'p7', [62, 64],
            { block_id: 'p7', char_start: 62, char_end: 64, original: ' .', replacement: '.', description: 'boşluk kaldırılır' }),
          finding('r2', 'error', 'Çift noktalama', 'Arka arkaya iki virgül var.', 'Aynı noktalama işareti tekrarlanmaz.', 'p12', [43, 45],
            { block_id: 'p12', char_start: 43, char_end: 45, original: ',,', replacement: ',', description: 'tek virgüle indirilir' }),
          finding('r3', 'error', 'Ek ayrı yazılmalı', 'Kısaltmaya gelen ek kesme ile ayrılır.', 'Kısaltmalara getirilen ekler kesme işaretiyle ayrılır.', 'p24', [0, 5]),
          finding('r4', 'error', 'Ek ayrı yazılmalı', 'Kısaltmaya gelen ek kesme ile ayrılır.', 'Kısaltmalara getirilen ekler kesme işaretiyle ayrılır.', 'p7', [37, 42]),
          finding('r5', 'error', 'Rakam ile yazı uyuşmuyor', 'Rakam ve yazıyla verilen tutar farklı.', 'Tutarın rakamla ve yazıyla yazımı aynı olmalıdır.', 'p11', [16, 28]),
          finding('r6', 'warning', 'Noktalamadan önce boşluk', 'Noktalı virgülden önce boşluk var.', 'Noktalama işaretinden önce boşluk bırakılmaz.', 'p19', [69, 71],
            { block_id: 'p19', char_start: 69, char_end: 71, original: ' ;', replacement: ';', description: 'boşluk kaldırılır' }),
          finding('r7', 'warning', 'Belirsiz atıf', '“Bu husus” hangi paragrafa atıf yaptığı belirsiz.', 'Atıf yapılan yer açıkça gösterilmelidir.', 'p15', [0, 9]),
          finding('r8', 'review', 'Uzun cümle', 'Cümle 28 kelimeden uzun.', 'Uzun cümleler dilekçede okunurluğu düşürür.', 'p25', [0, 20]),
        ];
        const result8 = {
          document: doc8,
          blocks: Object.keys(P).map(id => ({ id, kind: 'paragraph', text: P[id], isHeading: false, level: null })),
          findings: findings8, errorCount: 5, warningCount: 2, reviewCount: 1,
          truncatedRules: [], elapsedMs: 24, profileName: null,
        };
        const blockText8 = new Map(Object.keys(P).map(id => [id, P[id]]));
        const reviewChip = el('span', { className: 'doc-chip' },
          el('span', { className: 'doc-chip-name' }, 'dava-dilekcesi.docx'));

        // Bulgu yok: üçüncü kolon açılmaz ama belge bağlamı barda yaşamaya devam eder.
        export const review0 = withStatus(renderToStaticMarkup(shell({
          current: 'ikincigoz', context: reviewChip, actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el('div', { className: 'surface review' },
          el(ReviewClear, { doc: { ...doc8, blockCount: 13, wordCount: 84 } })))), 'Bulgu yok');

        const chromeHtml = renderToStaticMarkup(el(ReviewChrome, {
          result: result8, fixable: findings8.filter(f => f.fix), selected: new Set(),
          onApply(){}, onSelectAll(){}, onClearSelection(){},
        }));
        const bodyTag = '<div class="inspector-body">';
        const reviewPanel = chromeHtml.slice(
          chromeHtml.indexOf(bodyTag) + bodyTag.length,
          chromeHtml.lastIndexOf('</div></aside>'),
        );

        export const review8 = withPanel(withActions(withStatus(renderToStaticMarkup(shell({
          current: 'ikincigoz', context: reviewChip,
          actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el('div', { className: 'surface review' },
          el(FindingList, {
            findings: findings8, blockText: blockText8, activeKey: 'r1:p7:62',
            selectedKeys: new Set(), onSetActive(){}, onToggleFix(){},
          })))), '5 kesin hata · 2 uyarı · 1 incele'),
          primary('Kopyaya Uygula')), reviewPanel, 'Denetim özeti');

        /* --------------------------------------------------------- Ayarlar */
        // Çıktı klasörü satırı motor çağrısına bağlıdır ve statik çizimde
        // komut yoktur; gerçek bileşen fixture klasörle çizilip kendi
        // yuvasına yerleştirilir (portal/komut yokluğunun harness karşılığı).
        const folderRow = renderToStaticMarkup(el(OutputFolderRow, {
          folder: { path: '/Users/örnek/Belgeler/Dönüştürülen Belgeler', is_default: true },
          onChoose: noop, onReset: noop,
        }));
        const prefs = (tab) => renderToStaticMarkup(
          el(React.Fragment, null,
            shell({ context: el('span', { className: 'toolbar-mode' }, 'Düzenle') },
              el(DocumentSurface, { feature: features[0], recents, outcome: null, onDocuments(){}, onForget(){} })),
            el(PreferencesSheet, {
              settings, version: '0.2.0', tab, onTab(){}, onChange(){}, onForget(){}, onClose(){},
            }))).replace('<div class="prefs-group"></div>',
              '<div class="prefs-group">' + folderRow + '</div>');
        export const prefsGenel = prefs('genel');
        export const prefsGorunum = prefs('gorunum');
        export const prefsErisim = prefs('erisim');
        export const prefsHakkinda = prefs('hakkinda');
        export const prefsTelif = prefs('telif');
        export const prefsGeri = prefs('geri');
      `,
      resolveDir: process.cwd(),
      loader: "tsx",
    },
    bundle: true, platform: "node", format: "cjs", outfile: out,
    logLevel: "silent", external: ["@tauri-apps/*"],
  });
  mod = createRequire(import.meta.url)(out);
} finally {
  rmSync(dir, { recursive: true, force: true });
}

const tokens = readFileSync("src/shared-ui/tokens.css", "utf8");
const shell = readFileSync("src/shared-ui/shell.css", "utf8");
// Modül stilleri de gerçek uygulamadaki sırayla: modüller önce, kabuk sonra
// (main.tsx'te de böyle; eşit özgüllükte kabuk kazanır).
const compare = readFileSync("src/modules/degisikis/compare.css", "utf8");
const pdf = readFileSync("src/modules/duzenek/pdf.css", "utf8");

for (const [name, body] of Object.entries(mod)) {
  for (const theme of ["light", "dark"]) {
    writeFileSync(
      `qa/shots/${name}-${theme}.html`,
      // lang="tr" ZORUNLU: CSS `text-transform: uppercase` yerel ayara duyarlıdır.
      // Türkçe olmadan "Erişilebilirlik" → "ERISILEBILIRLIK" olur (noktasız I).
      `<!doctype html><html lang="tr" data-theme="${theme}"><head><meta charset="utf-8">` +
        `<style>${compare}\n${pdf}\n${tokens}\n${shell}</style></head>` +
        `<body><div style="height:100vh">${body}</div></body></html>`,
    );
  }
}
console.log("çizildi:", Object.keys(mod).join(", "));
