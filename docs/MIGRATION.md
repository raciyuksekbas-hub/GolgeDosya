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
