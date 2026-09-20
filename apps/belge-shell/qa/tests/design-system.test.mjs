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
  // Satır ve kontrol yükseklikleri de: %200 metni 28px kutuya sığdırmak kırpar.
  for (const name of [
    "--text-title", "--text-heading", "--text-body", "--text-meta", "--text-label",
    "--row-h", "--control-h", "--toolbar-h",
  ]) {
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

test("boş, hata ve sonuç-yok durumları tek yüzeyden çizilir", () => {
  // Dört ekran aynı bilgi mimarisini dört ayrı biçimde çiziyordu: iki farklı
  // dikey hizalama, iki optik konum, dört CSS bloğu. Tek yüzey kaldı.
  // Home artık kipe özgü bir KARŞILAMA yüzeyi çizer (her kip aynı şablonu
  // paylaşmaz); geri kalan üç boş durum ortak ilkeli kullanır.
  const users = sources.filter((p) => /EmptyState/.test(readFileSync(p, "utf8")));
  assert.ok(
    users.length >= 3,
    `boş durumu olan her ekran ortak ilkeli kullanmalı (şu an ${users.length})`,
  );
  const welcome = readFileSync("src/features/DocumentSurface.tsx", "utf8");
  assert.match(welcome, /mode\.emptyTitle/, "karşılama başlığı kipten gelmeli");
  assert.match(welcome, /mode\.openLabel/, "açma eyleminin adı kipten gelmeli");
  // Eski, ekrana özel boş durum sınıfları geri gelmemeli.
  for (const gone of ["preview-empty", "preview-failed", "review-clear", "dropzone", "empty-primary"]) {
    assert.equal(
      allTsx.filter((s) => s.includes(gone)).length,
      0,
      `ekrana özel boş durum sınıfı geri geldi: ${gone}`,
    );
  }
  // Optik konum tek yerde tanımlı ve KARŞILAMA yüzeyiyle aynı: dört ekran
  // birbirinin metin varyantı gibi durmasın diye kompozisyon tek. Ortalanmış
  // kahraman blok geri gelmemeli — boş durum da sola hizalı üst banttadır.
  const blocks = shell.match(/^\.empty \{[^}]*\}/gmu) ?? [];
  assert.equal(blocks.length, 1, "boş durum tek CSS bloğuyla tanımlanmalı");
  assert.match(blocks[0], /var\(--band-top\)/u, "boş durum ortak üst bandı kullanmalı");
  // Blok da, içindeki her satır da AYNI merkez eksenine oturur.
  assert.match(blocks[0], /align-items: center/u, "boş durum ortalanmalı");
  assert.match(blocks[0], /text-align: center/u, "boş durumun metni de ortalanmalı");
  const welcomeBlock = shell.match(/^\.welcome \{[^}]*\}/gmu) ?? [];
  assert.equal(welcomeBlock.length, 1, "karşılama tek CSS bloğuyla tanımlanmalı");
  assert.match(welcomeBlock[0], /var\(--band-top\)/u, "karşılama aynı bandı kullanmalı");
  assert.match(welcomeBlock[0], /align-items: center/u, "karşılama ortalanmalı");
  assert.match(welcomeBlock[0], /text-align: center/u, "karşılamanın metni de ortalanmalı");
  assert.match(welcomeBlock[0], /margin-inline: auto/u, "karşılama sütunu ortalanmalı");
  // Eylem ve yardımcı satır da aynı eksende: buton bir yerde, ipucu başka
  // yerde duramaz.
  assert.match(shell, /\.welcome-action \{[^}]*align-items: center/su, "eylem de ortalanmalı");
});

test("etkisiz eylem ekranın en ağır öğesi olamaz", () => {
  // Devre dışı birincil düğme dolgulu gri çiziliyordu; koyu temada bu, çevresindeki
  // her şeyden AÇIKTI ve etkisiz eylem ekranın en güçlü öğesi oluyordu.
  const rule = shell.match(/\.btn-primary:disabled \{[^}]*\}/u);
  assert.ok(rule, ".btn-primary:disabled tanımlı olmalı");
  assert.ok(
    !/background:\s*var\(--text-muted\)/u.test(rule[0]),
    "etkisiz birincil düğme metin rengiyle doldurulmamalı",
  );
  assert.match(rule[0], /color:\s*var\(--text-muted\)/u, "etkisiz eylemin metni sönük olmalı");
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

test("Türkçe büyük harf doğru: lang=tr olmadan İ noktasını kaybeder", () => {
  // CSS `text-transform: uppercase` yerel ayara duyarlıdır. lang="tr"
  // olmadan "Erişilebilirlik" ekranda "ERISILEBILIRLIK" olarak çıkar —
  // Türkçe bir hukuk ürününde kabul edilemez. Arayüzde yedi yerde uppercase
  // kullanılıyor, hepsi bu tek özniteliğe bağlı.
  const html = readFileSync("index.html", "utf8");
  assert.match(html, /<html[^>]*lang="tr"/, 'index.html lang="tr" taşımalı');
});

test("belge seçici hatası kullanıcıya görünür", () => {
  // Canlı klavye denemesinde ⌘O, seçici reddedince YAKALANMAMIŞ bir söz reddi
  // üretiyordu: kullanıcı düğmeye basıyor, hiçbir şey olmuyordu. Sessiz
  // başarısızlık, hata mesajından kötüdür.
  const surface = readFileSync("src/features/DocumentSurface.tsx", "utf8");
  // Keyfî bir karakter penceresi değil, GERÇEK gövde: bileşen büyüdükçe
  // pencere kayıyor ve sözleşme sessizce ölçmeyi bırakıyordu.
  const from = surface.indexOf("const browse");
  const to = surface.indexOf("const handledOpen");
  assert.ok(from > 0 && to > from, "belge seçici gövdesi bulunmalı");
  const browse = surface.slice(from, to);
  assert.match(browse, /try \{/, "seçici çağrısı korunmalı");
  assert.match(browse, /catch/, "reddi yakalamalı");
  assert.match(browse, /setRefused/, "kullanıcıya söylemeli");
});

test("modül CSS'i kabuk ilkellerinin sınıf adlarını ezmez", () => {
  // compare.css kuralları kapsamsızdı: .status, .workspace ve .inspector-head
  // kabuğun ilkelleriyle AYNI adı taşıyordu ve yükleme sırasına göre onları
  // uygulamanın her yerinde eziyordu. Karşılaştır kapalıyken bile.
  const shell = readFileSync("src/shared-ui/shell.css", "utf8");
  const names = (css) => {
    const out = new Set();
    for (const m of css.matchAll(/(^|\})\s*([^{}@]+)\{/g)) {
      for (const sel of m[2].split(",")) {
        const t = sel.trim();
        if (t.startsWith(".")) out.add(t.match(/^\.[A-Za-z0-9_-]+/)[0]);
      }
    }
    return out;
  };
  const shellNames = names(shell);
  for (const mod of ["src/modules/degisikis/compare.css", "src/modules/duzenek/pdf.css"]) {
    const clash = [...names(readFileSync(mod, "utf8"))].filter((c) => shellNames.has(c));
    assert.deepEqual(clash, [], `${mod} kabuk sınıflarını eziyor: ${clash.join(", ")}`);
  }
});

test("kısayollar bağlı ve keşfedilebilir", () => {
  // macOS'un öğrettiği iki kısayol. Katalog üretilmedi: keşfedilemeyen bir
  // kısayol profesyonel kullanıcıya da yardım etmez, o yüzden ikisi de
  // tetikledikleri kontrolün ipucunda yazılı.
  const app = readFileSync("src/App.tsx", "utf8");
  assert.match(app, /key: "o"/, "⌘O bağlı olmalı");
  assert.match(app, /key: ","/, "⌘, bağlı olmalı");
  assert.match(app, /enabled: !prefsTab/, "tercihler açıkken ⌘O kapalı olmalı");

  const surface = readFileSync("src/features/DocumentSurface.tsx", "utf8");
  // Birincil eylemin adı kipe göre değişir (PDF Aç / Belge Aç / İlk Belgeyi Aç).
  assert.match(surface, /title=\{`\$\{mode\.openLabel\} {2}⌘O`\}/, "açma eylemi ⌘O'yu göstermeli");
  const sidebar = readFileSync("src/shell/Sidebar.tsx", "utf8");
  assert.match(sidebar, /title="Ayarlar {2}⌘,"/, "Ayarlar ⌘,'i göstermeli");

  // Metin girişindeyken kısayol çalışmamalı.
  const hook = readFileSync("src/shared-ui/useShortcuts.ts", "utf8");
  assert.match(hook, /inTextEntry/, "metin girişinde devre dışı olmalı");
});

test("üretim derlemesinde ham hata konsola da yazılmaz", () => {
  // Konsol da bir yüzeydir: paketlenmiş üründe açılabilir. Ham motor metni
  // yol ve belge adı taşır; ikisi de müvekkil bilgisidir.
  const gate = readFileSync("src/shared-ui/failure.ts", "utf8");
  assert.match(gate, /export function logFailure/u, "tek kapı tanımlı olmalı");
  assert.match(gate, /if \(isDev\(\)\)/u, "ham kayıt yalnız geliştirmede olmalı");
  assert.match(gate, /export function redact/u, "üretim satırı redakte edilmeli");

  for (const file of sources) {
    if (file.endsWith("shared-ui/failure.ts") || file.endsWith("devMock.ts")) continue;
    const body = readFileSync(file, "utf8");
    for (const line of body.split("\n")) {
      if (!/console\.(debug|log|error|warn)\(/.test(line)) continue;
      // Kalan tek istisna: App.tsx'in açılış kayıtları, DEV bloğunun içinde.
      assert.ok(
        /import\.meta\.env\.DEV/.test(body),
        `${file}: konsol kaydı geliştirme bloğuna alınmamış — ${line.trim()}`,
      );
    }
  }
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

test("eski ürün adı hiçbir kullanıcı yüzeyinde kalmadı", () => {
  // Ürün GölgeDosya adını aldı. Eski ad yalnız görünür metinlerde değil,
  // pencere başlığında, paket adında, açılış ekranında ve varsayılan çıktı
  // dosyası adında da kalmamalı. Bu test o yüzeylerin hepsini birden tarar.
  const OLD = "Yüksekbaş Belge";
  const surfaces = {
    "index.html": readFileSync("index.html", "utf8"),
    "package.json": readFileSync("package.json", "utf8"),
    "tauri.conf.json": readFileSync("src-tauri/tauri.conf.json", "utf8"),
    "tokens.css": tokens,
    "shell.css": shell,
    ...Object.fromEntries(sources.map((p) => [p, readFileSync(p, "utf8")])),
    ...Object.fromEntries(
      walk("src")
        .filter((p) => p.endsWith(".css"))
        .map((p) => [p, readFileSync(p, "utf8")]),
    ),
  };
  for (const [name, text] of Object.entries(surfaces)) {
    assert.ok(!text.includes(OLD), `${name} hâlâ eski ürün adını taşıyor`);
  }

  // Paketleme kimliği ve ürün adı birlikte değişmeli.
  const conf = JSON.parse(surfaces["tauri.conf.json"]);
  assert.equal(conf.productName, "GölgeDosya");
  assert.equal(conf.identifier, "tr.yuksekbas.golgedosya");
  assert.equal(conf.app.windows[0].title, "GölgeDosya");

  // Kaydedilen kopyanın varsayılan adı kullanıcıya görünür.
  const pdf = readFileSync("src/modules/duzenek/PdfWorkspace.tsx", "utf8");
  assert.match(pdf, /copyDestination\(`GolgeDosya-\$\{kind\}`/, "çıktı adı ürün adını taşımalı");
  assert.ok(!pdf.includes("DuzenEk-"), "çıktı adında eski modül markası kalmamalı");
});

test("kimlik değişti ama kullanıcı verisi devralınıyor", () => {
  // Kimlik değişince yapılandırma dizini de değişir. Eski dizin okunur,
  // ASLA silinmez; yeni ad altında henüz ayar yoksa eskisi devralınır.
  const paths = readFileSync("src-tauri/src/paths.rs", "utf8");
  assert.match(paths, /APP_ID: &str = "tr\.yuksekbas\.golgedosya"/);
  assert.match(paths, /LEGACY_BELGE: &str = "tr\.yuksekbas\.belge"/);

  const legacy = readFileSync("src-tauri/src/legacy.rs", "utf8");
  assert.match(legacy, /fn migrate_belge/, "önceki birleşik ad bir migration kaynağı olmalı");
  assert.match(legacy, /LEGACY_BELGE/, "kaynak listesine eklenmiş olmalı");
  // Devralma yalnız yeni dosya YOKKEN olur: kullanıcının yeni ayarları ezilmez.
  const fn = legacy.slice(legacy.indexOf("fn migrate_belge"));
  assert.match(fn.slice(0, 1200), /settings_path\(target_dir\)\.is_file\(\)/);
  assert.ok(!/remove_dir|remove_file/.test(fn.slice(0, 1200)), "migration hiçbir şey silmemeli");
});

test("marka varlıkları tek kaynaktan üretiliyor", () => {
  // src-tauri/icons elle düzenlenmez: brand/appicon.svg'den türetilir.
  for (const f of ["brand/appicon.svg", "brand/mark.svg", "brand/build-icons.sh", "brand/BRAND.md"]) {
    assert.ok(statSync(f).isFile(), `${f} bulunmalı`);
  }
  const mark = readFileSync("brand/mark.svg", "utf8");
  const icon = readFileSync("brand/appicon.svg", "utf8");
  // Aynı geometri oranı: kaldırılmış köşe genişliğin %60'ı, yüksekliğin %50'si.
  assert.match(mark, /M12 8 H52 V32 L28 56 H12 Z/);
  assert.match(icon, /M232 176 H792 V512 L456 848 H232 Z/);
  // Kenar çubuğu işareti aynı yolu çizer.
  const icons = readFileSync("src/shell/icons.tsx", "utf8");
  assert.match(icons, /M12 8 H52 V32 L28 56 H12 Z/, "AppMark marka geometrisini taşımalı");
});

test("her ekran bağlamın ÖLÇÜSÜNÜ de söyler", () => {
  // §24: "hangi belge açık" sorusunun ikinci yarısı sayıdır — kaç sayfa, hangi
  // yöne, kaç fark, kaç bulgu. Dördü de aynı yerde, yardımcı barın ortasında.
  // Bu olmadan bar yalnız bir ad taşıyan boş bir bant oluyordu.
  for (const file of [
    "src/modules/duzenek/PdfWorkspace.tsx",
    "src/modules/tavzih/ConvertWorkspace.tsx",
    "src/modules/degisikis/CompareWorkspace.tsx",
    "src/modules/ikincigoz/ReviewWorkspace.tsx",
  ]) {
    assert.match(readFileSync(file, "utf8"), /<ToolbarStatus>/u, `${file}: bar durumu eksik`);
  }
  const chrome = readFileSync("src/shell/chrome.tsx", "utf8");
  assert.match(chrome, /export function ToolbarStatus/u, "durum yuvası tanımlı olmalı");
  // Yuva boşsa hiç yer kaplamaz: "bağlam yoksa ek UI yok".
  assert.match(shell, /\.toolbar-status:empty \{ display: none; \}/u);
});

test("aynı sayının tek evi var", () => {
  // Fark sayacı bir zamanlar üç yerdeydi: panelde bir blok, rayın dibinde
  // "2/4", bir de seçili fark başlığında. Sayaç artık yalnız barda.
  const rail = readFileSync("src/modules/degisikis/ChangeRail.tsx", "utf8");
  assert.ok(!rail.includes("rail-position"), "rayda ikinci bir sayaç kalmamalı");
  const compare = readFileSync("src/modules/degisikis/CompareWorkspace.tsx", "utf8");
  assert.match(compare, /fark`/u, "sayaç barda olmalı");
});

test("son kullanılanlar kipin AÇABİLDİĞİ belgeleri gösterir", () => {
  // Açılamayacak bir satır bilgi değil tuzaktır: tıklanınca reddedilir.
  const surface = readFileSync("src/features/DocumentSurface.tsx", "utf8");
  assert.match(surface, /mode\.extensions\.includes\(extensionOf\(r\.path\)\)/u, "liste süzülmeli");
  assert.match(surface, /mode\.recentTitle/u, "başlık kipten gelmeli");
  // Kayıt yokken "henüz yok" değil, ekranın ne kabul ettiği yazılır.
  assert.match(surface, /formatList\(mode\)/u, "desteklenen türler gösterilmeli");
});

test("iki belge isteyen kip modelini boş durumda da gösterir", () => {
  // Karşılaştır'ın boş durumu diğer üçünün metin varyantı değildir.
  const surface = readFileSync("src/features/DocumentSurface.tsx", "utf8");
  assert.match(surface, /mode\.slots!\[0\]/u, "ilk yuva çizilmeli");
  assert.match(surface, /mode\.slots!\[1\]/u, "ikinci yuva çizilmeli");
  const modes = readFileSync("src/shell/modes.ts", "utf8");
  // Urun dili her yuzeyde ayni: paneller, rapor ve ray zaten "Temel surum" /
  // "Degisik surum" diyordu; bos durum "Belge A / Belge B" diyordu. Kullanici
  // ayni iki belgeye iki ayri ad altinda bakiyordu.
  assert.match(modes, /slots: \["Temel sürüm", "Değişik sürüm"\]/u);
  assert.match(modes, /slotInvites: \["Temel sürümü seç", "Değişik sürümü seç"\]/u);
  // Yalniz KOD satirlari: tarihceyi anlatan yorumun eski adi anmasi dogrudur.
  const code = modes
    .split("\n")
    .filter((line) => !line.trim().startsWith("//") && !line.trim().startsWith("*"))
    .join("\n");
  assert.ok(!/"Belge A"|"Belge B"/u.test(code), "eski 'Belge A/B' dili kalmamali");
  // Yuva kart değildir: gölge ve çerçeve almaz.
  const slot = shell.match(/^\.slot \{[^}]*\}/mu);
  assert.ok(slot, ".slot tanımlı olmalı");
  assert.ok(!/box-shadow|border: 1px/u.test(slot[0]), "yuva kart görünümü almamalı");
});

test("tercihler tek grid: kontroller tek hizada biter", () => {
  // Flex ile yazıldığında her kontrol kendi metnine göre başka yerde bitiyor
  // ve sütun kayboluyordu (§13).
  const field = shell.match(/^\.field \{[^}]*\}/mu);
  assert.ok(field, ".field tanımlı olmalı");
  assert.match(field[0], /display: grid/u, "alan bir grid olmalı");
  assert.match(field[0], /var\(--control-col\)/u, "kontrol kolonu tek yerde tanımlı olmalı");
  assert.match(shell, /\.field > \.btn \{ justify-self: end; \}/u, "düğme kolonun sağında bitmeli");
});

test("geri bildirim var olmayan bir kanal uydurmaz", () => {
  // Ürünün ağ izni yok ve bildirilmiş bir uç noktası yok. Yüzey bunu söyler;
  // sahte bir gönderim, ölü bir düğme veya uydurma bir adres üretmez.
  const settings = readFileSync("src/shell/Settings.tsx", "utf8");
  for (const invented of ["fetch(", "mailto:", "http://", "https://", "XMLHttpRequest"]) {
    assert.ok(!settings.includes(invented), `geri bildirim yüzeyinde uydurma kanal: ${invented}`);
  }
  assert.match(settings, /uygulama içinden gönderim yoktur/u, "durum açıkça yazılmalı");
  // Kapasitede de URL açma izni yok: yüzeyin söylediği şey doğrulanabilir.
  const caps = readdirSync("src-tauri/capabilities").map((f) =>
    readFileSync(join("src-tauri/capabilities", f), "utf8"),
  );
  for (const c of caps) {
    assert.ok(!/opener:allow-open-url|http:default|shell:allow-open/.test(c), "URL açma izni yok");
  }
});

test("arayüzde renkli vurgu yok: renk yalnız semantik ve odak", () => {
  // Hardal/pirinç vurgu kaldırıldı. Vurgu token'ı nötr; marka işareti kendi
  // token'ında ve o da nötr bir tonda.
  const accents = [...tokens.matchAll(/--accent(?:-hover)?:\s*([^;]+);/g)].map((m) => m[1].trim());
  assert.ok(accents.length >= 2, "vurgu token'ları tanımlı olmalı");
  for (const a of accents) {
    assert.ok(
      /^rgb\(\s*(255 255 255|28 28 30)\s*\/\s*\d+%\s*\)$/.test(a),
      `vurgu nötr olmalı, bulunan: ${a}`,
    );
  }
  assert.match(tokens, /--brand-mark:/u, "marka işareti kendi token'ında olmalı");
  const icons = readFileSync("src/shell/icons.tsx", "utf8");
  assert.ok(!icons.includes("var(--accent)"), "marka işareti vurgu token'ını kullanmamalı");
});

test("tipografi dört ölçü: 18 · 14 · 13 · 11.5", () => {
  const sizes = {
    "--text-title": "18px",
    "--text-section": "14px",
    "--text-body": "13px",
    "--text-meta": "11.5px",
  };
  for (const [name, px] of Object.entries(sizes)) {
    const line = tokens.split("\n").find((l) => l.includes(`${name}:`));
    assert.ok(line?.includes(px), `${name} ${px} olmalı — bulunan: ${line?.trim()}`);
  }
});

test("panel başlığı cümle düzeninde: teknik BÜYÜK HARF etiket yok", () => {
  // "DENETİM ÖZETİ" bir sistem çıktısı gibi okunuyordu. Başlık artık panelin
  // neyi gösterdiğini söyler ve bölüm başlığıyla aynı ölçüdedir.
  const rule = shell.match(/^\.inspector-title \{[^}]*\}/mu);
  assert.ok(rule, ".inspector-title tanımlı olmalı");
  assert.ok(!/text-transform/u.test(rule[0]), "panel başlığı büyük harfe çevrilmemeli");
  assert.match(rule[0], /font-size: var\(--text-section\)/u);
  // Arayüzde hiçbir yerde büyük harf zorlaması kalmadı.
  assert.ok(!/text-transform:\s*uppercase/u.test(shell), "kabukta uppercase kalmamalı");
});

test("belge sayfası tema yüzünden karartılmaz", () => {
  // Apple Preview mantığı: chrome koyulaşır, KÂĞIT koyulaşmaz. Koyu temada
  // gerçek PDF sayfasının rengi değişirse kullanıcı belgeyi yanlış görür.
  const pdf = readFileSync("src/modules/duzenek/pdf.css", "utf8");
  assert.match(pdf, /\.pdf-page-surface \{[^}]*background: var\(--surface-document\)/su);
  // İki temada da belge yüzeyi aynı: kâğıt kâğıttır.
  const values = [...tokens.matchAll(/--surface-document:\s*([^;]+);/g)].map((m) => m[1].trim());
  assert.ok(values.length >= 2, "belge yüzeyi iki temada da tanımlı olmalı");
  assert.equal(new Set(values).size, 1, `belge yüzeyi temayla değişmemeli: ${values.join(" / ")}`);
  // Üç yüzey tonla ayrılır: gezgin · kuyu · sayfa.
  assert.match(pdf, /\.thumbnail-list \{[^}]*background: var\(--surface-app\)/su);
  assert.match(pdf, /\.pdf-page-viewport \{[^}]*background: var\(--surface-sunken\)/su);
});

test("boşluk ölçeği sistematik: 4 · 8 · 12 · 16 · 20 · 24 · 32", () => {
  const scale = [4, 8, 12, 16, 20, 24, 32];
  scale.forEach((px, i) => {
    const line = tokens.split("\n").find((l) => l.includes(`--space-${i + 1}:`));
    assert.ok(line?.includes(`${px}px`), `--space-${i + 1} ${px}px olmalı — ${line?.trim()}`);
  });
});

test("boş durum başlığı emir değil, işin adıdır", () => {
  const modes = readFileSync("src/shell/modes.ts", "utf8");
  const titles = [...modes.matchAll(/emptyTitle: "([^"]+)"/g)].map((m) => m[1]);
  // Kip sayisi SABIT YAZILMAZ: Rust ozellik matrisinden okunur. Ikisi ayri
  // dusunce ("Ekler" motora eklendi ama kabukta yok) bu test kirilir.
  const matrix = readFileSync("src-tauri/src/features.rs", "utf8");
  const moduleCount = Number(/const MODULES: \[\(&str, &str, &str\); (\d+)\]/u.exec(matrix)[1]);
  assert.equal(titles.length, moduleCount, "her kipin bir boş durum başlığı olmalı");
  assert.equal(new Set(titles).size, moduleCount, "başlıklar birbirinin kopyası olmamalı");
  for (const t of titles) {
    assert.ok(t.length <= 32, `başlık tek satıra sığmalı: "${t}" (${t.length})`);
    // "…çalışın", "…dönüştürün", "…karşılaştırın": emir kipi bir çalışma notu
    // gibi okunuyordu. Başlık işin ADINI söyler, buyurmaz.
    assert.ok(
      !/(?:ın|in|un[uü]n?|ün|yın|yin)$/u.test(t.split(" ").pop()),
      `başlık emir kipiyle bitmemeli: "${t}"`,
    );
  }
  // Açıklama iki satırı geçmez: 56ch ölçüde ~112 karakter.
  const hints = [...modes.matchAll(/hint:\s*(?:"([^"]+)"|\n\s*"([^"]+)" \+\n\s*"([^"]+)")/g)].map(
    (m) => (m[1] ?? "") + (m[2] ?? "") + (m[3] ?? ""),
  );
  assert.equal(hints.length, moduleCount, "her kipin bir açıklaması olmalı");
  for (const h of hints) {
    assert.ok(h.length <= 120, `açıklama iki satırı aşıyor: "${h}" (${h.length})`);
  }
});

test("büyük harf düzeni tek kural: komut Başlık, bölüm cümle düzeninde", () => {
  // Komut etiketleri (düğme, gezinme) Başlık Düzeni; bölüm başlıkları ve
  // yardımcı metin cümle düzeni. "…" ile biten etiket bir ilerleme cümlesidir
  // ve cümle düzeninde kalır ("PDF hazırlanıyor…").
  const labels = [];
  for (const file of sources.filter((p) => p.endsWith(".tsx"))) {
    const body = readFileSync(file, "utf8");
    for (const m of body.matchAll(/<Button\b[^>]*>\s*([^<>{}]+?)\s*<\/Button>/gs)) {
      labels.push([file, m[1].replace(/\s+/g, " ").trim()]);
    }
  }
  assert.ok(labels.length >= 12, `düğme etiketleri taranmalı (bulunan ${labels.length})`);
  for (const [file, label] of labels) {
    if (label.endsWith("…")) continue;
    for (const word of label.split(" ")) {
      assert.match(
        word,
        /^[A-ZÇĞİÖŞÜ0-9&]/u,
        `${file}: komut etiketi Başlık Düzeninde olmalı — "${label}"`,
      );
    }
  }
  // Bölüm başlıkları cümle düzeninde: yalnız ilk kelime büyük. Kısaltmalar
  // (PDF, UYAP) Türkçede büyük kalır; "Son PDF'ler" cümle düzenidir.
  const modes = readFileSync("src/shell/modes.ts", "utf8");
  for (const m of modes.matchAll(/recentTitle: "([^"]+)"/g)) {
    const rest = m[1].split(" ").slice(1);
    for (const w of rest) {
      assert.match(
        w,
        /^(?:[a-zçğıöşü]|[A-ZÇĞİÖŞÜ]{2,})/u,
        `bölüm başlığı cümle düzeninde olmalı — "${m[1]}"`,
      );
    }
  }
});

test("aynı kavramın tek adı var", () => {
  // Son belgeler listesi karşılama yüzeyinde ve tercihler penceresinde aynı
  // listedir; iki ad iki kavram demektir ve arayüzü amatör gösterir.
  const modes = readFileSync("src/shell/modes.ts", "utf8");
  const settings = readFileSync("src/shell/Settings.tsx", "utf8");
  assert.match(modes, /recentTitle: "Son belgeler"/u);
  assert.ok(!/Son kullanılanlar/.test(settings), "tercihler de aynı adı kullanmalı");
  const visible = (src) => [...src.matchAll(/>([^<>{}]{3,60})</g)].map((m) => m[1]).join("|");
  assert.ok(visible(settings).includes("Son belgeler"), "tercihlerde 'Son belgeler' görünmeli");

  // Çıktı klasörü de tek ad: akışta "Çıktı", tercihlerde "Çıktı klasörü" iki
  // ayrı şey gibi okunuyordu.
  const convert = readFileSync("src/modules/tavzih/ConvertWorkspace.tsx", "utf8");
  const folder = readFileSync("src/modules/tavzih/OutputFolderField.tsx", "utf8");
  for (const [name, src] of [["akış", convert], ["tercihler", folder]]) {
    assert.ok(visible(src).includes("Çıktı klasörü"), `${name} "Çıktı klasörü" demeli`);
  }

  // Menü dili tek elden: kenar çubuğu ile tercih sekmeleri aynı yazımı taşır.
  const sidebar = readFileSync("src/shell/Sidebar.tsx", "utf8");
  for (const src of [sidebar, settings]) {
    assert.ok(src.includes("Geri Bildirim"), "tek yazım: Geri Bildirim");
    assert.ok(!/Geri bildirim<|"Geri bildirim"/.test(src), "ikinci yazım kalmamalı");
  }
});

test("zaman basamakları tek dilde konuşur", () => {
  // "6 sa önce" ile "3 gün önce" aynı listede yan yana durunca liste bir
  // çalışma notu gibi okunuyordu.
  const rel = readFileSync("src/features/relativeTime.ts", "utf8");
  for (const short of [" dk önce", " sa önce"]) {
    assert.ok(!rel.includes(short), `kısaltma kalmamalı: "${short.trim()}"`);
  }
  assert.ok(rel.includes("dakika önce") && rel.includes("saat önce"), "tam kelime kullanılmalı");
  assert.ok(!/month: "short"/.test(rel), "ay adı da kısaltılmamalı");
});
