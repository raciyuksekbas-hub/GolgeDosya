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
        import { SettingsSheet } from './src/shell/Settings';
        import { Button, Pill } from './src/shared-ui/primitives';
        import { compareDocuments } from './src/modules/degisikis/core/compare';
        import { buildComparisonViewModel } from './src/modules/degisikis/viewModels/comparisonViewModel';
        import { DocumentPane } from './src/modules/degisikis/DocumentPane';
        import { ChangeRail } from './src/modules/degisikis/ChangeRail';
        import { ChangeInspector } from './src/modules/degisikis/ChangeInspector';

        const features = [
          { key: 'duzenek', label: 'Düzenle', route: 'duzenek', compiled: true, enabled: true },
          { key: 'tavzih', label: 'Dönüştür', route: 'tavzih', compiled: true, enabled: true },
          { key: 'degisikis', label: 'Karşılaştır', route: 'degisikis', compiled: true, enabled: true },
          { key: 'ikincigoz', label: 'Denetle', route: 'ikincigoz', compiled: true, enabled: true },
        ];
        // Rust gibi Unix SANİYE.
        const now = Math.floor(Date.now() / 1000);
        const recents = [
          { path: '/Belgeler/dava-dilekcesi.docx', openedAt: now - 12 * 60 },
          { path: '/Belgeler/ek-3 bilirkişi raporu.pdf', openedAt: now - 30 * 3600 },
          { path: '/Belgeler/İŞ SÖZLEŞMESİ.udf', openedAt: now - 3 * 86400 },
        ];
        const settings = { theme:'system', textScale:100, highContrast:'system', reduceMotion:'system',
          respectReducedMotion:true, acceptedTerms:1, outputDir:null, rendererPath:null,
          disabledRules:[], includeReview:true, sourceReadOnly:true, linearResults:false,
          recentDocuments:recents, migratedFrom:[] };

        const el = React.createElement;
        const shell = (props, children) => el(Layout, {
          features, current: 'duzenek', onNavigate(){}, onOpenSettings(){}, ...props,
        }, children);

        export const home = renderToStaticMarkup(shell({}, el(DocumentSurface, {
          feature: features[0], recents, outcome: null, onDocuments(){}, onForget(){},
        })));

        export const homeEmpty = renderToStaticMarkup(shell({}, el(DocumentSurface, {
          feature: features[2], recents: [], outcome: null, onDocuments(){}, onForget(){},
        })));

        // Belge açıkken: barda bağlam ve kabuğun kendi eylemi.
        const chip = el('span', { className: 'doc-chip' },
          el('span', { className: 'doc-chip-name' }, 'İŞ SÖZLEŞMESİ.docx'),
          el(Pill, null, 'docx'));
        export const withDoc = renderToStaticMarkup(shell({
          current: 'ikincigoz',
          context: chip,
          actions: el(Button, { variant: 'quiet' }, 'Kapat'),
        }, el('div', { className: 'surface review' },
          el('h2', { className: 'section-head' }, 'Bulgular'),
          el('ul', { className: 'findings' },
            el('li', { className: 'finding', 'data-severity': 'error', 'data-active': 'true' },
              el('button', { className: 'finding-head' },
                el('span', { className: 'finding-mark' }, '●'),
                el('span', { className: 'finding-sev' }, 'Kesin hata'),
                el('span', { className: 'finding-title' }, 'Taraf adı belge içinde tutarsız'),
                el('span', { className: 'finding-loc' }, '12. paragraf')),
              el('div', { className: 'finding-body' },
                el('p', { className: 'finding-message' }, 'Aynı taraf iki farklı biçimde yazılmış.'),
                el('p', { className: 'finding-excerpt selectable' }, 'İşbu sözleşme, taraflar arasında ',
                  el('mark', null, 'Yüksekbaş Ltd. Şti.'), ' ile akdedilmiş olup hükümleri aşağıda gösterilmiştir.'),
                el('p', { className: 'finding-why' }, 'Belge içinde aynı tüzel kişinin farklı yazımları, icra aşamasında taraf teşhisini güçleştirir.'))),
            el('li', { className: 'finding', 'data-severity': 'warning' },
              el('button', { className: 'finding-head' },
                el('span', { className: 'finding-mark' }, '▲'),
                el('span', { className: 'finding-sev' }, 'Uyarı'),
                el('span', { className: 'finding-title' }, 'Madde numarası atlanmış'),
                el('span', { className: 'finding-loc' }, '31. paragraf'))),
            el('li', { className: 'finding', 'data-severity': 'review' },
              el('button', { className: 'finding-head' },
                el('span', { className: 'finding-mark' }, '○'),
                el('span', { className: 'finding-sev' }, 'İncele'),
                el('span', { className: 'finding-title' }, 'Uzun cümle okunabilirliği düşürüyor'),
                el('span', { className: 'finding-loc' }, '7. paragraf')))))));

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
          el(Pill, null, 'docx'),
          el('span', { className: 'doc-chip-sep' }, '↔'),
          el('span', { className: 'doc-chip-name' }, 'sozlesme-v2.docx'),
          el(Pill, null, 'docx'));
        const doc = (name) => ({ name, extension: 'docx', size: 24576, blocks: [], warnings: [] });
        export const compare = renderToStaticMarkup(shell({
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
              paneRef: ref, rowRefs: mapRef, onScroll: noop })))));

        // Panel gerçek pencerede kabuğun sağ yuvasına portallanır; statik
        // çizimde portal çalışmadığı için kendi karesinde gösteriliyor.
        export const comparePanel = renderToStaticMarkup(
          el('div', { style: { width: '312px', height: '100vh', padding: '12px 12px 12px 0', background: 'var(--surface-workspace)' } },
            el('aside', { className: 'inspector compare-root', 'aria-label': 'Farklar' },
              el('div', { className: 'inspector-head' }, el('h2', { className: 'inspector-title' }, 'Farklar')),
              el('div', { className: 'inspector-body' },
                el(ChangeInspector, { summary: cmpModel.summary, changes: cmpModel.changes,
                  filter: 'all', onFilterChange: noop, filteredChanges: cmpModel.changes,
                  selectedChange: cmpModel.changes[1], onSelect: noop })))));

        export const settingsSheet = renderToStaticMarkup(
          el(React.Fragment, null,
            shell({}, el(DocumentSurface, {
              feature: features[0], recents, outcome: null, onDocuments(){}, onForget(){},
            })),
            el(SettingsSheet, { settings, version: '0.0.1', onChange(){}, onClose(){} })));
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
