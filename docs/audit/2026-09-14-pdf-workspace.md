# Düzenle / PDF çalışma alanı — işlevsel denetim kaydı (2026-09-14)

Giriş noktası kullanıcının üç semptomuydu: *kaydetmiyor · sıkıştırmıyor ·
düzenlemede problem var*. Denetim bu üçüyle sınırlı kalmadı; modül gerçek
kullanıcı zincirleri üzerinden uçtan uca çalıştırıldı.

Yöntem: `crates/ekler-core/tests/pdf_workspace_audit.rs` — 36 test, her biri
`niyet → ToolOperation → run_tool_with_outcome → safe_io → çıktıyı yeniden aç
→ değişmezleri ölç` zincirini izler. Bütün fixture'lar sentetiktir (lopdf ile
üretilir) ve yalnız `tempfile::tempdir()` altına yazılır. Gerçek kullanıcı
belgesi ya da dizini kullanılmadı; `user-data-fingerprint.sh` sözleşmesi
korundu.

## Temel

| | |
|---|---|
| HEAD | `1a8fe00` (main, temiz ağaç) |
| Mimari taban | `be49507` HEAD'in atası ✓ |
| Sürüm | 0.0.1 |
| Motor testleri (önce) | ekler-core + pdf-core: 75 geçti · 0 düştü · 9 atlandı |
| Arayüz (önce) | tsc temiz · 246 vitest · 49 UX sözleşmesi |
| LibreOffice | bu makinede **yok** (`/Applications/LibreOffice.app` bulunamadı) |

## Özellik envanteri (kodda gerçekten var olanlar)

Kabuğun Düzenle yüzeyi, `duzenek_run_pdf_tool` üzerinden `ToolOperation`
çalıştırır: `Select · Reorder · Delete · RotatePages` (sayfa araçları),
`Merge · Compress · raster (PDF → PNG/JPG)` (belge araçları), `Crop ·
Watermark · Number · Images` (nadir). Önizleme `duzenek_preview_pdf_page`
(CoreGraphics). Kaydetme iki yoldan: kaydetme penceresi ya da klasör seçimi;
ikisi de `safe_io::write_new_bytes` (aynı dizinde geçici dosya · fsync ·
`persist_noclobber`) ile yayınlanır ve kabuk çıktıyı tarayıcıyla yeniden açar.

Komut katmanında var olup kabuğun **çağırmadığı** komutlar: `duzenek_convert_office_pdf`,
`duzenek_prepare_export_plan`, `duzenek_execute_export_plan`, `duzenek_prepare_uyap`,
`duzenek_convert_udf_to_md`, `duzenek_split_pdf`, `duzenek_detect_blank_pages`,
proje kaydet/yükle. Bunlar bağımsız DüzenEk'in yüzeyleridir; bu turda
bağlanmadı ve bağlanmadıkları kayda geçti (aşağıda B5).

## Bulunan buglar

### B1 — P1 · ICCBased/SMask görselde sıkıştırma tümden düşüyordu ("sıkıştırmıyor")

- **Semptom:** Sıkıştır → "Sıkıştırma tamamlanamadı".
- **Yeniden üretim:** `[/ICCBased 900 0 R]` renk uzaylı Flate RGB tarama →
  `Compress{Balanced}` → `Failed { "1 görsel bulundu; renk uzayı, maske veya
  filtreleri güvenli sıkıştırma kapsamında değil." }`. SMask'lı görselde aynı.
- **Beklenen:** Gerçek dünyanın normali olan ICCBased RGB/Gray yeniden
  kodlanmalı; kapsam dışı görsel (maske, CMYK, paletli) sıkıştırmayı
  çökertmemeli, dürüstçe `NoBenefit` demeli.
- **Kök neden (iki parça):** (a) `pdf-core::optimizer::decode_image` yalnız
  çıplak `/DeviceRGB`/`/DeviceGray` adını tanıyordu; ofis uygulamaları,
  Preview ve tarayıcılar ICC dizisi yazar. (b) `optimize_pdf` "görsel var ama
  hiçbiri desteklenmiyor" durumunda **hata** döndürüyor, `toolbox` ön ayar
  döngüsü ilk hatada `?` ile bütün işlemi terk ediyordu — temizlik adayı bile
  kayboluyordu.
- **Düzeltme:** `optimizer::decodable_colorspace` (Device adı ya da ICC `/N
  1|3` → Device adı, yalnız ÇÖZMEK için; yazılan akış özgün `/ColorSpace`
  nesnesini korur). Kapsam dışı görsel hata değil, `images_supported`'da
  sayılır; bu yüzden optimize_pdf'ten çıkan her hata gerçek bir bozukluktur
  ve ölümcül kalır (bkz. B8). Bozuk JPEG yine `Failed` (mevcut sözleşme
  korundu).
- **Regresyon testleri:** `compress_icc_based_scan_must_compress_not_fail`,
  `compress_smask_image_falls_back_to_cleanup_or_no_benefit_not_failed`,
  `compress_keeps_original_icc_colorspace_object_on_recompressed_image`,
  `chain_e_merge_reorder_compress`.

### B2 — P1 · Tek sayfa seçimi bütün belgeyi sürüklüyordu

- **Semptom:** 40 sayfalık belgeden 1 sayfa → çıktı kaynağın %100'ü.
- **Kök neden:** `pdf-core::pdf::clone_object_remapping` her referansı derin
  kopyalar. İçindekiler bağlantısının `/Dest`i başka bir sayfaya işaret eder;
  o sayfa ham kopyalanınca `/Parent` üzerinden bütün sayfa ağacı ve tüm
  sayfalar yetim nesne olarak çıktıya taşınıyordu.
- **Düzeltme:** `is_foreign_page_node`: kopyalanan sayfanın dışındaki
  `/Type /Page` ya da `/Pages` hedefine giden referans `null` olur (PDF'te
  geçerli; bağlantı ölü ama zararsız). Aynı sayfaya dönen `/P` id haritasında
  olduğu için yeni sayfaya eşlenir.
- **Regresyon testi:** `select_with_cross_page_link_does_not_drag_whole_document_along`.

### B3 — P1 · Uzantısız kaydetme hedefi "kaydedilemedi" gösteriyordu ("kaydetmiyor")

- **Semptom:** Dosya diskte, kullanıcıya "Kaydedilen kopya yeniden açılıp
  doğrulanamadı."
- **Kök neden:** Kaydetme penceresi uzantıyı garanti etmez; motor uzantısız
  hedefe yazar; kabuğun alım fişi (`scan_source_files`) biçimi **uzantıdan**
  okur ve reddeder.
- **Düzeltme (frontend):** `copyDestination.withPdfExtension` — pencere
  sonucu `.pdf` ile bitmiyorsa tamamlanır (büyük/küçük harf korunur).
- **Regresyon testleri:** Rust `receipt_scan_requires_pdf_extension_so_frontend_must_normalise_destination`
  (boşluğu kanıtlar), vitest `saveDestination.test.ts` (kabuğun kapattığını kanıtlar).

### B4 — P1 · Sil aracı sayfa 1'i önceden işaretli açıyordu ("düzenlemede problem")

- **Semptom:** Sil'e geçince kaydet düğmesi hemen açık; hiçbir şey seçmeden
  kaydedince sayfa 1 çıkıyor.
- **Kök neden:** `build()` her araçta ilk sayfayı işaretler (Seç/Döndür için
  doğru varsayılan); Sil için bu "çıkarılacak sayfa" anlamına geliyordu.
- **Düzeltme (frontend):** `pick('delete')` boş seçimle başlar.
- **Regresyon testi:** vitest `saveDestination.test.ts › silme aracı`.

### B5 — P1 · Düzenle DOCX/UDF/görseli kabul ediyor ama işleyemiyordu

- **Semptom:** Düzenle'de bir DOCX açılınca (LibreOffice varsa) tarama geçer,
  ardından her önizleme "oluşturulamadı", kaydetme "Belge okunamadı".
  LibreOffice yoksa açılış zaten düşer.
- **Kök neden:** Tarayıcı DOCX'i LibreOffice ile *sayfa saymak için*
  dönüştürür; kabuğun PDF çalışma alanı ise önizlemeyi ve her aracı **ham
  yol** üzerinde çalıştırır. Bağımsız DüzenEk'in dönüşüm adımı
  (`duzenek_convert_office_pdf`) kabukta hiç çağrılmıyor.
- **Düzeltme (minimum, dürüst):** Kipin kapısı `["pdf"]`'ye daraltıldı;
  karşılama metni ve son belgeler başlığı buna göre yazıldı. Görseller
  "Görseller → PDF" aracının kendi seçicisinden, Word/UYAP belgeleri
  Dönüştür kipinden girer. Dönüşümün kabuğa bağlanması ayrı bir iş (bkz.
  kalan borç).
- **Regresyon testi:** vitest `saveDestination.test.ts › kipin kapısı`.

### B6 — P2 · NoBenefit mesajı "zaten optimize" diyordu, görseller kapsam dışıyken de

- B1'in düzeltmesiyle kapsam dışı görsel `NoBenefit`'e düşer; eski metin
  ("Bu belge zaten yeterince optimize") yanlış sebep söylerdi.
- **Düzeltme (frontend):** `images_found > 0 && images_recompressed == 0`
  ise "N görsel bulundu; renk uzayı veya maskesi nedeniyle güvenle yeniden
  kodlanamadı."

### B7 — P1 · Öngörücülü Flate görsel çözülmeden JPEG'e kodlanıyordu (şüpheci turu)

- **Semptom:** `/Predictor 2` (TIFF) ya da dizi biçimli `/DecodeParms` taşıyan
  Flate görsel sıkıştırma sonrası görsel olarak yok oluyordu (gerçek
  piksellere göre ortalama fark 113/255).
- **Kök neden:** lopdf yalnız sözlük biçimli `DecodeParms` okur ve yalnız PNG
  öngörücülerini (10–15) geri alır; `decode_image` çözülmemiş deltaları
  piksel sanıyor, kalite kapısı "önce"yi aynı hatalı yoldan çözdüğü için
  farkı göremiyordu. Çıplak DeviceRGB'de önceden de vardı; B1 bunu yaygın
  ICCBased durumuna genişletmişti.
- **Düzeltme:** `decode_image` `DecodeParms`i filtreyle eşleştirip sözlüğe
  indirger; geri alamadığı öngörücü taşıyan görseli dokunmadan atlar;
  `from_raw` öncesi tam uzunluk denetimi (fazla bayt piksel gibi geçemez).
- **Regresyon testleri:** `compress_never_encodes_undecoded_tiff_predictor_deltas`,
  `compress_decodes_array_form_decodeparms_png_predictor_correctly`.

### B8 — P2 · Bozuk görselli belge, temizlik kazandırınca "Compressed" yazılıyordu (şüpheci turu)

- **Semptom:** Bozuk JPEG + 400 KB atılabilir Metadata → çıktı yazıldı,
  bozuk görsel içinde.
- **Kök neden:** B1'in ilk düzeltmesi optimize_pdf hatasını "ön ayarı ele,
  devam et" yapmıştı; kapsam dışı görsel artık hata üretmediği için bu
  devamın tek müşterisi gerçek bozukluklardı.
- **Düzeltme:** optimize_pdf hatası yeniden ölümcül (`?`): bozuk görsel
  taşıyan çıktı hiçbir kazançta yayınlanmaz. `quality_errors` yalnız kalite
  reddi taşır ve %3 kapısında NoBenefit'i Failed'a çevirmez (daha sert ön
  ayar piksel kalitesinden elendi, hafif olanı kazandı ama kazanç küçükse bu
  dürüstçe "kazanç yok"tur).
- **Regresyon testi:** `compress_broken_image_fails_even_when_cleanup_would_win`.

### B9 — P2 · Ham görsel + sıkıştırılamaz balast: NoBenefit yerine Failed (önceden vardı)

- **Kök neden:** `optimize_pdf`'in son `doc.compress()`'i filtresiz görsel
  akışlarını da Flate'liyor; kalite kapısı görsel akışında "bayt bayt aynı"
  kaçışını kullanamayınca temizlik adayı bile düşüyordu.
- **Düzeltme:** son geçişte yalnız görsel OLMAYAN ve filtresiz akışlar
  sıkıştırılır; görseller yukarıda açıkça ele alınır.
- **Regresyon testi:** `compress_raw_image_with_incompressible_ballast_is_no_benefit_not_failed`.

## Bağımsız doğrulama

Beş düzeltme, altı ayrı bakış açısıyla çürütülmeye çalışıldı (renk
fidelity'si · hata semantiği · aday seçimi · referans bütünlüğü · sayfa ağacı
tespiti · yol/durum kenar durumları); her çürütme iddiası ikinci bir bağımsız
ajanla yeniden üretildi. Sonuç: iki gerçek regresyon (B7, B8), bir önceden
var olan kusur (B9) — üçü de düzeltildi ve kilitlendi; F3/F4/F5 çürütülemedi.
Ajanların bıraktığı geçici test dosyası yok (`git status` temiz).

## Doğrulanan ve sağlam bulunan alanlar

- **Yayın:** hedef mevcutsa reddedilir ve dokunulmaz; kaynak SHA-256 her
  işlemde öncesi/sonrası aynı; geçici dosya kalmaz; reddedilen işlem dosya
  bırakmaz; Türkçe/boşluklu/uzun yollar sorunsuz.
- **Sayfa düzenleme:** sıralama (ilk→son, son→ilk), silme (tek/çoklu/hepsini
  reddetme), seçim; yinelenen/eksik permütasyon reddi.
- **Dönüş:** sayfa başına 90/180/270, tekrarlı birikim, mirasla gelen
  `/Rotate` seçim/sıralama/silme/birleştirmede korunuyor, miras üstüne dönüş
  birikiyor (90+90=180). Dönüş `/Rotate` özniteliğidir; MediaBox yeniden
  örneklenmez.
- **Önizleme ↔ çıktı:** döndürülmüş ve yeniden sıralanmış çıktının her
  sayfası, kaynaktan aynı dönüşle üretilen önizlemeyle piksel düzeyinde
  eşleşiyor (MAE < 3).
- **PDF → PNG/JPG:** her sayfa, sırayla, `sayfa-0001…` adıyla; boyut DPI ile
  ölçekli; 270° sayfa dik; açıklamalı sayfa reddi kısmi klasör bırakmıyor.
- **Görseller → PDF:** PNG/JPEG, dik/yatay; A4'e sığdırma.
- **Birleştirme:** 2 ve 3 belge, farklı kutular, miras dönüş; şifreli girdi
  güvenle reddediliyor.
- **Filigran/kırpma/numara:** çıktı yeniden açılıyor; Unicode filigran açıkça
  reddediliyor; sayfayı yok eden kırpma reddediliyor.
- **Negatif girdi:** boş dosya, `.pdf` uzantılı metin, yarım PDF, şifreli PDF
  → çökme yok, kaynak değişmiyor, kısmi çıktı yok, teknik sızıntı yok.
- **İmzalı kaynak:** onaysız reddedilir, onayla yayınlanır.

## Ölçüm matrisi

| Alan | Fixture | Beklenen | Sonuç |
|---|---|---|---|
| Open | 4 sayfa vektör, karışık kutu | yeniden açılır | ✓ |
| Save | seçim 4,2 | geçerli çıktı, sıra 4-2 | ✓ |
| Save | Türkçe/boşluklu klasör | geçerli çıktı | ✓ |
| Save | mevcut hedef | reddedilir, dokunulmaz | ✓ |
| Reorder | 4 sayfa | tam sıra | ✓ (ilk→son, son→ilk) |
| Delete | 4→3 / 4→2 / 4→0 | tam sayı / reddedilir | ✓ |
| Rotate | dik+yatay, 270/90/180, tekrarlı | `/Rotate` doğru, kutu korunur | ✓ |
| Rotate | miras `/Rotate 90` + seçim/sıra/silme | miras korunur | ✓ |
| Parity | dönüş+sıra → PNG vs önizleme | MAE < 3 | ✓ |
| Compress | ICCBased Flate tarama 5,9 MB | küçülür | ✓ 5.901.380 → 714.610 B (%88) |
| Compress | DeviceRGB Flate tarama 5,9 MB | küçülür | ✓ 5.901.133 → 714.442 B (%88) |
| Compress | JPEG q100 | küçülür ya da NoBenefit, asla büyük çıktı | ✓ |
| Compress | vektör belge | NoBenefit, çıktı yok | ✓ |
| Compress | SMask'lı görsel | NoBenefit (Failed değil), çıktı yok | ✓ 850.279 → aday 853.784, 2 görsel/0 kodlandı |
| Compress | yarım PDF | Failed, çıktı yok, sızıntı yok | ✓ |
| Merge | 2 / 3 PDF | tam sıra, kutu, dönüş | ✓ |
| Merge | şifreli girdi | güvenli hata | ✓ |
| PNG | 4 sayfa, 72 dpi | her görsel, sıra, boyut | ✓ |
| JPG | 4 sayfa, 150 dpi | her görsel, sıra, boyut | ✓ |
| Images→PDF | PNG dik + JPEG yatay | 2 sayfa A4 | ✓ |
| Branding | 1 / 10 / 100 sayfa | sınırlı büyüme | ✓ +3.506 B · +492 B/s · +197 B/s (100 s. toplam +19,7 KB) |
| Link bloat | 40 sayfa + içindekiler bağlantısı, 1 sayfa seç | çıktı ≪ kaynak | ✓ (öncesi: %100) |
| Compress | ICC + Flate + TIFF öngörücü 2 | atlanır, bayt dokunulmaz | ✓ |
| Compress | ICC gri + dizi DecodeParms + PNG öngörücü 15 | doğru çözülür ya da atlanır | ✓ |
| Compress | bozuk JPEG + 400 KB Metadata | Failed, çıktı yok | ✓ |
| Compress | ham görsel + 300 KB balast | NoBenefit her düzeyde | ✓ |
| Perf | 100 sayfa ters sıralama | makul | ✓ 51–86 ms |

Not: marka payı her türetilmiş sayfanın **görsel alt kenarına** 34 pt ekler
(dik sayfada `y0` aşağı, 90/270 dönük sayfada x ekseninde). Bu ürün
davranışıdır ve UI'de açıklanır; ölçümler buna göre yapıldı.

## Veri güvenliği

Her testte kaynak SHA-256 işlem öncesi/sonrası karşılaştırıldı; hiçbir işlem
kaynağı değiştirmedi. Hedef mevcutsa `ensure_new_destination` reddeder;
yayın `persist_noclobber` ile atomiktir; başarısız işlem hedefte hiçbir şey
bırakmaz.

## Değişen dosyalar

- `crates/pdf-core/src/optimizer.rs` — `decodable_colorspace`, kapsam dışı
  görsel hata değil, yazılan akış özgün ColorSpace'i korur; DecodeParms
  normalizasyonu ve öngörücü kapısı; tam uzunluk denetimi; son sıkıştırma
  geçişi görsellere dokunmaz.
- `crates/pdf-core/src/pdf/mod.rs` — `is_foreign_page_node` (`/Type` dolaylı
  olsa da çözülür), yabancı sayfa referansı `null`.
- `crates/ekler-core/src/toolbox.rs` — çözüm hatası ölümcül kalır; kalite
  reddi %3 kapısında Failed üretmez; kalite doğrulaması aynı renk uzayı
  indirgemesini kullanır.
- `apps/belge-shell/src/modules/duzenek/copyDestination.ts` — `.pdf` tamamlama.
- `apps/belge-shell/src/modules/duzenek/PdfWorkspace.tsx` — Sil boş seçimle
  başlar; NoBenefit sebebi dürüst.
- `apps/belge-shell/src/shell/modes.ts` — Düzenle yalnız PDF.
- Testler: `crates/ekler-core/tests/pdf_workspace_audit.rs` (36),
  `apps/belge-shell/src/modules/duzenek/saveDestination.test.ts` (4),
  `qa/tests/design-system.test.mjs` (kısaltma toleransı).

## Kalan borç

1. **Ofis/UDF → PDF dönüşümü kabuğa bağlı değil** (B5). Bağımsız DüzenEk'in
   `convert_office_pdf` + onay akışı kabukta yok; LibreOffice keşfi ve
   geçici dosya yaşam döngüsü ayrı bir iş. Bu makinede LibreOffice olmadığı
   için doğrulanamazdı.
2. **Araç değiştirmek düzenlemeyi sessizce sıfırlar** (`pick` → `build`):
   döndürüp Sırala'ya geçen kullanıcı dönüşü kaybeder. Mevcut davranış
   önizleme/çıktı eşitliğini korur (her kayıt tek işlem uygular); durumu
   korumak çok-işlemli tek kayıt gerektirir. P2, tasarım kararı bekliyor.
3. **PDF → görsel klasör adı `DuzenEk-Gorseller-…`** eski ürün adını taşıyor.
   "Output naming" bu turun dokunma listesinde olduğu için değiştirilmedi.
4. `run_tool` Select/Reorder/Delete her sayfayı iki kez kopyalar
   (`extract_page_range` → `merge_documents`); doğru ama gereksiz. Ölçülen
   süre makul (100 sayfa < 100 ms), refactor gerekmiyor.
5. **Seçimde kalan sayfalar arası bağlantılar** artık `null` (önceden ölü +
   şişkin). Doğru çözüm: `merge_documents`/`extract_page_range` içinde kaynak
   başına tek id haritası ve kopyalamadan sonra `/Dest`/`/P` yeniden bağlama.
   P2.
6. **`/Type`süz sayfa sözlükleri** (lopdf'in de sayfa saymadığı) hâlâ derin
   kopyalanır; semptomu üretemez, kayda geçti.
7. Çok büyük DCT görselde (`image` crate 512 MiB ayırma sınırı) çözüm hatası
   Failed üretir; önceden de böyleydi.
