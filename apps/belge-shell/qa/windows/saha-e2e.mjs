// Kapı 6 — paketlenmiş GölgeDosya, gerçek WebView2, gerçek Windows girdisi.
//
// Saha kullanıcısının Windows'ta yaşadıklarını, kullanıcının kullandığı AYNI
// ikiliyle yeniden üretir: tarayıcı menüsü ("Yenile / Farklı kaydet / Paylaş"),
// F5 / Ctrl+R ile uyarısız sıfırlanma, "Klasörde Göster"in üst klasörü açması,
// PDF önizlemesi (maddeler 2–3) ve ekran görüntüleri (açık/koyu, üç boyut).
//
// Her iddianın bir KONTROLÜ vardır: yeniden yüklemenin ölçülebildiği, tuşların
// pencereye ulaştığı, menü algılayıcının gerçek bir menüyü gördüğü ayrıca
// kanıtlanır. Kontrol düşerse iddia "doğrulanamadı" sayılır, geçti sayılmaz.
//
// Kullanım (Windows):  node qa/windows/saha-e2e.mjs --exe <GolgeDosya.exe> --out <klasör>
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { deflateSync } from "node:zlib";
import { attach } from "./cdp.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(here, "../../../..");
const NATIVE = join(here, "native.ps1");
const argv = process.argv.slice(2);
const arg = (name) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : undefined;
};
const EXE = resolve(arg("--exe") ?? "");
const OUT = resolve(arg("--out") ?? "saha-kanit");
const BASE_PORT = Number(arg("--port") ?? 9322);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Unicode kullanıcı profili: Türkçe harf, boşluk, kesme işareti.
const ROOT = join(process.env.RUNNER_TEMP ?? join(REPO, "target"), "saha");
const HOME = join(ROOT, "Users", "Çağrı Şahin");
const APPDATA = join(HOME, "AppData", "Roaming");
const DOCS = join(HOME, "Documents");
const WORK = join(HOME, "Masaüstü", "Müvekkil'in Dosyası");
const SHOTS = join(OUT, "ekran");
for (const d of [APPDATA, DOCS, WORK, SHOTS]) mkdirSync(d, { recursive: true });

// ---------------------------------------------------------------- sonuçlar
const results = [];
const log = (...a) => console.log(...a);
function record(kind, id, title, ok, detail) {
  results.push({ kind, id, title, ok, detail });
  log(`${kind === "check" ? (ok ? "GEÇTİ " : "DÜŞTÜ ") : "GÖZLEM"} [${id}] ${title} — ${typeof detail === "string" ? detail : JSON.stringify(detail)}`);
}
const check = (id, title, ok, detail) => record("check", id, title, Boolean(ok), detail);
const observe = (id, title, detail) => record("observe", id, title, true, detail);

// ---------------------------------------------------------------- uygulama
let app;
let cdp;
let launches = 0;

function native(action, extra = {}) {
  const params = ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", NATIVE, "-Action", action, "-ProcessId", String(app.pid)];
  for (const [k, v] of Object.entries(extra)) params.push(`-${k}`, String(v));
  const out = execFileSync("powershell.exe", params, { encoding: "utf8", timeout: 60_000 });
  const line = out.trim().split(/\r?\n/).pop();
  return JSON.parse(line);
}

/** Önceki adımlardan kalan uygulama ve WebView2 süreçleri. */
function clearLeftovers() {
  for (const image of ["belge-shell.exe", "GolgeDosya_0.3.0_x64.exe", "msedgewebview2.exe"]) {
    try {
      execFileSync("taskkill.exe", ["/F", "/T", "/IM", image], { stdio: "ignore" });
    } catch {
      // Süreç yok.
    }
  }
}

/** Uzaktan hata ayıklama ucu açılmadıysa neden: WebView2 süreçleri ve komut satırları. */
function diagnose() {
  try {
    return execFileSync(
      "powershell.exe",
      ["-NoProfile", "-Command",
        "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'msedgewebview2|belge-shell|GolgeDosya' } | ForEach-Object { \"$($_.ProcessId) $($_.Name) $($_.CommandLine)\" }; netstat -ano | Select-String 'LISTENING' | Select-String ':93'"],
      { encoding: "utf8", timeout: 60_000 },
    );
  } catch (e) {
    return String(e?.message ?? e);
  }
}

async function launch() {
  // WebView2, aynı kullanıcı verisi klasörünü kullanan ÇALIŞAN bir tarayıcı
  // süreci varsa ona bağlanır ve bizim argümanlarımızı (uzaktan hata ayıklama
  // ucu) YOK SAYAR. Önceki smoke adımları uygulamayı zorla kapattığı için artık
  // süreçler kalabiliyor. Her açılış: temiz süreç, kendi veri klasörü, kendi uç.
  launches++;
  const port = BASE_PORT + launches;
  const userData = join(ROOT, `webview2-${launches}`);
  mkdirSync(userData, { recursive: true });
  clearLeftovers();
  await sleep(1000);
  app = spawn(EXE, [], {
    env: {
      ...process.env,
      APPDATA,
      USERPROFILE: HOME,
      WEBVIEW2_USER_DATA_FOLDER: userData,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-allow-origins=*`,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  app.stderr.on("data", (d) => process.stderr.write(`[uygulama] ${d}`));
  try {
    cdp = await attach(port);
  } catch (e) {
    observe("teşhis", "WebView2 süreçleri", diagnose());
    throw e;
  }
  await cdp.until("document.readyState === 'complete' && !!document.querySelector('.sidebar-nav')", { timeoutMs: 60_000, what: "kabuk hazır" });
  await sleep(800);
}

async function quit() {
  try { cdp?.close(); } catch { /* kapanıyor */ }
  if (app && app.exitCode === null) {
    app.kill();
    await new Promise((r) => app.once("exit", r));
  }
  await sleep(1500);
}

const invoke = (cmd, payload = {}) =>
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)})`);

/** Metni içeren (ya da `exact` ise tam o olan) ilk öğeye tıklar. */
const clickText = (selector, text, exact = false) =>
  cdp.eval(`(() => {
    const t = ${JSON.stringify(text)};
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})]
      .find((e) => ${exact ? "e.textContent.trim() === t" : "e.textContent.includes(t)"});
    if (!el) return false; el.click(); return true;
  })()`);

async function mode(label) {
  const ok = await clickText(".sidebar-nav .sidebar-item", label);
  await sleep(500);
  return ok;
}

async function closeDocument() {
  await clickText("button", "Kapat", true);
  await sleep(400);
}

let shotNo = 0;
function shot(name) {
  const file = join(SHOTS, `${String(++shotNo).padStart(2, "0")}-${name}.png`);
  try {
    native("shot", { Path: file });
  } catch (e) {
    observe("ekran", `görüntü alınamadı: ${name}`, String(e.message ?? e));
  }
  return file;
}

// ---------------------------------------------------------------- fikstürler
function crc32(buf) {
  let c, crc = 0xffffffff;
  for (let n = 0; n < buf.length; n++) {
    c = (crc ^ buf[n]) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return (crc ^ 0xffffffff) >>> 0;
}
/** Bağımlılıksız RGB PNG: sol yarısı koyu, sağ yarısı renkli bir blok. */
function png(width, height, [r, g, b]) {
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (width * 3 + 1)] = 0;
    for (let x = 0; x < width; x++) {
      const o = y * (width * 3 + 1) + 1 + x * 3;
      const dark = x < width / 2;
      raw[o] = dark ? 20 : r;
      raw[o + 1] = dark ? 20 : g;
      raw[o + 2] = dark ? 20 : b;
    }
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(td));
    return Buffer.concat([len, td, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; ihdr[9] = 2; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function edgePdf(html, out) {
  const edge = [
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  ].find(existsSync);
  if (!edge) return false;
  const page = out.replace(/\.pdf$/, ".html");
  writeFileSync(page, html, "utf8");
  try {
    execFileSync(edge, [
      "--headless=new", "--disable-gpu", "--no-first-run", "--no-pdf-header-footer",
      `--user-data-dir=${join(ROOT, "edge-profil")}`,
      `--print-to-pdf=${out}`, pathToFileURL(page).href,
    ], { timeout: 90_000, stdio: "ignore" });
  } catch {
    return existsSync(out);
  }
  return existsSync(out);
}

const F = {
  temiz: join(WORK, "Dilekçe (temiz).docx"),
  hatali: join(WORK, "Dilekçe (hatalı).docx"),
  png1: join(WORK, "görsel 1.png"),
  png2: join(WORK, "görsel'2.png"),
  kendi: join(WORK, "Görsellerden PDF.pdf"),
  makale: join(WORK, "Makale (Edge).pdf"),
  birlesik: join(WORK, "Birleştirilmiş görseller (Edge).pdf"),
  vektor: join(WORK, "vektör.pdf"),
  taranmis: join(WORK, "taranmış.pdf"),
};

function fixtures() {
  copyFileSync(join(REPO, "crates/ikincigoz-core/tests/samples/ornek-dilekce-temiz.docx"), F.temiz);
  copyFileSync(join(REPO, "crates/ikincigoz-core/tests/samples/ornek-dilekce-hatali.docx"), F.hatali);
  copyFileSync(join(REPO, "crates/ekler-core/tests/corpus/vector.pdf"), F.vektor);
  copyFileSync(join(REPO, "crates/ekler-core/tests/corpus/scanned.pdf"), F.taranmis);
  writeFileSync(F.png1, png(620, 877, [200, 40, 40]));
  writeFileSync(F.png2, png(877, 620, [40, 90, 200]));
  const text = Array.from({ length: 40 }, (_, i) =>
    `<p>${i + 1}. Davacı vekili olarak, müvekkilin işçilik alacaklarına ilişkin açıklamalarımızı sunarız; ğüşiöç ĞÜŞİÖÇ.</p>`).join("");
  const makale = edgePdf(`<!doctype html><meta charset="utf-8"><style>body{font:12pt 'Segoe UI';margin:2cm}</style><h1>Makale</h1>${text}`, F.makale);
  const img = (p) => `<img src="${pathToFileURL(p).href}" style="display:block;width:100%;page-break-after:always">`;
  const birlesik = edgePdf(`<!doctype html><meta charset="utf-8"><style>@page{margin:0}body{margin:0}</style>${img(F.png1)}${img(F.png2)}`, F.birlesik);
  observe("fikstür", "Edge ile üretilen dış PDF'ler", { makale, birlesik });
}

// ---------------------------------------------------------------- senaryolar
async function environment() {
  const info = await cdp.eval(`({ ua: navigator.userAgent, dpr: devicePixelRatio, w: innerWidth, h: innerHeight })`);
  observe("ortam", "WebView2 ve pencere", { ...info, window: native("window") });
}

/** Maddeler 2–3: GölgeDosya'nın kendi çıktısı, dış vektör PDF, yalnız görsel PDF. */
async function previews() {
  const made = await invoke("duzenek_run_pdf_tool", {
    paths: [F.png1, F.png2], operation: { kind: "images" }, outputPath: F.kendi, approved: false,
  }).catch((e) => ({ status: "failed", reason: String(e) }));
  check("2", "Görsellerden PDF (GölgeDosya) üretildi", made?.status === "published", made);

  const list = [F.kendi, F.makale, F.vektor, F.birlesik, F.taranmis].filter(existsSync);
  await invoke("remember_documents", { paths: list });
  // Kullanıcının akışı: kapat → yeniden aç → Düzenle.
  await quit();
  await launch();

  for (const file of list) {
    const name = file.split("\\").pop();
    await mode("Düzenle");
    await closeDocument();
    const opened = await clickText(".file-row", name);
    if (!opened) {
      check("2-3", `Son belgelerden açıldı: ${name}`, false, "satır bulunamadı");
      continue;
    }
    let state;
    try {
      await cdp.until(`(() => {
        const err = document.querySelector('.pdf-wait[data-tone="error"]');
        const big = document.querySelector('.pdf-page-viewport img');
        const thumbs = [...document.querySelectorAll('.thumbnail-list img')].filter((i) => i.complete && i.naturalWidth > 0);
        return !!err || (big && big.complete && big.naturalWidth > 0 && thumbs.length > 0);
      })()`, { timeoutMs: 60_000, what: `önizleme: ${name}` });
    } catch { /* aşağıda ölçülür */ }
    state = await cdp.eval(`({
      errors: [...document.querySelectorAll('.pdf-wait[data-tone="error"]')].map((e) => e.textContent),
      thumbs: [...document.querySelectorAll('.thumbnail-list img')].filter((i) => i.complete && i.naturalWidth > 0).length,
      pages: document.querySelectorAll('.thumbnail-image-button').length,
      big: (() => { const i = document.querySelector('.pdf-page-viewport img'); return i && i.complete ? [i.naturalWidth, i.naturalHeight, i.alt] : null; })(),
    })`);
    let switched = null;
    if (state.pages > 1) {
      await cdp.eval(`document.querySelectorAll('.thumbnail-image-button')[1].click()`);
      try {
        await cdp.until(`(() => { const i = document.querySelector('.pdf-page-viewport img'); return i && i.complete && i.naturalWidth > 0 && /sayfa 2$/.test(i.alt); })()`, { timeoutMs: 30_000 });
        switched = true;
      } catch { switched = false; }
    }
    shot(`onizleme-${name.replace(/[^\p{L}\p{N}]+/gu, "-")}`);
    check("2-3", `Önizleme: ${name}`, state.errors.length === 0 && state.thumbs > 0 && state.big && switched !== false,
      { ...state, sayfaDegisti: switched });
  }
  await closeDocument();
}

/** Maddeler 32 / 38 / 26 / 39: tarayıcı yüzeyi. */
async function browserSurface() {
  // Kontrol A: yeniden yükleme ölçülebiliyor mu?
  await cdp.eval(`window.__kanit = 'var'`);
  await cdp.send("Page.reload");
  await sleep(1500);
  await cdp.until("document.readyState === 'complete' && !!document.querySelector('.sidebar-nav')");
  const probeGone = (await cdp.eval(`window.__kanit ?? null`)) === null;
  check("kontrol", "Yeniden yükleme ölçülebiliyor (CDP ile yüklenince iz siliniyor)", probeGone, { probeGone });

  const arm = () => cdp.eval(`(() => {
    window.__kanit = 'var'; window.__tuslar = []; window.__menu = [];
    addEventListener('keydown', (e) => __tuslar.push((e.ctrlKey ? 'Ctrl+' : '') + (e.shiftKey ? 'Shift+' : '') + e.key), true);
    addEventListener('contextmenu', (e) => __menu.push(e.defaultPrevented));
    return true;
  })()`);
  await arm();

  // Sayfanın etkileşimsiz bir noktası.
  const spot = await cdp.eval(`(() => {
    const bad = 'button,a,input,textarea,select,label,[role=button],[tabindex],img,canvas,svg,[contenteditable]';
    for (let y = Math.round(innerHeight * 0.85); y > innerHeight * 0.3; y -= 23)
      for (let x = Math.round(innerWidth * 0.55); x < innerWidth * 0.95; x += 31) {
        const el = document.elementFromPoint(x, y);
        if (el && !el.closest(bad)) return { x, y, dpr: devicePixelRatio, el: el.tagName + '.' + el.className };
      }
    return null;
  })()`);
  // Kontrol B: gerçek tuşlar pencereye ulaşıyor mu? Önce WebView'in klavye
  // odağını almak için etkileşimsiz noktaya bir sol tık.
  const focus = spot
    ? native("click", { X: Math.round(spot.x * spot.dpr), Y: Math.round(spot.y * spot.dpr) })
    : native("focus");
  await cdp.eval(`window.__tuslar = []`);
  native("key", { Keys: "F24" });
  await sleep(600);
  const delivered = (await cdp.eval(`window.__tuslar ?? []`)).includes("F24");
  check("kontrol", "Gerçek klavye girdisi WebView'e ulaşıyor", delivered, { focus, tuslar: await cdp.eval(`window.__tuslar ?? []`) });

  for (const keys of ["F5", "CTRL+R", "CTRL+SHIFT+R", "CTRL+S"]) {
    native("key", { Keys: keys });
    await sleep(2500);
    const tops = native("toplevel");
    const dialogs = tops.windows.filter((w) => w.cls === "#32770");
    let alive = null;
    try { alive = await cdp.eval(`window.__kanit ?? null`); } catch { alive = null; }
    const reached = alive ? await cdp.eval(`window.__tuslar`) : [];
    const item = keys === "CTRL+S" ? "38" : "32";
    check(item, `${keys}: çalışma korunur, tarayıcı işlemi açılmaz`, delivered && alive === "var" && dialogs.length === 0,
      { korundu: alive === "var", sayfayaUlasti: reached, iletisimKutusu: dialogs.map((d) => d.name) });
    if (dialogs.length) native("key", { Keys: "ESC" });
    if (alive !== "var") {
      await cdp.until("document.readyState === 'complete' && !!document.querySelector('.sidebar-nav')").catch(() => {});
      await arm();
    }
  }

  const menuAt = async (point, label) => {
    const x = Math.round(point.x * point.dpr);
    const y = Math.round(point.y * point.dpr);
    const win = native("window");
    const region = { X: win.client.x + x - 60, Y: win.client.y + y - 60, W: 460, H: 560 };
    const before = join(SHOTS, `menu-${label}-once.png`);
    const after = join(SHOTS, `menu-${label}-sonra.png`);
    native("shot", { ...region, Path: before });
    native("rightclick", { X: x, Y: y });
    await sleep(900);
    native("shot", { ...region, Path: after });
    const menus = native("menus");
    const changed = native("diff", { Path: `${before}|${after}` }).changed;
    native("key", { Keys: "ESC" });
    await sleep(400);
    return { menus, changed };
  };

  // Kontrol C: algılayıcı gerçek bir menüyü görüyor mu? (metin alanında menü AÇILMALI)
  await cdp.eval(`(() => { const i = document.createElement('input'); i.id = '__e2e'; i.value = 'deneme';
    Object.assign(i.style, { position: 'fixed', left: '45%', top: '45%', width: '220px', zIndex: 99999 });
    document.body.appendChild(i); return true; })()`);
  const inputAt = await cdp.eval(`(() => { const r = document.getElementById('__e2e').getBoundingClientRect(); return { x: r.x + 20, y: r.y + r.height / 2, dpr: devicePixelRatio }; })()`);
  const control = await menuAt(inputAt, "metin-alani");
  await cdp.eval(`document.getElementById('__e2e').remove()`);
  const detectorWorks = control.menus.items.length > 0 || control.changed > 2000;
  check("kontrol", "Menü algılayıcı gerçek bir menüyü görüyor (metin alanı)", detectorWorks, control);

  if (!spot) {
    check("26/38/39", "Sayfada boş nokta bulunamadı", false, null);
    return;
  }
  const page = await menuAt(spot, "sayfa");
  const prevented = (await cdp.eval(`window.__menu ?? []`)).slice(-1)[0];
  check("26/38/39", "Sayfanın boş yerinde tarayıcı menüsü açılmaz (Yenile/Farklı kaydet/Paylaş)",
    detectorWorks && page.menus.items.length === 0 && page.changed < 2000 && prevented === true,
    { ...page, nokta: spot, olayEngellendi: prevented });
}

/** Madde 13: Klasörde Göster, çıktı klasörünün KENDİSİNİ açmalı. */
async function reveal() {
  await invoke("tavzih_accept_terms");
  const before = readFileSync(F.temiz);
  const result = await invoke("tavzih_convert_file", { path: F.temiz }).catch((e) => ({ error: String(e?.message ?? JSON.stringify(e)) }));
  const outDir = join(DOCS, "GölgeDosya", "Dönüştürülen Belgeler");
  check("13", "Dönüştürme Unicode profilde çıktı üretti, kaynak değişmedi",
    typeof result?.output === "string" && result.output.startsWith(outDir) && Buffer.compare(before, readFileSync(F.temiz)) === 0,
    { output: result?.output, error: result?.error });

  native("close-explorer");
  await sleep(1000);
  const revealed = await invoke("tavzih_reveal_output_folder", { outputs: result?.output ? [result.output] : [] })
    .then(() => "ok").catch((e) => String(e?.message ?? JSON.stringify(e)));
  let windows = [];
  for (let i = 0; i < 20; i++) {
    await sleep(500);
    windows = native("explorer").windows;
    if (windows.length) break;
  }
  const norm = (p) => (p ?? "").toLocaleLowerCase("tr-TR").replace(/\\+$/, "");
  const target = windows.find((w) => norm(w.path) === norm(outDir));
  native("screen", { Path: join(SHOTS, "klasorde-goster.png") });
  check("13", "Klasörde Göster çıktı klasörünün kendisini açar (üst klasörü değil)",
    Boolean(target), { beklenen: outDir, acilan: windows, komut: revealed });
  observe("13", "Dönüştürülen dosya seçili mi", target ? target.selected : null);
  native("close-explorer");
}

/** Madde 10 / §43-A: Ekler paketi hazırlanır, "Klasörü Aç" paketin KENDİSİNİ açar. */
async function annexFolder() {
  const scanned = await invoke("duzenek_scan_source_files", { paths: [F.vektor, F.taranmis] });
  const [a, b] = scanned.sources;
  const project = {
    version: "1.0.0", name: "Dilekçe Ekleri", created_at: "", updated_at: "",
    sources: scanned.sources,
    exhibits: [
      { id: "ek-1", order: 1, name: "Sözleşme", sources: [{ source_id: a.id }] },
      { id: "ek-2", order: 2, name: "Banka Dekontu", sources: [{ source_id: b.id }] },
    ],
    target_size_bytes: 9_961_472,
    stamp_config: { enabled: true, position: "top_right", font_size: 10, margin_pt: 20, show_badge: true },
  };
  const outputDir = join(DOCS, "GölgeDosya");
  mkdirSync(outputDir, { recursive: true });
  const result = await invoke("ekler_prepare_package", { project, outputDir })
    .catch((e) => ({ error: String(e?.message ?? JSON.stringify(e)) }));
  check("10", "Ekler paketi Unicode profilde hazırlandı", typeof result?.package_dir === "string", {
    package_dir: result?.package_dir, outputs: result?.outputs?.length, error: result?.error,
  });
  if (typeof result?.package_dir !== "string") return;
  native("close-explorer");
  await sleep(1000);
  const opened = await invoke("ekler_open_package_folder", { path: result.package_dir })
    .then(() => "ok").catch((e) => String(e?.message ?? JSON.stringify(e)));
  let windows = [];
  for (let i = 0; i < 20; i++) {
    await sleep(500);
    windows = native("explorer").windows;
    if (windows.length) break;
  }
  const norm = (p) => (p ?? "").toLocaleLowerCase("tr-TR").replace(/\\+$/, "");
  check("10", "Klasörü Aç paketin kendisini açar (üst klasörü değil)",
    windows.some((w) => norm(w.path) === norm(result.package_dir)), { beklenen: result.package_dir, acilan: windows, komut: opened });
  native("screen", { Path: join(SHOTS, "ekler-klasoru-ac.png") });
  native("close-explorer");
}

/** Ekran görüntüleri: açık/koyu × üç boyut × her kip; yerleşim ölçüleri. */
async function screenshots() {
  const labels = await cdp.eval(`[...document.querySelectorAll('.sidebar-nav .sidebar-item')].map((b) => b.textContent.trim())`);
  const sizes = [
    { name: "kucuk", w: 900, h: 600 },
    { name: "orta", w: 1280, h: 800 },
    { name: "buyuk" },
  ];
  for (const size of sizes) {
    const geom = size.w ? native("move", { X: 40, Y: 40, W: size.w, H: size.h }) : native("maximize");
    observe("47", `pencere ${size.name}`, geom);
    for (const theme of ["light", "dark"]) {
      await cdp.eval(`document.documentElement.dataset.theme = ${JSON.stringify(theme)}`);
      for (const label of labels) {
        await mode(label);
        await closeDocument();
        shot(`${size.name}-${theme}-${label}`);
      }
    }
  }
  native("move", { X: 40, Y: 40, W: 1280, H: 800 });

  // Madde 1: ürün işaretinin yeri, yerel başlık çubuğunun altında.
  const head = await cdp.eval(`(() => {
    const box = (s) => { const e = document.querySelector(s); if (!e) return null; const r = e.getBoundingClientRect(); return { top: Math.round(r.top), bottom: Math.round(r.bottom), h: Math.round(r.height) }; };
    return { mark: box('.sidebar-mark'), brand: box('.sidebar-brand'), head: box('.sidebar-head'), band: box('.titlebar-band'), nav: box('.sidebar-nav'), platform: document.documentElement.dataset.platform ?? null };
  })()`);
  observe("1", "Kenar çubuğu başı (CSS px)", head);
  check("1", "Windows'ta macOS trafik ışığı bandı yok; işaret bardaki satırda",
    head.platform === "windows" && (head.band?.h ?? 0) === 0 && head.mark && head.mark.top < 40, head);
  await cdp.eval(`document.documentElement.dataset.theme = 'dark'`);
  const scheme = await cdp.eval(`getComputedStyle(document.documentElement).colorScheme`);
  check("8", "Koyu temada tarayıcı parçaları (açılır liste, kaydırma çubuğu) koyu", /dark/.test(scheme), { colorScheme: scheme });
  await cdp.eval(`document.documentElement.dataset.theme = 'light'`);

  // Yüklü durumlar: Karşılaştır, Denetle.
  await invoke("remember_documents", { paths: [F.hatali, F.temiz] });
  await cdp.send("Page.reload");
  await cdp.until("document.readyState === 'complete' && !!document.querySelector('.sidebar-nav')");
  await sleep(800);
  for (const theme of ["light", "dark"]) {
    await cdp.eval(`document.documentElement.dataset.theme = ${JSON.stringify(theme)}`);
    await mode("Karşılaştır");
    await closeDocument();
    await clickText(".file-row", "temiz");
    await sleep(500);
    await clickText(".file-row", "hatal");
    await sleep(3000);
    const scroll = await cdp.eval(`[...document.querySelectorAll('*')].filter((e) => {
      const s = getComputedStyle(e); return /(auto|scroll)/.test(s.overflowY) && e.scrollHeight > e.clientHeight + 1 && e.clientHeight > 0;
    }).map((e) => e.tagName.toLowerCase() + '.' + String(e.className).split(' ').join('.'))`);
    observe("20", `Karşılaştır kaydırma sahipleri (${theme})`, scroll);
    const layout = await cdp.eval(`(() => {
      const body = document.querySelector('.content-body');
      const insp = document.querySelector('.inspector-body');
      const nested = insp ? [...insp.querySelectorAll('*')].filter((e) => {
        const s = getComputedStyle(e); return /(auto|scroll)/.test(s.overflowY) && e.scrollHeight > e.clientHeight + 1;
      }).length : 0;
      const wide = [...document.querySelectorAll('.document-scroll')].filter((e) => e.scrollWidth > e.clientWidth + 1).length;
      const row = document.querySelector('.row-body');
      return {
        loaded: !!document.querySelector('.compare-root'),
        outer: body ? [body.scrollHeight, body.clientHeight] : null,
        nested, wide,
        userSelect: row ? getComputedStyle(row).userSelect : null,
        toggle: !!document.querySelector('.toolbar-switch input[role="switch"]'),
        copy: [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Değişiklikleri Kopyala'),
      };
    })()`);
    if (theme === "light") {
      check("20", "Karşılaştır: dış kaydırma yok, panel içinde ikinci kaydırıcı yok, yatay taşma yok",
        layout.loaded && layout.outer && layout.outer[0] <= layout.outer[1] + 1 && layout.nested === 0 && layout.wide === 0, layout);
      check("28/33", "Karşılaştır: 'Eş zamanlı kaydır' anahtarı, 'Değişiklikleri Kopyala' ve seçilebilir metin",
        layout.toggle && layout.copy && layout.userSelect === "text", layout);
    }
    shot(`karsilastir-yuklu-${theme}`);
    await mode("Denetle");
    await closeDocument();
    await clickText(".file-row", "hatal");
    await sleep(4000);
    shot(`denetle-yuklu-${theme}`);
  }
}

// ---------------------------------------------------------------- akış
async function main() {
  if (!existsSync(EXE)) throw new Error(`exe yok: ${EXE}`);
  fixtures();
  await launch();
  const steps = [
    ["ortam", environment],
    ["önizleme", previews],
    ["tarayıcı yüzeyi", browserSurface],
    ["klasörde göster", reveal],
    ["ekler klasörü", annexFolder],
    ["ekran görüntüleri", screenshots],
  ];
  for (const [name, step] of steps) {
    try {
      await step();
    } catch (e) {
      check("adım", `${name} tamamlanamadı`, false, String(e?.stack ?? e));
    }
  }
  await quit();
}

try {
  await main();
} catch (e) {
  check("adım", "harness", false, String(e?.stack ?? e));
  await quit().catch(() => {});
} finally {
  writeFileSync(join(OUT, "sonuc.json"), JSON.stringify(results, null, 2), "utf8");
  const lines = ["# Kapı 6 — gerçek pencere", "", "| | madde | iddia | ayrıntı |", "|---|---|---|---|"];
  for (const r of results) {
    const mark = r.kind === "observe" ? "·" : r.ok ? "✓" : "✗";
    const detail = (typeof r.detail === "string" ? r.detail : JSON.stringify(r.detail)).replace(/\|/g, "\\|").slice(0, 400);
    lines.push(`| ${mark} | ${r.id} | ${r.title} | ${detail} |`);
  }
  writeFileSync(join(OUT, "sonuc.md"), lines.join("\n") + "\n", "utf8");
  const failed = results.filter((r) => r.kind === "check" && !r.ok);
  console.log(`\n${results.filter((r) => r.kind === "check").length} iddia, ${failed.length} düşen`);
  process.exitCode = failed.length ? 1 : 0;
}
