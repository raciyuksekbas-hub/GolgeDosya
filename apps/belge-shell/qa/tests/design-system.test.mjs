// Tasarım dilinin sözleşmeleri.
//
// Bu testler görünüşü değil, KULLANICI SÖZLEŞMESİNİ kilitler: erişilebilir ad,
// tek taşıyıcı olmayan renk, ölçeklenebilir metin, tek görsel dil. Piksel veya
// sınıf adı ayrıntısına bağlanmazlar; bir tasarım kararı bu sözleşmeleri
// bozuyorsa tasarım yanlıştır.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const tokens = readFileSync("src/shared-ui/tokens.css", "utf8");
const shell = readFileSync("src/shared-ui/shell.css", "utf8");
const primitives = readFileSync("src/shared-ui/primitives.tsx", "utf8");

function walk(dir) {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}
const sources = walk("src").filter((p) => /\.tsx?$/.test(p) && !/\.test\./.test(p));
const allTsx = sources.filter((p) => p.endsWith(".tsx")).map((p) => readFileSync(p, "utf8"));

test("ikon-only kontrol erişilebilir ad olmadan yazılamaz", () => {
  // IconButton `label`'ı zorunlu tutar ve onu hem tooltip hem aria-label yapar.
  // Tooltip tek başına ekran okuyucuya ulaşmaz.
  assert.match(primitives, /label: string;/, "IconButton label zorunlu olmalı");
  assert.match(primitives, /aria-label=\{label\}/, "label aria-label olmalı");
  assert.match(primitives, /title=\{label\}/, "label tooltip de olmalı");

  // Ham `icon-btn` sınıfı ilkelin dışında kullanılırsa bu güvence delinir.
  const raw = allTsx.filter((s) => /className="icon-btn/.test(s));
  assert.equal(raw.length, 0, "icon-btn yalnız IconButton içinde kullanılmalı");
});

test("anlam hiçbir yerde yalnız renkle verilmez", () => {
  // Bulgu ciddiyeti: şekil + metin etiketi.
  assert.ok(shell.includes(".finding-mark"), "ciddiyet şekli tanımlı olmalı");
  assert.ok(shell.includes(".finding-sev"), "ciddiyet metni tanımlı olmalı");
  // İşaretli aralık: arka plan + alt çizgi.
  const mark = shell.slice(shell.indexOf(".finding-excerpt mark"));
  assert.match(mark, /text-decoration:\s*underline/, "vurgu alt çizgi taşımalı");
  // Durum bildirimi: hata ve başarı birer işaretle eşlenir.
  assert.match(primitives, /MARK = \{[^}]*error:\s*"✕"/s, "hata bir işaret taşımalı");
  assert.match(primitives, /MARK = \{[^}]*success:\s*"✓"/s, "başarı bir işaret taşımalı");
});

test("metin ölçeği kullanıcı ayarına bağlı", () => {
  // Sabit px ile yazılmış gövde metni, metin boyutu ayarını sessizce yok sayar.
  for (const name of ["--text-title", "--text-heading", "--text-body", "--text-meta"]) {
    const line = tokens.split("\n").find((l) => l.includes(`${name}:`));
    assert.ok(line, `${name} tanımlı olmalı`);
    assert.match(line, /var\(--text-scale\)/, `${name} ölçeğe bağlı olmalı`);
  }
});

test("hareket, kontrast ve odak tercihleri uygulanır", () => {
  assert.ok(tokens.includes('[data-motion="reduced"]'), "hareket azaltma");
  assert.ok(tokens.includes('[data-contrast="more"]'), "yüksek kontrast");
  assert.ok(tokens.includes(":focus-visible"), "klavye odağı görünür");
  // Azaltılmış hareket geçişleri gerçekten kaldırmalı.
  const reduced = tokens.slice(tokens.indexOf('[data-motion="reduced"]'));
  assert.match(reduced, /transition-duration:\s*1ms\s*!important/, "geçişler kaldırılmalı");
});

test("hareket sakin: yay/zıplama yok, süre kısa", () => {
  assert.ok(!/cubic-bezier\([^)]*1\.[0-9]/.test(tokens), "aşırı yay eğrisi olmamalı");
  const durations = [...tokens.matchAll(/--motion(?:-fast)?:\s*(\d+)ms/g)].map((m) => +m[1]);
  assert.ok(durations.length >= 2, "hareket süreleri tanımlı olmalı");
  for (const d of durations) assert.ok(d <= 220, `geçiş ${d}ms — 220ms'i aşmamalı`);
});

test("tek görsel dil: modüller kendi paletini veya fontunu getirmez", () => {
  // Karşılaştır bir zamanlar kendi markasını ve Manrope fontunu taşıyordu.
  const compare = readFileSync("src/modules/degisikis/compare.css", "utf8");
  const pdf = readFileSync("src/modules/duzenek/pdf.css", "utf8");
  for (const [name, css] of [["compare.css", compare], ["pdf.css", pdf]]) {
    assert.ok(!/font-family:\s*(Manrope|Inter|"Avenir)/.test(css), `${name} kendi fontunu getirmemeli`);
    // Ham altıgen renk yalnız yeniden bağlama bloğunda olabilir; kural
    // gövdelerinde ürün token'ları kullanılmalı.
    const hexInRules = [...css.matchAll(/#[0-9a-fA-F]{6}\b/g)].length;
    assert.ok(hexInRules <= 3, `${name} içinde ${hexInRules} ham renk — token kullanılmalı`);
  }
});

test("sistem fontu kullanılır, font paketlenmez", () => {
  assert.match(tokens, /--font-ui:\s*-apple-system/, "macOS sistem fontu");
  const fonts = walk("src").filter((p) => /\.(ttf|otf|woff2?)$/.test(p));
  assert.equal(fonts.length, 0, "arayüz kaynaklarında font dosyası olmamalı");
});

test("SaaS dashboard dili yok", () => {
  const css = tokens + shell;
  assert.ok(!/linear-gradient|radial-gradient/.test(css), "gradient olmamalı");
  assert.ok(!/backdrop-filter/.test(css), "cam efekti olmamalı");
  // Gölge yalnız gerçekten yükselen yüzeyde (sheet) ve neredeyse görünmez.
  const shadows = [...css.matchAll(/box-shadow:\s*([^;]+);/g)].map((m) => m[1]);
  for (const s of shadows) {
    if (s.trim() === "none") continue;
    const alpha = s.match(/\/\s*(\d+)%/);
    assert.ok(alpha && +alpha[1] <= 20, `gölge çok güçlü: ${s.trim()}`);
  }
});

test("belge bağlamı kipler arasında korunur", () => {
  const modes = readFileSync("src/shell/modes.ts", "utf8");
  assert.match(modes, /export function carryContext/, "bağlam taşıma tanımlı olmalı");
  // Üç sonucun üçü de ele alınmalı: sessiz düşme yok.
  for (const k of ['kind: "keep"', 'kind: "needsMore"', 'kind: "mismatch"']) {
    assert.ok(modes.includes(k), `${k} sonucu tanımlı olmalı`);
  }
  const app = readFileSync("src/App.tsx", "utf8");
  assert.ok(!/setDocuments\(\[\]\)/.test(app.split("closeDocuments")[0]),
    "gezinme belgeyi sessizce temizlememeli");
});

test("teknik ayrıntı kullanıcı arayüzünde görünmez", () => {
  const forbidden = [
    "localStorage", "configDir", "migratedFrom", "feature_", "bundle",
    "tr.yuksekbas", "ekler_core", "tavzih_", "ikincigoz_core",
  ];
  const ui = allTsx.join("\n");
  // JSX metin düğümlerinde geçmemeli. Kod kimliği (api çağrısı) serbest.
  const textNodes = [...ui.matchAll(/>([^<>{}]{4,})</g)].map((m) => m[1]).join(" ");
  for (const f of forbidden) {
    assert.ok(!textNodes.includes(f), `kullanıcıya görünen metinde "${f}" geçmemeli`);
  }
});
