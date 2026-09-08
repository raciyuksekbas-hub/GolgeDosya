// Tasarım değerlendirmesi için gerçek bileşenlerin statik çizimi.
// Tauri IPC olmadan çalışır: yalnız görsel dili görmek için.
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
        import { Sidebar } from './src/shell/Sidebar';
        import { DocumentSurface } from './src/features/DocumentSurface';
        import { SettingsSheet } from './src/shell/Settings';
        import { Toolbar, ToolbarTitle, ToolbarSpacer, Button, Section, Status } from './src/shared-ui/primitives';

        const features = [
          { key: 'duzenek', label: 'Düzenle', route: 'duzenek', compiled: true, enabled: true },
          { key: 'tavzih', label: 'Dönüştür', route: 'tavzih', compiled: true, enabled: true },
          { key: 'degisikis', label: 'Karşılaştır', route: 'degisikis', compiled: true, enabled: true },
          { key: 'ikincigoz', label: 'Denetle', route: 'ikincigoz', compiled: true, enabled: true },
        ];
        const now = Date.now();
        const recents = [
          { path: '/Users/x/Belgeler/İŞ SÖZLEŞMESİ.docx', openedAt: now - 9e5 },
          { path: '/Users/x/Belgeler/dilekçe-taslak.udf', openedAt: now - 7e6 },
          { path: '/Users/x/Belgeler/ek-3 bilirkişi raporu.pdf', openedAt: now - 2e8 },
        ];
        const settings = { theme:'system', textScale:100, highContrast:'system', reduceMotion:'system',
          respectReducedMotion:true, acceptedTerms:1, outputDir:null, rendererPath:null,
          disabledRules:[], includeReview:true, sourceReadOnly:true, linearResults:false,
          recentDocuments:recents, migratedFrom:[] };

        const el = React.createElement;
        export const home = renderToStaticMarkup(
          el('div', { className: 'shell' },
            el(Sidebar, { features, current: 'tavzih', onNavigate(){}, onOpenSettings(){}, openDocuments: [] }),
            el('section', { className: 'content' },
              el(Toolbar, null,
                el(ToolbarTitle, { title: 'Dönüştür', subtitle: 'Word ve UYAP biçimleri arasında' }),
                el(ToolbarSpacer)),
              el('main', { className: 'content-body' },
                el(DocumentSurface, { feature: features[1], recents, outcome: null, onDocuments(){}, onForget(){} })))));

        export const withDoc = renderToStaticMarkup(
          el('div', { className: 'shell' },
            el(Sidebar, { features, current: 'ikincigoz', onNavigate(){}, onOpenSettings(){},
              openDocuments: ['/Users/x/Belgeler/İŞ SÖZLEŞMESİ.docx'] }),
            el('section', { className: 'content' },
              el(Toolbar, null,
                el(ToolbarTitle, { title: 'Denetle', subtitle: 'İŞ SÖZLEŞMESİ.docx' }),
                el(ToolbarSpacer),
                el(Button, { variant: 'quiet' }, 'Kapat')),
              el('main', { className: 'content-body' },
                el('div', { className: 'surface' },
                  el('p', { className: 'doc-meta' }, '48 paragraf · 2.140 kelime — 3 bulgu: 1 hata, 2 uyarı'),
                  el(Section, { title: 'Bulgular', id: 'b' },
                    el('ul', { className: 'findings' },
                      el('li', { className: 'finding', 'data-severity': 'error', 'data-active': 'true' },
                        el('button', { className: 'finding-head' },
                          el('span', { className: 'finding-mark' }, '✕'),
                          el('span', { className: 'finding-sev' }, 'Hata'),
                          el('span', { className: 'finding-title' }, 'Taraf adı belge içinde tutarsız'),
                          el('span', { className: 'finding-loc' }, '12. paragraf')),
                        el('div', { className: 'finding-body' },
                          el('p', { className: 'finding-message' }, 'Aynı taraf iki farklı biçimde yazılmış.'),
                          el('p', { className: 'finding-excerpt selectable' }, '…taraflar arasında ',
                            el('mark', null, 'Yüksekbaş Ltd. Şti.'), ' ile akdedilen…'),
                          el('p', { className: 'finding-why' }, 'Belge içinde aynı tüzel kişinin farklı yazımları, icra aşamasında taraf teşhisini güçleştirir.'))),
                      el('li', { className: 'finding', 'data-severity': 'warning' },
                        el('button', { className: 'finding-head' },
                          el('span', { className: 'finding-mark' }, '!'),
                          el('span', { className: 'finding-sev' }, 'Uyarı'),
                          el('span', { className: 'finding-title' }, 'Madde numarası atlanmış'),
                          el('span', { className: 'finding-loc' }, '31. paragraf'))),
                      el('li', { className: 'finding', 'data-severity': 'review' },
                        el('button', { className: 'finding-head' },
                          el('span', { className: 'finding-mark' }, '?'),
                          el('span', { className: 'finding-sev' }, 'İnceleme'),
                          el('span', { className: 'finding-title' }, 'Uzun cümle okunabilirliği düşürüyor'),
                          el('span', { className: 'finding-loc' }, '7. paragraf'))))))))));

        export const settingsSheet = renderToStaticMarkup(
          el('div', { className: 'shell' },
            el(Sidebar, { features, current: 'duzenek', onNavigate(){}, onOpenSettings(){}, openDocuments: [] }),
            el('section', { className: 'content' },
              el(Toolbar, null, el(ToolbarTitle, { title: 'Düzenle', subtitle: 'Dilekçe eklerini hazırlayın' }), el(ToolbarSpacer)),
              el('main', { className: 'content-body' })),
            el(SettingsSheet, { settings, onChange(){}, onClose(){} })));
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
      // Uygulamanın index.html'i lang="tr" taşıyor; inceleme koşumu da taşımalı,
      // yoksa gerçekte olmayan bir hata görülür.
      `<!doctype html><html lang="tr"><head><meta charset="utf-8">` +
        `<style>${tokens}\n${shell}</style></head>` +
        `<body${theme === "dark" ? ' data-shot-theme="dark"' : ""}>` +
        `<div style="height:100vh">${body}</div>` +
        (theme === "dark"
          ? `<script>document.documentElement.setAttribute("data-theme","dark")</script>`
          : "") +
        `</body></html>`,
    );
  }
}
console.log("çizildi:", Object.keys(mod).join(", "));
