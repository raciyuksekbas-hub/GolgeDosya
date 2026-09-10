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

for (const [name, body] of Object.entries(mod)) {
  for (const theme of ["light", "dark"]) {
    writeFileSync(
      `qa/shots/${name}-${theme}.html`,
      // lang="tr" ZORUNLU: CSS `text-transform: uppercase` yerel ayara duyarlıdır.
      // Türkçe olmadan "Erişilebilirlik" → "ERISILEBILIRLIK" olur (noktasız I).
      `<!doctype html><html lang="tr" data-theme="${theme}"><head><meta charset="utf-8">` +
        `<style>${tokens}\n${shell}</style></head>` +
        `<body><div style="height:100vh">${body}</div></body></html>`,
    );
  }
}
console.log("çizildi:", Object.keys(mod).join(", "));
