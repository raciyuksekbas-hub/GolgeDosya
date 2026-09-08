// Kabuk yüzeyi sözleşmesi.
//
// Gerçek React bileşenlerini sunucuda render eder ve çıktıda NE OLMASI, NE
// OLMAMASI gerektiğini doğrular. Bu gerçek bir pencere ölçümü değildir; amacı
// geliştirme yüzeyinin son kullanıcı ekranına geri sızmasını engellemektir.
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { buildSync } from "esbuild";

const dir = mkdtempSync(resolve("qa/.shell-ux-"));
let render;
try {
  const out = resolve(dir, "shell.cjs");
  buildSync({
    stdin: {
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { Sidebar } from './src/shell/Sidebar';
        import { DocumentSurface } from './src/features/DocumentSurface';
        const features = [
          { key:'duzenek',   label:'Düzenle',     route:'/duzenle',     compiled:false, enabled:false },
          { key:'tavzih',    label:'Dönüştür',    route:'/donustur',    compiled:true,  enabled:true  },
          { key:'degisikis', label:'Karşılaştır', route:'/karsilastir', compiled:false, enabled:false },
          { key:'ikincigoz', label:'Denetle',     route:'/denetle',     compiled:false, enabled:false },
        ];
        export const sidebar = renderToStaticMarkup(
          React.createElement(Sidebar, { features, current:'/donustur', onNavigate(){}, onOpenSettings(){} })
        );
        export const surfaceEmpty = renderToStaticMarkup(
          React.createElement(DocumentSurface, {
            feature: features[1], recents: [], onDocuments(){}, onForget(){},
          })
        );
        export const surfaceRecents = renderToStaticMarkup(
          React.createElement(DocumentSurface, {
            feature: features[1],
            recents: [{ path:'/Belgeler/İŞ SÖZLEŞMESİ.docx', openedAt: 1 }],
            onDocuments(){}, onForget(){},
          })
        );
      `,
      resolveDir: process.cwd(),
      loader: "tsx",
    },
    bundle: true,
    platform: "node",
    format: "cjs",
    outfile: out,
    logLevel: "silent",
    external: ["@tauri-apps/*"],
  });
  render = createRequire(import.meta.url)(out);
} finally {
  rmSync(dir, { recursive: true, force: true });
}

const markup = render.sidebar + render.surfaceEmpty + render.surfaceRecents;

/** Etiketler çıkarılmış görünür metin. */
const visibleText = (html) => html.replace(/<[^>]*>/g, " ");

/** Kullanıcıya okunan öznitelikler: tooltip ve erişilebilirlik adları. */
const spokenAttributes = (html) =>
  [...html.matchAll(/(?:title|aria-label|placeholder)="([^"]*)"/g)].map((m) => m[1]).join(" ");

const userVisible = visibleText(markup) + " " + spokenAttributes(markup);

// Geliştirme yüzeyi son kullanıcıya asla görünmemeli. Bu liste, kabuğun ilk
// sürümünde gerçekten ekranda duran ve kaldırılan metinlerdir.
const FORBIDDEN = [
  "bölüm kullanılabilir",
  "henüz taşınmadı",
  "Phase",
  "localStorage",
  "Library/Application Support",
  "BELGE_DISABLE",
  "geçici ad",
  "tr.yuksekbas",
  "migration",
  "Migration",
];

test("kullanıcı yüzeyinde hiçbir geliştirme bilgisi yok", () => {
  // Görünür metin ve kullanıcıya okunan öznitelikler denetlenir.
  // `data-feature` gibi öznitelikler kasıtlıdır: log ve hata ayıklama içindir,
  // ekranda görünmez ve ekran okuyucuya söylenmez.
  for (const needle of FORBIDDEN) {
    assert.ok(
      !userVisible.includes(needle),
      `Kullanıcı arayüzünde görünmemesi gereken metin bulundu: ${needle}`,
    );
  }
});

test("hata ayıklama bilgisi data özniteliğinde kalır", () => {
  assert.ok(render.sidebar.includes('data-feature="tavzih"'));
  assert.ok(!spokenAttributes(render.sidebar).includes("tavzih"));
});

test("ana navigasyon dört eylem adını gösterir", () => {
  for (const label of ["Düzenle", "Dönüştür", "Karşılaştır", "Denetle"]) {
    assert.ok(render.sidebar.includes(label), `Kenar çubuğunda eksik: ${label}`);
  }
  assert.ok(render.sidebar.includes("Ayarlar"));
});

test("eski ürün adları kullanıcıya gösterilmez, yalnız data özniteliğinde kalır", () => {
  // data-feature log/debug içindir; görünür metin değildir.
  assert.ok(render.sidebar.includes('data-feature="tavzih"'));
  const visible = render.sidebar.replace(/<[^>]*>/g, "");
  for (const brand of ["Tavzih", "DüzenEk", "Değişikİş", "İkinciGöz"]) {
    assert.ok(!visible.includes(brand), `Görünür metinde eski ürün adı: ${brand}`);
  }
});

test("taşınmamış bölüm soluk ve tıklanamaz, gövdede açıklama yok", () => {
  const disabled = (render.sidebar.match(/disabled/g) || []).length;
  assert.equal(disabled, 3, "üç bölüm devre dışı olmalı");
  assert.ok(render.sidebar.includes('aria-current="page"'), "etkin bölüm işaretlenmeli");
});

test("ana ekran belge merkezlidir", () => {
  // Tek net birincil görev. Metin tasarım turunda "Dosya Aç"tan "Belge Aç"a
  // döndü: ürün dosyalarla değil BELGELERLE çalışır ve dil bunu yansıtmalı.
  // Sözleşme değişmedi — tek, açık, birincil bir açma eylemi.
  assert.ok(render.surfaceEmpty.includes("Belge Aç"), "birincil açma eylemi görünmeli");
  assert.ok(render.surfaceEmpty.includes("sürükleyin"), "sürükle-bırak ipucu olmalı");
  // Boş durumda son kullanılanlar bölümü hiç çizilmez: gösterilecek bir şey
  // yokken başlık ve "henüz yok" metni bilgi üretmez (Kanso).
  assert.ok(!render.surfaceEmpty.includes("Son kullanılanlar"), "boşken başlık çizilmemeli");
});

test("son kullanılanlar dosya adını gösterir, tam yolu değil", () => {
  const visible = render.surfaceRecents.replace(/<[^>]*>/g, "");
  assert.ok(visible.includes("İŞ SÖZLEŞMESİ.docx"));
  assert.ok(!visible.includes("/Belgeler/"), "tam yol görünür metinde olmamalı");
});

test("kabuk stil dili SaaS dashboard değil", () => {
  const css = readFileSync("src/shared-ui/shell.css", "utf8");
  // Kart yığını ve gradient dili bilinçli olarak dışarıda.
  assert.ok(!/linear-gradient|radial-gradient/.test(css), "gradient kullanılmamalı");
  const shadows = (css.match(/box-shadow/g) || []).length;
  assert.ok(shadows <= 1, `gölge yalnız modal için olmalı, bulunan: ${shadows}`);
});
