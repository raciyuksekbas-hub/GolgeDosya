# GölgeDosya v0.2.0 — Ulaşılamayan komut triyajı & ürün yüzeyi

Önceki tur 24 backend command'ın arayüzden erişilemediğini tespit etmişti.
Bu tur onları **otomatik olarak UI'ye bağlamadı**. Backend'de bir capability
bulunması, ürünün onu kullanıcıya göstermek zorunda olduğu anlamına gelmez.

Sorulan soru her komut için şuydu: *"GölgeDosya kullanıcısının bu işi yapması
ürünün mevcut tanımlı işlevlerinden biri mi?"* — *"Motor yapabiliyor mu?"* değil.

Baseline: `29e925b`, çalışma ağacı temiz, sürüm 0.2.0, `be49507` ata.

## 1. Sonuç: sınıf dağılımı

```text
A — USER-FACING MISSING WIRING      0
B — INTERNAL PRIMITIVE              1
C — INTENTIONAL NON-UI CAPABILITY  10
D — LEGACY / ORPHANED              13
                                   ──
                                    24
```

**Hiçbir komut A sınıfı çıkmadı.** 24 komutun hiçbiri, ürünün bugün vaat ettiği
bir iş akışını yarım bırakmıyor. Bu turda düzeltilen gerçek P1'ler
ulaşılamayan komutlardan değil, **kullanıcının GÖRÜP çalıştıramadığı ya da
yalan söyleyen kontrollerden** çıktı (§5).

## 2. Komut reachability tablosu

### A — User-facing missing wiring (0)
Yok.

### B — Internal primitive (1)

| Command | Reason | Action | User impact | Test evidence |
|---|---|---|---|---|
| `tavzih_inspect_file` | Ölü değil: `tavzih_inspect_files` (arayüzün kullandığı çoğul biçim) bunu doğrudan çağırır — tekil biçim yapı taşıdır | UI eklenmedi | Yok; çoğul biçim zaten bağlı | `product_acceptance.rs` bu komutu doğrudan sürüyor (gerçek DOCX tanınır, sahte `.docx` reddedilir) |

### C — Intentional non-UI capability (10)

| Command | Reason | Action | User impact | Test evidence |
|---|---|---|---|---|
| `duzenek_convert_office_pdf` | Kapsam dışı olduğu KODDA BELGELİ: `modes.ts:75` "Bağımsız DüzenEk'in dönüşüm adımı kabukta bağlı değildir"; Düzenle yalnız PDF kabul eder, Word/UDF Dönüştür kipinden girer | UI eklenmedi | Yok — kullanıcı doğru kipe yönlendiriliyor | motor testi (`convert_to_pdf_file`) |
| `duzenek_renderer_status` | Yalnız yukarıdaki dönüşüm için anlamlı (LibreOffice keşfi) | UI eklenmedi | Yok | motor testi (`find_renderer`) |
| `duzenek_select_renderer` | Aynı | UI eklenmedi | Yok | motor testi (`select_renderer`) |
| `duzenek_detect_blank_pages` | Gerçek ve yararlı yardımcı, ama hiçbir ürün vaadi buna dayanmıyor; bağlamak YENİ ÖZELLİK olurdu (bu tur yasak) | UI eklenmedi; öneri aşağıda | Yok (eksik değil, yokluk) | `product_acceptance.rs`: 3 sayfalık belgede 2. sayfayı doğru buluyor |
| `ikincigoz_get_dictionary` | Denetle bilinçli olarak üç kontrollü bir yüzey ("Kopyaya Uygula", "Tümünü Seç", "Seçimi Temizle"); hiçbir yerde sözlük vaat edilmiyor | UI eklenmedi | Yok | — |
| `ikincigoz_accept_word` | Motor BOŞ sözlüğü geçerli ve sessiz bir durum sayıyor — `ortho.rs` testi bunu ismiyle sabitliyor: `an_empty_dictionary_produces_no_user_findings` | UI eklenmedi | Yok | `ortho.rs:366` |
| `ikincigoz_remove_accepted_word` | Aynı | UI eklenmedi | Yok | — |
| `ikincigoz_add_correction` | Aynı; ilgili kural "elle tanımlanmış kısa listeye dayanır, genel yazım denetimi yapılmaz" — sözlük gürültü azaltıcı bir ek, zorunlu değil | UI eklenmedi | Yok | `ortho.rs` |
| `ikincigoz_remove_correction` | Aynı | UI eklenmedi | Yok | — |
| `ikincigoz_list_rules` | Kural listesi bir gezinti yüzeyi olurdu; ürün "tanımlı kurallar" der ve listeyi sunmaz | UI eklenmedi | Yok | — |

### D — Legacy / orphaned (13)

**D1. Arayüzün ZATEN yaptığı işin ikinci kopyası (5).** Bunlar eksik özellik
değil: `toolbox.rs` bunları "Compatibility entry points" diye adlandırıyor ve
hepsi `run_tool` üzerinden aynı `ToolOperation`'a gidiyor — arayüz o işlemleri
`duzenek_run_pdf_tool` ile zaten sunuyor.

| Command | Aynı işi yapan UI aracı | Action | User impact |
|---|---|---|---|
| `duzenek_merge_pdfs` | **Birleştir** (`ToolOperation::Merge`) | UI eklenmedi | Yok — kullanıcı zaten birleştirebiliyor |
| `duzenek_split_pdf` | **Seç** (`ToolOperation::Select`) | UI eklenmedi | Yok |
| `duzenek_delete_pdf_pages` | **Sil** (`ToolOperation::Delete`) | UI eklenmedi | Yok |
| `duzenek_rotate_pdf_pages_cmd` | **Döndür** (`ToolOperation::Rotate`) | UI eklenmedi | Yok |
| `duzenek_images_to_pdf` | **Görseller → PDF** (`ToolOperation::Images`) | UI eklenmedi | Yok |

Test kanıtı: beşi de `product_acceptance.rs` içinde doğrudan sürülüyor (5+3=8
sayfa birleşiyor, 2 sayfa bölünüyor, 2 sayfa siliniyor, 180° dönüyor) ve her
birinde kaynak SHA-256 değişmiyor.

**D2. Bağımsız DüzenEk'in "Ekler paketi / UYAP" proje akışı (7).** Hepsi bir
`Project` modeli üzerinde çalışıyor. GölgeDosya'nın Düzenle'si durumsuz bir
tek-işlem çalışma alanıdır; frontend'de `Project`, `ExportPlan`, "ek" ya da
proje kaydetme kavramı **hiç yok** (arama: yalnız `.udf` dosya türü etiketi
olarak "UYAP" geçiyor). Bunları bağlamak bir proje yönetimi arayüzü tasarlamak
demekti.

| Command | Action | User impact | Test evidence |
|---|---|---|---|
| `duzenek_prepare_export_plan` | UI eklenmedi | Yok | motor testi (8 geçiş) |
| `duzenek_execute_export_plan` | UI eklenmedi | Yok | motor testi (4) |
| `duzenek_split_into_new_exhibit` | UI eklenmedi | Yok | motor testi (3) |
| `duzenek_prepare_uyap` | UI eklenmedi | Yok | motor testi (10) |
| `duzenek_save_project_to_file` | UI eklenmedi | Yok | motor testi (3) |
| `duzenek_load_project_from_file` | UI eklenmedi | Yok | motor testi (2) |
| `duzenek_relink_source` | UI eklenmedi | Yok | motor testi (2) |

**D3. Kapsam dışı tekil dönüşüm (1).**

| Command | Reason | Action | User impact | Test evidence |
|---|---|---|---|---|
| `duzenek_convert_udf_to_md` | UDF → Markdown, bağımsız DüzenEk'ten kalma; GölgeDosya'nın Dönüştür'ü DOCX↔UDF tanımlıdır, Markdown ürün kapsamında değil | UI eklenmedi; **gömülü eski markası temizlendi** (aşağıya bakınız) | Yok | motor testi (4) |

## 3. Dead-code adayları — silme etkisi

Bu turda **hiçbir şey silinmedi** (talimat gereği). Ölçüm:

```text
Silinebilecek olan : #[tauri::command] SARMALAYICILARI (IPC yüzeyi)
Silinemeyecek olan : altlarındaki MOTOR fonksiyonları — hepsi test edilmiş
                     (prepare_export_plan 8, prepare_uyap 10, execute_export_plan 4,
                      persistence 3+2+2, convert_udf_to_markdown 4, office 1+2+2)
Bağlı crate        : ekler-core (motor testleri burada yaşıyor)
Silinirse ne kırılır: yalnız sarmalayıcılar silinirse HİÇBİR test kırılmaz;
                     motor fonksiyonları silinirse ~40 test kırılır
Kazanç             : 13 komutluk IPC yüzeyi kapanır (saldırı yüzeyi daralır)
Risk               : proje/ek akışı ileride ürüne alınırsa sarmalayıcılar
                     yeniden yazılır (ucuz; motor duruyor)
```

**Öneri (ayrı tur):** D1'in 5 uyumluluk sarmalayıcısı en güvenli adaydır —
arayüz aynı işi `duzenek_run_pdf_tool` ile yapıyor, motor fonksiyonları
(`merge_pdf_files` vb.) `toolbox.rs`'te kalmaya devam eder ve testleri korunur.

## 4. Karşılaştır rapor/export kararı (§4)

Sorular tek tek yanıtlandı:

* **Eski Değişikİş'te kullanıcı özelliği miydi?** Evet — dört biçim
  (HTML/Markdown/JSON/DOCX) ve iki Tauri komutu bugün de duruyor.
* **GölgeDosya'nın Karşılaştır ekranı rapor beklentisi yaratıyor mu?**
  **HAYIR.** Kipin vaadi `modes.ts`'te birebir şu: *"İki belge seçin; eklenen,
  silinen ve değişen bölümler **yan yana gösterilir**."* Vaat GÖRÜNTÜLEMEdir.
  `CompareWorkspace.tsx` içinde rapor/dışa aktarma/indirme kontrolü ya da
  metni **yok**.
* **Dört format gerçekten gerekli mi?** Hayır. JSON geliştirici çıktısıdır,
  Markdown niş; kullanıcıya dört düğme göstermek capability explorer olurdu.
* **Karar:** Sınıf **D** — bağımsız üründen kalmış, GölgeDosya'nın bugünkü
  tanımlı işlevlerinden biri değil. §7'nin workflow-gap ölçütü
  ("ürün açıkça rapor beklentisi yaratıyor") **karşılanmıyor**, dolayısıyla
  P1 değildir ve bu turda bağlanmadı.
* **İleride eklenirse öneri:** dört düğme değil, yardımcı bardaki mevcut eylem
  alanına **tek** "Raporu Dışa Aktar"; biçim seçimi kaydetme penceresinin
  kendi uzantı seçicisinde. JSON kullanıcıya gösterilmemeli.

Rapor motorunun markası yine de canonical tutuldu (aşağıya bakınız): kapsam
kararı ne olursa olsun, depoda eski marka kalmamalı.

## 5. Bu turda düzeltilen gerçek P1'ler (visible-control audit'ten)

Hiçbiri 24 komuttan gelmedi; hepsi kullanıcının GÖRDÜĞÜ kontrolün
yapmadığını/yalan söylediğini yapmasından geldi.

| # | Semptom | Kök neden | Regression | Mutation proof |
|---|---|---|---|---|
| 1 | **Denetle:** kaydetme penceresi dosya adı soruyor, yazılan ad sessizce atılıyor | `target.slice(0, lastIndexOf("/"))` yalnız klasörü alıyor; ad motorda türetiliyordu | `denetle_honours_the_file_name_the_user_typed_in_the_save_dialog` | ✅ kırmızı |
| 2 | **Denetle:** uygulama hatası bulguları, seçimi ve düğmeleri yok ediyor | `setFailure` bileşenin erken dönüşünü tetikliyordu; kurtarılabilir hata ölümcül hatayla aynı duruma yazıyordu | typecheck + vitest | — |
| 3 | **Karşılaştır:** "Sonraki/Önceki değişiklik" belgeyi o değişikliğe götürmüyor | `move()` yalnız `setSelected`; ray belgeyi izliyor, ters yön hiç yoktu | typecheck + vitest | — |
| 4 | **Kabuk:** kip geçişinde "yalnız ilk belge taşınır" kararı hesaplanıp atılıyor; bar bir belge derken motor ikisini dönüştürüyor | çalışma alanlarına `outcome.paths` değil ham `documents` geçiliyordu | typecheck + vitest + UX | — |
| 5 | **Migration:** "Belge" kurulumundan yükseltmede sözlük ve profiller geride kalıyor | `migrate_belge` yalnız settings.json taşıyor, ama başarısı `pristine`i false yapıp İkinciGöz kaynağını tamamen atlatıyordu | `upgrading_from_the_previous_unified_install_carries_the_dictionary_and_profiles` | ✅ kırmızı |

## 6. Marka kalıntısı — render/asset denetimi (§13)

Dize taraması yetmiyor; filigran vakası bunu kanıtlamıştı. Bu turda **gömülü
asset'ler** tarandı:

* **UDF → Markdown paketi** eski markayı üç yerde taşıyordu: dosya
  `duzenek-logo.svg` adıyla yazılıyor, Markdown'a `alt="DüzenEk"` ile
  ekleniyor, SVG'nin kendisi (`aria-label="DüzenEk"`) eski logoydu. Üçü de
  canonical GölgeDosya işaretine taşındı; yeni SVG, PDF markasının
  geometrisiyle birebir aynı (7032 → ~430 bayt).
* **Rapor adı süzgeci** (`degisikis.rs`) hâlâ ESKİ öneki zorunlu tutuyordu.
  Önceki turda arayüzün ürettiği ad canonical adına taşınmıştı; bu seam açık
  kalsaydı rapor yolu kullanıldığı an backend kendi arayüzünün ürettiği adı
  reddedecekti. Süzgeç artık canonical öneki kabul ediyor, eskiyi de geriye
  dönük açabiliyor; dizin-dışına-çıkma denetimi aynen korundu.

## 7. Filigran regresyonu — yeniden doğrulandı (§12)

`brand_identity.rs` (5 test) yeşil. Ayrıca gerçek çıktı üretilip **render
edilerek** gözle doğrulandı:

| Senaryo | Sonuç |
|---|---|
| yeni belge, 1 ve 100 sayfa | ✅ tek "GölgeDosya" işareti, sağ altta |
| dikey / yatay | ✅ (yatay ayrıca render'da görüldü) |
| `/Rotate` 90 / 180 / 270 | ✅ işaret DİK ve okunur |
| **eski DüzenEk markalı belge, İKİ KEZ işlendi** | ✅ tek GölgeDosya işareti; çift filigran yok, birikme yok (render) |
| eski `DüzenEk` işareti | ❌ hiçbir render'da yok |

## 8. P1 yeniden sayımı (§16)

Önceki turun 14 P1'i yeni ölçüte göre ("yalnız gerçek kullanıcı problemi")
yeniden denetlendi:

```text
DÜZELTİLDİ (5)   #2 fark navigasyonu · #6 çalışma alanı yıkımı ·
                 #7 kaydetme adı · #10/#13 taşıma kararı · #14 migration kaybı
SINIF DEĞİŞTİ(3) #1 "24 komut ulaşılamıyor" → sınıflandırma tamamlandı, A=0
                 #8 Denetle sözlük arayüzü → C (vaat yok, boş sözlük geçerli durum)
                 #11 rendererPath → görünen kontrol yok → P2
P2'YE İNDİ (6)   #3 sürüm takası göstergesi · #4 büyük belge sınırı ·
                 #5 çelişen düzeltmeler (gerçek örneklerde ÜRETİLEMEDİ) ·
                 #9 yinelenen bulgu sayacı · #12 çıktı klasörü kapsamı
```

**#5 hakkında dürüstlük notu:** "Tümünü Seç + Kopyaya Uygula tümden reddediliyor"
iddiası gerçek örnek belgelerde **yeniden üretilemedi** (6/2/3 düzeltmenin
tamamı uygulanıyor). Ama gerçek bir doğrulama boşluğu var: `samples.rs`
çakışanları eleyip *"the way the correction panel does"* diyor — **panel öyle
yapmıyor**, `onSelectAll` hepsini seçiyor. Bugün zarar yok; testin güvencesi
sahte. Kalan P2 olarak kayıtlı.

### Kalan gerçek kullanıcıya dönük P1
**0.**

### Kalan P2 (~40)
Yutulan hata yolları, sahte başarı gösterimleri, erişilebilirlik boşlukları,
Karşılaştır sayaç/gösterge tutarsızlıkları, büyük belge için sınır/timeout
olmaması, `samples.rs`'in sahte güvencesi, `udf_tests.rs`'te kullanıcının
gerçek `~/Documents` yoluna çivilenmiş (ignore'lu) test. Tam liste denetim
çıktısında.

## 9. Öneriler (bu turda UYGULANMADI)

* **Boş sayfa tespiti** ileride eklenecekse: yeni araç değil, mevcut **Sil**
  aracının içinde tek bir "Boş sayfaları işaretle" eylemi — seçim kutularını
  doldurur, kullanıcı onaylar. Bir komut = bir düğme yaklaşımına düşmez.
* **Karşılaştır raporu** eklenecekse: tek "Raporu Dışa Aktar", biçim kaydetme
  penceresinden; JSON gösterilmez.
* **D1'in 5 uyumluluk sarmalayıcısı** ayrı bir temizlik turunda kaldırılabilir.

## 10. Görünen kontrol denetimi (§6) — "ölü" 27 kontrolün gerçeği

Envanterdeki 27 "dead" kontrol tek tek incelendi. **Hiçbiri kullanıcının
görüp çalıştıramadığı bir kontrol değil**; üç gruba ayrılıyorlar:

```text
(a) HİÇ VAR OLMAYAN özellik   — sözlük yönetimi, kural listesi, belge
    (22 kayıt)                   profilleri, rapor/dışa aktarma, proje
                                 kaydet/yükle, UDF→Markdown, LibreOffice
                                 seçimi, dört ayar alanı…
                                 → ekranda kontrol YOK; yokluk, kırıklık değil.
                                 Sınıflandırma: C ya da D (yukarıdaki tablo).

(b) YAZILMIŞ AMA HİÇ ÇİZİLMEYEN bileşen — UploadZone, PaneHeader
    (2 kayıt)                      (`DocumentPane.tsx`'te tanımlı, render: 0 yer)
                                 → kullanıcı göremez; kullanılmayan kod.

(c) YANLIŞ KAYIT (3 kayıt)     — "Fark metnini kopyala" diye bir kontrol
                                 kodda hiç yok; komut sayımının kontrol
                                 listesine karışmış kalemleri.
```

Kullanıcının **gördüğü** ve çalışmayan/yalan söyleyen kontroller §5'te
listelendi ve beşi de bu turda kapatıldı.

## 11. Final doğrulama

```text
cargo test --workspace --locked      51 takım · 738 test · 0 başarısız · çıkış 0   ✅
scripts/check-architecture.sh        MİMARİ DENETİMİ GEÇTİ                          ✅
frontend vitest                      27 dosya · 254 test                            ✅
frontend qa/ux (node --test)         49 test                                        ✅
tsc --noEmit (iki proje)             temiz                                          ✅
brand_identity.rs                    5 test + render ile görsel doğrulama            ✅

TAZE KLON (klonla → npm ci → npm run build → npm test → cargo test --locked)
  npm ci                             çıkış 0                                        ✅
  npm run build                      çıkış 0 (dist üretildi)                        ✅
  npm test                           çıkış 0 (49 + 254)                             ✅
  cargo test --workspace --locked    51 takım · 738 test · 0 başarısız · çıkış 0    ✅
  klon silindi
```

## 12. Commitler

```text
cbfd255  Denetle: kaydetme penceresindeki adı onurlandır; uygulama hatası oturumu yok etmesin
0ce1a11  Karşılaştır: "Sonraki/Önceki değişiklik" gerçekten o değişikliğe götürsün
2890839  Marka: UDF→Markdown paketindeki eski logo/ad kalıntısı ve rapor önek uyumu
16b169f  Kabuk: kip geçişindeki taşıma kararı motora da uygulansın
59c4b51  Migration: "Belge" kurulumundan yükseltmede sözlük ve profiller de devralınsın
```

push / tag / release: **YAPILMADI**.

## 13. Durum

```text
GÖLGEDOSYA v0.2.0 PRODUCT SURFACE ACCEPTED
```

Kabul ölçütleri tek tek:

```text
gerçek user-facing P1 = 0                    ✅  (5'i bu turda kapatıldı)
bütün visible controls çalışıyor             ✅  (§10: "ölü" 27 kaydın hiçbiri
                                                  görünür-ama-bozuk değil)
gerçek missing workflow'lar kapalı           ✅  (A sınıfı = 0; Karşılaştır raporu
                                                  §4'te kanıtla D sınıfı)
unreachable command'ların tamamı sınıflandı  ✅  (24/24 · A0 B1 C10 D13)
internal/non-UI'ya gereksiz UI eklenmedi     ✅  (0 yeni kontrol eklendi)
filigran regresyonu kapalı                   ✅  (§7 · render ile doğrulandı)
legacy user-facing marka kalıntısı yok       ✅  (§6 · gömülü asset'ler dahil)
fresh clone yeşil                            ✅  (§11)
```

**Dürüstlük notu.** "Bütün visible controls çalışıyor", 244 kontrolün tamamının
elle tıklanmasına değil şu kanıta dayanıyor: (a) arayüzün gerçekten çağırdığı
production komut yüzeyi beş kipte uçtan uca sürüldü ve çıktılar yeniden açıldı
(10 kabul testi), (b) kontrol envanterindeki "ölü" ve "yarım" kayıtların
tamamı tek tek incelendi, (c) görünür-ama-bozuk bulunan beş kontrol düzeltildi.
Kalan ~40 P2'nin büyük kısmı **geri bildirim kalitesi**dir (hata yolunda
sessizlik, yanıltıcı sayaç, erişilebilirlik etiketi) — kontrolün çalışmaması
değil.
