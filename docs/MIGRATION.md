# Migration kaydı

Bu dosya, birleşme sırasında alınan kararların ve dondurulan durumların kaydıdır.
Her faz kendi bölümünü ekler; önceki bölümler düzeltilmez, üzerine yazılır not eklenir.

---

## Kaynak snapshot — 2026-09-06T20:45Z

| Depo | Yol | Branch | HEAD | Durum |
|---|---|---|---|---|
| Tavzih | `~/Documents/Tavzih` | `main` | `7df3f5dd2775d30a6aec48f2cc84547583c0b396` | temiz |
| İkinciGöz | `~/İkinciGöz` | `main` | `770efb56edafa1b1534cbb8771dc104da1ba1037` | 1 silinmiş `.pyc` (derleme artefaktı) |
| Değişikİş | `~/Documents/ChatGPT/Degisik-Is` | `feature/v0.4.0-brand-refresh` | `d624b343681ff68fe886e51a9410a9f29d319837` | temiz |
| DüzenEk | `~/Desktop/DuzenEk/Kaynak_Kodlari` | `main` | `274ceaeda9b38758ab62a4fec79abca138911ad4` | **22 commit edilmemiş dosya** |

### Değişikİş — canonical commit
`feature/v0.4.0-brand-refresh`, `main`'i **tamamen içeriyor**: merge-base = `main` =
`8482094`, `HEAD..main` boş. Yani main'e merge bir fast-forward olurdu ve hiçbir
commit kaybolmaz.

Merge **yapılmadı**. Gerekçe: bir devam eden marka yenileme dalını `main`'e almak
ürün kararıdır, migration gereği değil. Migration kaynağı açıkça pinlendi:

```
Değişikİş migration source = d624b343681ff68fe886e51a9410a9f29d319837
```

Kullanıcı istediğinde `git checkout main && git merge --ff-only feature/v0.4.0-brand-refresh`
güvenlidir. Dalın kendi `origin`'inin 1 commit önünde olduğu unutulmamalı.

### DüzenEk — Phase 1 blokajı
Çalışma ağacında 11 değişik + 3 yeni test dosyası + 3 rapor + 3 kanıt dizini var
(658 ekleme / 182 silme; `compression_regression_tests.rs` 194 satır,
`toolbox_ux_tests.rs` 269, `workspace_raster_tests.rs` 102). Değişen dosyalar
arasında `pdf/stamp.rs`, `toolbox.rs`, `optimizer.rs` ve `raster.rs` — yani
no-touch bölgelerinin dördü.

Bu durumda `Cargo.toml` ve `crates/tavzih-core/` üzerinde çalışmak Astra'nın
işiyle çakışır. Kural gereği (dirty ağaçta migration başlatma) **hiçbir şeye
dokunulmadı**: reset yok, stash yok, clean yok.

### iCloud kararsızlığı — açık risk
Bu oturum sırasında `~/Desktop/DuzenEk` **iCloud Çöp Kutusu'na taşındı ve geri geldi**.
Ara pencerede tüm dizin `~/Library/Mobile Documents/.Trash/DuzenEk` altındaydı.
Desktop ve Documents iCloud senkronunda; `~/Projects` değil.

Bu, DüzenEk'in kendi `ACCEPTANCE_REPORT.md`'sinde kayıtlı olan sorunun aynısıdır
(*"iCloud initially evicted source/build files and stalled local reads"*).

**Bu yüzden birleşik workspace `~/Projects/Yuksekbas-Belge` altına kuruldu** —
iCloud senkronu dışında.

---

## Phase 2 — birleşik workspace ve minimal kabuk

### Ne kuruldu
* Cargo workspace; sürümler dört uygulamanın **bugün çözdüğü** sürümlere sabit
  (tauri 2.11.5, zip 2.4.2, quick-xml 0.36.2, image 0.25.10, serde 1.0.229).
  Sürüm yükseltmek migration'ın parçası değildir.
* Tauri 2 kabuğu: dört rota (Düzenle / Dönüştür / Karşılaştır / Denetle),
  ortak yerleşim, atlama bağlantısı, tek canlı bölge, odak tuzağı, tema ve
  erişilebilirlik token'ları.
* Feature flag altyapısı — derleme zamanı (cargo feature) + çalışma zamanı
  (`BELGE_DISABLE_*`). Hiçbiri `default` değil.
* Ayar deposu ve **eski ayar migration'ı** (§11).
* Mimari değişmez denetimi, sürüm kapısı, macOS paketleme zinciri.

### Ne kurulmadı — bilinçli
* Router kütüphanesi. Dört sabit rota var; taşınacak dört arayüzün hiçbiri
  bugün router kullanmıyor.
* Ortak "design system". Dört `styles.css` (465 + 713 + 2131 + 694 satır)
  mekanik olarak çevrilmedi. Kabuk yalnız kendi çerçevesini boyar.
* State kütüphanesi. Dört uygulamanın dördü de yalnız `useState` kullanıyor.
* Yeni özellik. Kabuk boş; `the_shell_ships_no_engine_yet` testi bunu kilitler.

### React sürümü kararı
Kabuk **React 18.3.1 + Vite 6.4.3** kullanıyor: Tavzih, İkinciGöz ve DüzenEk
zaten bu sürümde. Değişikİş React 19.2.8 + Vite 7.3.6'da. Phase 5'te ya
Değişikİş 18'e uyarlanır ya da tüm kabuk 19'a çıkar; karar o fazın kendi test
kapısıyla verilir. Üç kanıtlanmış uygulamayı bugün 19'a taşımak gereksiz risk.

### Ayar migration'ı — gerçek makinede doğrulandı
Uygulama ilk açılışta eski konumları okudu:

| Kaynak | Sonuç |
|---|---|
| Tavzih `preferences.json` | `acceptedTerms: 1` aktarıldı |
| İkinciGöz `settings.json` | `highContrast: "on"` aktarıldı |
| İkinciGöz `dictionary.json` / `profiles.json` | bu makinede yok (kullanıcı hiç eklememiş) |
| DüzenEk localStorage | tespit edildi, elle adım olarak raporlandı |
| Değişikİş | işlevsel değeri olan tercih yok |

Eski dizinlerin zaman damgaları değişmedi. `the_legacy_directory_is_never_modified`
testi bunu her çalıştırmada doğrular.

**Kural:** migration boş alanları doldurur, hiçbir değeri ezmez, hiçbir dosyayı
silmez, iki kez çalıştırılabilir. Sürüm kapısı `legacy.rs` içinde `remove_dir_all`
veya `remove_file` bulursa başarısız olur.

### DüzenEk renderer yolu — açık iş
Kullanıcının seçtiği LibreOffice yolu WebView localStorage'ında
(`~/Library/WebKit/tr.yuksekbas.duzenek/…/localstorage.sqlite3`, `duzenek-renderer`
anahtarı, UTF-16LE). Okumak SQLite bağımlılığı gerektirir ve DüzenEk henüz
taşınmadı; bu bağımlılık bugün eklenmedi.

Kaybın gerçek etkisi sınırlı: DüzenEk'in kendi keşif zinciri
(env → `/Applications` → Homebrew → `PATH`) çalışmaya devam eder. Elle seçim
yalnız keşfin yanlış kurulumu bulduğu durumda önemli.

Phase 6 kararı: ya değer okunur, ya da kullanıcıdan **bir kez** yeniden seçmesi
istenir. Dosya hiçbir durumda silinmez.

### İzin yüzeyi
`capabilities/default.json` yalnız kabuğun ihtiyacını taşıyor: dosya seçici,
kaydetme yeri, pencere sürükleme, tema. **fs, shell, http, opener yok.**

Dört modülün izin kümesi `capabilities-planned/` altında gerekçeleriyle bekliyor
ve modülüyle birlikte etkinleşir. Özellikle DüzenEk'in
`opener:allow-open-path: "**"` izni, İkinciGöz'ün "yalnız dosya seçici, başka
hiçbir şey" duruşunu bozmayacak biçimde ayrı tutuluyor.

### Sürüm hattı
İki bağımsız uygulamanın en güçlü kontrolleri birleştirildi:

* **Tavzih'ten** — fail-closed sürüm kapısı; ağ bağımsızlığı (`cargo tree` ile
  transitive TLS taraması dâhil); Rust **ve** npm lisans denetimi; git-kaynaklı
  bağımlılık yasağı; bundle'ı `tauri build` sonrası **kendimiz mühürleme**
  (v1.0.0 mühürsüz yayımlandığı için); notarization sonucunu **çıkış kodundan
  değil metinden** okuma.
* **Değişikİş'ten** — genişletilmiş öznitelik temizliği (`xattr -cr`, iCloud'da
  tutulan bir ağaçta gerçek risk); **karantina gidiş-dönüşü**: DMG'ye
  `com.apple.quarantine` yazılır, mount edilir, içindeki `.app` `spctl --assess
  --type exec` ile denetlenir. İndirilen kopyanın tek gerçek provası budur.

Bugünkü durum: `BELGE_ADHOC=1` ile paket üretiliyor ve **gerçekten mühürleniyor**
(`Sealed Resources version=2`, linker-signed değil). Developer ID imzası ve
notarization, Apple kimlik bilgisi gerektirdiği için çalıştırılmadı.

---

## Açık teknik sorular

1. **Hardened runtime + harici process (Phase 6 önkoşulu).** DüzenEk
   `sandbox-exec`, `soffice`, `sips` ve CoreGraphics kullanıyor. İmzalı ve
   notarize edilmiş bir bundle'dan bunların çalıştığı **hiç denenmedi** —
   DüzenEk'in kendi deposunda `codesign`/`notarytool` geçen tek script yok.
   Çalışmazsa birleşik uygulamanın entitlement duruşu baştan tasarlanır.
   Bunu birleşme sırasında keşfetmek en pahalı senaryodur.
2. **İkinciGöz provenance (Phase 7).** `writeback.rs` `container_path`'e
   dayanıyor; Tavzih'in fidelity modelinde bu alan yok. Parser konsolidasyonu
   ancak bu eklendikten ve `verify_writeback` yeşil kaldıktan sonra düşünülür.
   Bu faz başarısız olursa İkinciGöz kendi parser'ıyla kalır — **kabul edilebilir
   bir sonuç**.
3. **React 18 ↔ 19** (Phase 5).
4. **`convert.rs`'in yeri.** Tavzih'in dosya düzeyi dönüşüm orkestrasyonu
   (964 satır) DüzenEk tarafından kasten alınmamış. `document-core`'a konursa
   DüzenEk'e kullanmadığı bir çıktı politikası dayatılır. Phase 3'te
   `feature-tavzih` altında kalması öneriliyor.
5. **`opener:allow-open-path` kapsamı.** Bugün `"**"`. Kullanıcının seçtiği
   çıktı köküyle daraltılması gerekir.

## Rollback

| Ne | Nasıl |
|---|---|
| Phase 2'nin tamamı | `~/Projects/Yuksekbas-Belge` silinir. Dört uygulama etkilenmez. |
| Tek bir modül (Phase 3+) | `BELGE_DISABLE_<AD>=1` veya cargo feature kapatılır |
| Birleşik uygulamanın ayarları | `~/Library/Application Support/tr.yuksekbas.belge/` silinir; eski dizinler zaten dokunulmamış |

---

## Phase 1 — document-core tekilleştirmesi (2026-09-07)

### Önkoşul: DüzenEk devri
Astra projeden ayrıldı. DüzenEk'in 22 dosyalık commit edilmemiş işi devralındı,
sınıflandırıldı, dört mantıksal commit'e ayrıldı ve
`duzenek-premerge-2026-09-07` olarak tag'lendi.

Canonical DüzenEk artık `~/Projects/DuzenEk` (iCloud dışı). Dondurulmuş kurtarma
snapshot'ı `~/Projects/DuzenEk-Recovery-20260907-0022` (salt-okunur).
`~/Desktop/DuzenEk` olduğu gibi bırakıldı ve `BU-KOPYA-BAYAT.md` ile işaretlendi.

### Yapılan
`crates/document-core` = Tavzih `crates/tavzih-core` @ `7df3f5dd`, 17 dosya
byte-identical. Paket adı `tavzih-core` olarak **korundu**: hiçbir tüketicide
tek bir `use tavzih_core::` satırı değişmedi. Dizin adı hedef mimariyi yansıtır.

DüzenEk'in vendored kopyası silindi; göreli yolla ortak çekirdeği tüketiyor.

### Doğrulama
| | önce | sonra |
|---|---|---|
| DüzenEk testleri | 145 / 0 fail | **77 / 0 fail** |
| document-core | — | **80 birim + 57 korpus / 0 fail** |
| Tavzih standalone | 80 + 57 | 80 + 57 (dokunulmadı) |
| Cargo.lock sürüm değişikliği | — | **yok** |

145 → 77 farkı tam 68'dir: vendored kopyanın modül içi birim testleri her iki
depoda da koşuyordu. Kayıp yok, çift sayım bitti.

### Kalan duplikasyon — dürüst durum
Bugün çekirdek **iki** yerde: `Yuksekbas-Belge/crates/document-core` (otorite) ve
`Documents/Tavzih/crates/tavzih-core` (Tavzih standalone hâlâ kendi kopyasını
kullanıyor). Tek kopyaya Phase 3'te, Tavzih birleşik kabuğa taşınıp standalone
kopyası emekliye ayrılınca ulaşılır.

DüzenEk'in göreli yolu (`../Yuksekbas-Belge/crates/document-core`) **geçicidir**
ve kardeş dizin varsayar; Phase 7'de DüzenEk birleşik workspace'e taşınınca kalkar.

---

## Phase 2/3 — kabuk UX'i ve Tavzih migration'ı (2026-09-07)

### Kabuk UX'i
Kabuk bir geliştirici paneli gibi görünüyordu. Kaldırılanlar: "0/4 bölüm
kullanılabilir", localStorage yolu, bundle kimliği, taşınan ayarların dökümü,
"henüz taşınmadı" metinleri, feature durumu. Hepsi `console.debug`'a taşındı.

Yeni yüzey belge merkezli: kenar çubuğu (Düzenle/Dönüştür/Karşılaştır/Denetle),
büyük "Dosya Aç" daveti, native sürükle-bırak, son kullanılan belgeler.
Taşınmamış bölüm macOS'ta olduğu gibi soluk ve tıklanamaz; gövdede açıklama yok.

`qa/tests/shell-ux.test.mjs` (8 test) bunu kilitler: gerçek bileşenler SSR ile
render edilip **görünür metin** ve kullanıcıya okunan öznitelikler denetlenir.
`data-feature` gibi öznitelikler kasıtlıdır ve ayrıca doğrulanır.

### Tavzih migration'ı
Dönüştür bölümü `feature_tavzih` arkasında kabuğa taşındı. Motor
`document-core`'un `convert` modülüdür; tek satırı değişmedi. Komut imzaları,
hata kodları, Türkçe mesajlar, tek-dönüştürme nöbetçisi ve kullanım koşulları
metni birebir korundu.

Tek adaptasyon: Tavzih'in `prefs.rs`'i yerine kabuğun ortak ayar deposu. Alanlar
ve `TERMS_VERSION` aynı; taşınan kabul geçerliliğini korur.

### Feature/capability tutarlılığı zorunlu
`check-architecture.sh` artık şunu denetliyor: etkin olmayan bir modülün
capability dosyası `capabilities/` içinde duramaz. İzin kodla birlikte gelir,
kodla birlikte gider. Kapalı modülün komutu binary'de hiç bulunmaz.

### Duplikasyon bitti
`crates/document-core` artık çekirdeğin **tek** kopyası:

| | önce | sonra |
|---|---|---|
| Yuksekbas-Belge | var | **var (tek otorite)** |
| Tavzih standalone | var | yok |
| DüzenEk | var | yok |

Tavzih standalone kendi kopyasını emekliye ayırdı ve ortak çekirdeği tüketiyor.
Kendi release gate'i taşımadan sonra da eksiksiz geçiyor: 66 geçen, 0 başarısız.
Kendi kendine yeten son hâli `tavzih-premerge-2026-09-07` ile donduruldu.

### Bilinen kısıt (taşımadan önce de vardı)
Tavzih'in prefs ve terms testleri tek bir gerçek yapılandırma dosyasını paylaşır
ve seri koşulmalıdır. `release-gate.sh` bunu `--test-threads=1` ile çağırıyor;
paralel `cargo test --workspace` bu iki testte hata verir. Regresyon değildir.

---

## Phase 4 — İkinciGöz migration'ı (2026-09-07)

### Baseline
`ikincigoz-premerge-2026-09-07` @ `d9ca082`. 325 passed / 0 failed / 0 ignored
(core 262 · app 15 · golden 5 · precision 22 · robustness 13 · samples 8),
clippy PASS, fmt PASS, npm build PASS.

Ayrı housekeeping commit'i: kazara commit'lenmiş `render-icons.cpython-314.pyc`
takipten çıkarıldı, `.gitignore`'a `__pycache__/` ve `*.pyc` eklendi. Kaynak
kod değişikliğiyle karıştırılmadı.

### Parser durumu — bilinçli bounded duplication
İkinciGöz **kendi DOCX/UDF parser'ını** kullanmaya devam ediyor.
`document-core → AnalysisDocument` projeksiyonuna geçilmedi.

Bu bir eksiklik değil, writeback güvenliğinin bedelidir: `writeback.rs`
`container_path` provenance'ına dayanır ve `document-core`'un sadakat modelinde
o alan yoktur. Parser consolidation ayrı bir fazdır.

Çekirdek **kopyalanmadı**: kaynak standalone deposunda kalıyor ve birleşik
workspace onu göreli yolla tüketiyor. Standalone depo hem referans
implementasyon hem de parity kaynağıdır; parser consolidation tamamlanmadan
silinmeyecek.

### Parity — ölçüldü, varsayılmadı
Yedi test: bulgu sayısı, kural kimliği, severity, kaynak konumu (blok +
codepoint offset), önerilen düzeltme, writeback çıktısı, diskteki tam zincir.

| Fixture | Referans | Birleşik |
|---|---|---|
| ornek-dilekce-hatali.docx | 11 hata / 2 uyarı / 3 inceleme, 16 kural | aynı |
| ornek-dilekce.udf | 4 bulgu, 2 düzeltme, net −2 karakter | aynı |
| ornek-dilekce-temiz.docx | 0 bulgu | aynı |

**Yaşanan tuzak:** `examples/lint.rs` insan için `block_index + 1` yazdırır;
`block_id` ise `p{block_index}`tir. Referans sayıları lint ekran çıktısından
kopyalamak, olmayan bir migration regresyonu uydurur. Bu tur bizzat buna
takıldı ve sözleşme teste gömüldü.

### Bulunan gerçek sorun — ayar kaybı
İkinciGöz hareket tercihini iki alanla ifade eder ve `reduceMotion` "system"
ise karar eski `respectReducedMotion` anahtarına düşer. Birleşik depoda yalnız
`reduceMotion` vardı; `"system" + false` bileşimi ("sistem azaltma dese bile
animasyonları göster") temsil edilemiyordu. Bu ayarı yapmış bir kullanıcı
sessizce tersine bir davranışa geçerdi. Alan eklendi, çözüm kuralı birebir
uygulandı, iki testle kilitlendi.

### Capability
Genişletilmedi: yalnız `dialog:allow-open`, `allow-save`, `allow-message`.
Ağ izni yok, opener yok, harici process yok, telemetri yok.
`check-architecture.sh` feature/capability tutarlılığını zorluyor.

### Kalan duplikasyon
| Katman | Durum |
|---|---|
| `document-core` (DOCX/UDF sadakat) | tek kopya |
| `ikincigoz-core` (analiz + writeback) | tek kaynak, iki tüketici (standalone + birleşik) |
| DOCX/UDF **parser** mantığı | **iki bağımsız implementasyon — bilinçli** |

---

## Phase 5 — Değişikİş migration'ı (2026-09-07)

### Canonical source
`degisikis-premerge-2026-09-07` @ `d624b34`, dal `feature/v0.4.0-brand-refresh`.

**Branch yapısına dokunulmadı.** Dal `main`'i tamamen içeriyor (merge-base = main
= `8482094`, `HEAD..main` boş), yani fast-forward teknik olarak mümkündü.
Yapılmadı: devam eden bir marka yenileme dalını `main`'e almak ürün kararıdır,
migration gereği değil. Dal ayrıca kendi origin'inin 1 commit önünde.

Baseline: vitest 219 passed / 23 dosya · cargo 8 passed · clippy PASS · build PASS.
Taşımadan önce de var olan durum: `cargo fmt --check` iki kozmetik sapma
bildiriyor (`src-tauri/src/lib.rs:111` ve `:125`). Çalışma ağacı temiz olduğu
için bunlar commit'li koddadır ve migration'la ilgisi yoktur; düzeltilmedi.

### Motor — TypeScript'te kaldı
`compare.ts` (1831 satır), extraction yolları (mammoth / pdfjs / fflate),
normalizasyon, view model ve diff arayüz bileşenleri **değiştirilmeden**
kopyalandı. Rust'a taşınmadı, pdfjs lopdf ile değiştirilmedi, normalizasyon
ortaklaştırılmadı.

Motorda yapılan tek değişiklik üç Tauri komut adının modül önekli hâle
getirilmesi ve üç kasıtlı-kullanılmayan parametrenin TypeScript'in `_`
konvansiyonuyla işaretlenmesidir. İkisi de davranışı değiştirmez.

### React 18 ↔ 19 — bütün shell yükseltmesi GEREKMEDİ
Motorun iş mantığı React 18 uyumlu çıktı: React 19'a özgü tek API kullanılmıyor
(`use`, `useOptimistic`, `useActionState`, form action yok); kullanılan
hook'ların hepsi React 16.8+ hook'ları. Testler `renderToStaticMarkup`
kullanıyor, React 19 semantiğine bağlı değil. Vite config'inde Vite 7'ye özgü
bir şey yok.

Tek fark tip katmanında: React 19 `RefObject<T>.current`'ı non-null yaptı. Üç UI
dosyasında ref tipleri, iki sürümde de yapısal olarak eşleşen yerel takma adlara
çevrildi (`reactCompat.ts`). Çalışma zamanı etkisi sıfır.

### CSP ve pdfjs worker — ölçüldü
Worker Vite tarafından **ayrı statik varlık** olarak yayımlanıyor
(`?url` import) ve aynı origin'den servis ediliyor. blob:, data:, CDN veya
`eval` ile worker kurulumu yok.

Tek CSP değişikliği: `worker-src 'self'`. Bu bir gevşetme değildir — aynı
origin worker'ı `default-src 'self'` geri düşüşüyle zaten izinliydi; açık
bildirim gereksinimi belgeliyor ve ileride `default-src` daralırsa worker
sessizce ana iş parçacığına düşmek yerine görünür biçimde bozuluyor.

Üretim bundle'ı, uygulamanın gerçek CSP başlığıyla servis edilip tarayıcıda
sınandı:

| Ölçüm | Sonuç |
|---|---|
| Worker byte-identical (standalone ile) | ✓ aynı SHA-256 |
| Worker URL | `/assets/pdf.worker.min-*.mjs` — aynı origin |
| Worker gerçekten çalışıyor mu | ✓ pdfjs el sıkışması: `{"action":"ready"}` |
| `new Function` / eval | **engellendi** — `unsafe-eval` verilmiyor |
| blob: worker | **engellendi** (CSP ihlali konsola düştü) |
| Dev server referansı | yok |
| Source map / node_modules sızıntısı | yok |

`new Function` bundle'da geçiyor ama bluebird'ün `canEvaluate` korumalı
isteğe bağlı optimizasyonudur; CSP engellediğinde eval'sız yola düşer.
pdfjs'in `_createCDNWrapper` blob yolu yalnız cross-origin worker'da
kullanılır ve bizimki aynı origin.

**Worker üretim binary'sine gömülü.** Tauri varlıkları binary'ye brotli ile
gömdüğü için `.app` içinde dosya olarak görünmez. Ölçüldü: worker `dist`'ten
çıkarılıp yeniden derlendiğinde binary 7.455.280 → 7.191.088 bayta düştü.
264.192 baytlık fark, 1,09 MB'lık worker'ın sıkıştırılmış hâlidir.

### Test muhasebesi — 219 → 190, kayıp açıklandı
| | Test | Neden |
|---|---:|---|
| StartupSplash | 4 | açılış ekranı; standalone kabuk taşınmadı |
| about | 2 | Hakkında paneli; standalone kabuk taşınmadı |
| updateCheck | 16 | birleşik uygulamada updater yok |
| workspaceUx (kalan) | 7 | standalone yerleşim ve kenar çubuğu |
| **emekliye ayrılan** | **29** | konusu taşınmayan koda ait |
| **taşınan** | **190** | hepsi geçiyor |

`workspaceUx`'in kalan 4 testi (swap semantiği ve `ChangeInspector`
erişilebilirliği) konusu taşınan koda ait olduğu için korundu.
Ek olarak 10 yeni parity testi yazıldı → **200**.

### Kalan duplikasyon
Karşılaştırma motoru şu an **iki yerde**: birleşik workspace (bundan sonra
canonical) ve standalone depo (dondurulmuş referans). Standalone, DüzenEk
migration'ı tamamlanana kadar referans/parity/rollback kaynağı olarak
korunuyor; bu bilinçli ve süreli bir duplikasyondur, parity testleriyle
korunmaktadır.
