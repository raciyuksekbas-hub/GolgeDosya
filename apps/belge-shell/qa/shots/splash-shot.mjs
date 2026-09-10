// Açılış ekranını gerçek bileşenden çiz: paketlenmiş pencerede ~80 ms görünür,
// screencapture yakalayamıyor. Bu, aynı Splash bileşeninin statik çizimidir.
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { buildSync } from "esbuild";
const dir = mkdtempSync(resolve("qa/.splash-"));
let mod;
try {
  const out = resolve(dir, "s.cjs");
  buildSync({
    stdin: {
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { Splash } from './src/shell/Splash';
        export const splash = renderToStaticMarkup(React.createElement(Splash));
      `,
      resolveDir: process.cwd(), loader: "tsx",
    },
    bundle: true, platform: "node", format: "cjs", outfile: out,
    logLevel: "silent", external: ["@tauri-apps/*"],
    define: { __APP_VERSION__: JSON.stringify(JSON.parse(readFileSync("package.json","utf8")).version) },
  });
  mod = createRequire(import.meta.url)(out);
} finally { rmSync(dir, { recursive: true, force: true }); }
const tokens = readFileSync("src/shared-ui/tokens.css", "utf8");
const shell = readFileSync("src/shared-ui/shell.css", "utf8");
for (const theme of ["dark", "light"]) {
  writeFileSync(`qa/shots/splash-${theme}.html`,
    `<!doctype html><html lang="tr" data-theme="${theme}"><head><meta charset="utf-8">` +
    `<style>${tokens}\n${shell}</style></head><body><div style="height:100vh">${mod.splash}</div></body></html>`);
}
console.log("splash çizildi");
