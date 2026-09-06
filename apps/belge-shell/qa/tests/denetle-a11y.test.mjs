// Denetle yüzeyinin erişilebilirlik sözleşmesi.
//
// İkinciGöz'ün erişilebilirlik yaklaşımı taşımada kaybolmamalı. Bu dosya,
// gerçek bileşen ve gerçek CSS üzerinden statik olarak doğrulanabilen
// sözleşmeleri kilitler. Klavye akışı ve ekran okuyucu davranışının tamamı
// gerçek GUI kabulü gerektirir; burada iddia edilmez.
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { buildSync } from "esbuild";

const dir = mkdtempSync(resolve("qa/.a11y-"));
let mod;
try {
  const out = resolve(dir, "r.cjs");
  buildSync({
    stdin: {
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { ReviewWorkspace, SEVERITY } from './src/modules/ikincigoz/ReviewWorkspace';
        export const severity = SEVERITY;
        export const busy = renderToStaticMarkup(
          React.createElement(ReviewWorkspace, { path: '/tmp/belge.docx' })
        );
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

const css = readFileSync("src/shared-ui/shell.css", "utf8");

test("her ciddiyet düzeyinin metin karşılığı var", () => {
  // Bulgu ciddiyeti asla yalnız renkle verilemez.
  for (const level of ["error", "warning", "review"]) {
    assert.ok(mod.severity[level], `eksik ciddiyet: ${level}`);
    assert.ok(mod.severity[level].label.length > 0, `${level} metin etiketi taşımalı`);
    assert.ok(mod.severity[level].mark.length > 0, `${level} şekil işareti taşımalı`);
  }
  const labels = Object.values(mod.severity).map((s) => s.label);
  assert.equal(new Set(labels).size, 3, "etiketler birbirinden ayırt edilebilmeli");
  const marks = Object.values(mod.severity).map((s) => s.mark);
  assert.equal(new Set(marks).size, 3, "şekiller birbirinden ayırt edilebilmeli");
});

test("ciddiyet için renk tek başına taşıyıcı değil", () => {
  // Metin etiketi ayrı bir öğede taşınır; renk yalnızca yardımcıdır.
  assert.ok(css.includes(".finding-sev"), "ciddiyetin metin öğesi tanımlı olmalı");
  assert.ok(css.includes(".finding-mark"), "ciddiyetin şekil öğesi tanımlı olmalı");
});

test("işaretli metin aralığı yalnız arka planla belirtilmez", () => {
  const mark = css.slice(css.indexOf(".finding-excerpt mark"));
  assert.ok(
    /text-decoration:\s*underline/.test(mark),
    "vurgulanan aralık alt çizgi de taşımalı",
  );
});

test("hareket ve kontrast sistem tercihine bağlanabilir", () => {
  const tokens = readFileSync("src/shared-ui/tokens.css", "utf8");
  assert.ok(tokens.includes('[data-motion="reduced"]'), "hareket azaltma uygulanmalı");
  assert.ok(tokens.includes('[data-contrast="more"]'), "yüksek kontrast uygulanmalı");
  assert.ok(tokens.includes(":focus-visible"), "klavye odağı görünür olmalı");
});

test("uzun süren inceleme ekran okuyucuya bildirilir", () => {
  assert.ok(mod.busy.includes('role="status"'), "durum canlı bölgede bildirilmeli");
  assert.ok(mod.busy.includes("inceleniyor"), "kullanıcı ne olduğunu görmeli");
});

test("Denetle yüzeyinde geliştirme bilgisi yok", () => {
  const visible = mod.busy.replace(/<[^>]*>/g, " ");
  for (const needle of ["ikincigoz", "parser", "container_path", "block_index", "AnalysisDocument"]) {
    assert.ok(!visible.includes(needle), `kullanıcıya sızan teknik terim: ${needle}`);
  }
});
