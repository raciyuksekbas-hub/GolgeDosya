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

## Kullanıcı raporu — "Sıkıştırma tamamlanamadı. Belge okunamadı" (29ffb73 üzerinde)

Kullanıcı `29ffb73` derlemesinde gerçek bir sözleşmeyi (12 sayfa, vurgulu)
sıkıştırmak istedi ve "Sıkıştırma tamamlanamadı. Belge okunamadı; kopya
oluşturulamadı. Farklı bir kopya deneyin." aldı. Belge açılıyor ve
önizleniyordu — PDF sağlamdı. Gerçek müvekkil belgesi kullanılmadı: mesaj
motorun `InvalidPdf` önekinin ("Bozuk veya geçersiz PDF dosyası: …") kabuktaki
karşılığı olduğundan, sıkıştırma zincirinde `InvalidPdf` üretebilecek her şekil
sentetik fixture ile tek tek denendi. 19 aday şeklin 10'u belirtiyi birebir
üretti; iki ayrı kök neden ailesi çıktı (B10, B11).

İkinci turda tahmin yerine **gerçek üreticilerin çıktısı** tarandı: aynı
sentetik sözleşme Chrome/Skia, macOS Quartz ve PDFKit ile basıldı, Word'ün ve
Acrobat'ın fiziksel yerleşimi elle kuruldu (15 belge, aşağıdaki matris).
Kullanıcının desenini — belge açılıyor, 12 sayfa önizleniyor, yalnız Sıkıştır
düşüyor — birebir üreten tek aile **B13** oldu: artımlı güncellenmiş dosya.
Word'ün "PDF olarak kaydet" çıktısı bu yapıdadır; ekran görüntüsündeki
sözleşme için en olası neden budur. Önizleme ekranı açıklamaları çizmediği
için B10'un (Önizleme/Acrobat notu) da dışlanamayacağı not edildi.

### B10 — P0 · Marka payı denetimi sağlam belgeyi "bozuk PDF" diye reddediyordu (kaydetme DE)

- **Semptom:** Sıkıştır, Yeni PDF Kaydet, Klasör Seçerek Kaydet, filigran ve
  sayfa numarası — marka payı eklenen **her** türetme — "Belge okunamadı".
  Kullanıcının ilk günden bildirdiği "kaydetmiyor" şikâyetinin de bir kaynağı.
- **Yeniden üretim (hepsi `InvalidPdf`):** sağ kenarı aşan bağlantı; vurgu +
  sayfanın sağındaki popup (gözden geçirilmiş sözleşmelerin tipik yapısı);
  `/Annots` içinde `null` ("An object does not have the expected type");
  dolaylı `/Rect` (aynı); `/Rect`'siz açıklama ("A required dictionary key was
  not found"); 90'ın katı olmayan `/Rotate` ("Geçersiz sayfa dönüşü").
- **Kök neden:** `pdf-core::pdf::stamp::apply_stamp_to_page` payı eklemeden
  ÖNCE her açıklamanın sayfa kutusunun dört kenarından da içeride olmasını
  şart koşuyordu. Oysa pay tek bir kenara eklenir (dik sayfada alt, 90°'de
  sağ…); sağdaki bir popup alt şeritle asla görünür olmaz. Denetim ayrıca
  `map_err(pdf_error)?` ile her okuma hatasını ölümcül yapıyordu.
- **Beklenen değişmez (korundu):** pay, daha önce görünür alanın dışında kalan
  ve GERÇEKTEN çizilen bir açıklamayı açığa çıkaramaz.
- **Düzeltme:** denetim payı ekledikten SONRA ve yalnız yeni açılan şeride
  bakar (`revealed_strip`, `annotation_would_be_revealed`). Görüntüleyicinin
  yerleştiremeyeceği girdi (null, sözlük olmayan, `/Rect`'i eksik ya da sayı
  olmayan) açığa çıkamaz; `/F` Hidden/NoView zaten görünmez; görünüm akışı
  olmayan, kenarlık kalınlığı 0 olan bağlantı (Word'ün yazdığı biçim) hiçbir
  şey çizmez. `/Rect` ve öğeleri çözülür, köşe sırası normalize edilir, saç
  teli temas sayılmaz. Uyumsuz `/Rotate` pdf.js gibi 0 sayılır; sayfanın kendi
  değerine dokunulmaz.
- **Gerçek üreticiyle doğrulama:** Quartz sözleşmesine PDFKit (Önizleme'nin
  motoru) ile vurgu + not eklendi. `29ffb73`'te Sıkıştır, Sırala ve Seç'in
  hepsi bu hatayla düşüyor; düzeltmeyle hepsi geçiyor.
- **Ek kural (aynı tarama):** PDFKit ve Acrobat not popup'ını sayfanın sağına,
  kutunun dışına koyar (`/Open` yok). 90° dönük sayfada pay tam o kenara
  eklendiği için kapalı popup yüzünden işlem reddediliyordu. Spec 12.5.6.14:
  popup'ın kendi görünümü yoktur, `/Open true` değilse hiçbir şey çizmez →
  açığa çıkamaz. Açık popup ve (spec dışı) görünüm akışlı popup çizilebilir
  sayılır; değişmez korunur.
- **Regresyon testleri:** `branding_does_not_refuse_annotations_that_the_gutter_cannot_reveal`,
  `branding_tolerates_annotation_entries_a_viewer_cannot_place`,
  `branding_still_refuses_a_visible_annotation_the_gutter_would_reveal` (değişmez;
  dik ve 90° dönük sayfada yön), `branding_places_mark_on_nonconforming_rotation_instead_of_refusing`,
  `branding_ignores_closed_popup_but_guards_open_popup_on_rotated_page`.

### B11 — P1 · Sıkıştırmada tek bir görsel ayrıntısı bütün belgeyi düşürüyordu

- **Semptom:** Sıkıştır → "Belge okunamadı"; kaydetme etkilenmiyor.
- **Yeniden üretim:** örnek verisinin sonunda bir satır sonu baytı (Flate ya
  da ham; bazı üreticiler `/Length`'e katar) → "Görsel çözümlenemedi: örnek
  uzunluğu boyutla uyuşmuyor"; dolaylı `/Width` → "An object does not have
  the expected type".
- **Kök neden:** B7'de eklenen tam uzunluk denetimi her uyuşmazlığı ölümcül
  hata yapıyordu (bu turun kendi gerilemesi); `decode_image` belgeyi görmediği
  için dolaylı `Width/Height/BitsPerComponent` çözülmüyordu.
- **Düzeltme:** sonda ≤2 bayt fazlalık (LF/CRLF) kırpılır; açıklanamayan
  uzunluk görseli ATLATIR (baytları olduğu gibi kalır), belgeyi düşürmez;
  boyut girdileri optimizer'da ve kalite kapısında çözülür. B7'nin koruması
  (geri alınmamış öngörücü artığı piksel gibi kodlanamaz) ve B8'in sözleşmesi
  (hiçbir çözücünün açamadığı görsel → Failed) aynen korundu.
- **Regresyon testleri:** `compress_tolerates_trailing_eol_and_indirect_dimensions_with_fidelity`
  (sıkıştırılır VE yeniden kodlanan görsel kaynağa sadık, MAE < 12),
  `compress_skips_image_with_unexplained_length_instead_of_failing_document`.

### B12 — P2 · Kabuk, belgenin okunduğu retleri "belge okunamadı" diye çeviriyordu

- Motor B10'un kalan meşru reddini, "damga sığmıyor" reddini ve açıklamalı
  sayfanın görsele dönüşüm reddini de "Bozuk veya geçersiz PDF" / "Desteklenmeyen
  dosya biçimi" önekiyle taşıyor; kabuk bunları "Belge okunamadı; farklı bir
  kopya deneyin" ya da "Yeniden deneyin" diye gösteriyordu — yanlış sebep,
  işe yaramaz öneri.
- **Düzeltme (frontend):** `describeSaveFailure`'a genel kategoriden önce üç
  özgül kategori. Gerçekten okunamayan belge hâlâ "Belge okunamadı" der.
- **Regresyon testi:** vitest `failure.test.ts › belge OKUNDUĞU hâlde yapılan ret…`.

### B13 — P0 · Artımlı güncellenmiş belge (Word çıktısı dâhil) sıkıştırılamıyor, döndürülemiyordu

- **Semptom:** belge açılıyor, önizleniyor, sayfa seçerek kaydediliyor; ama
  Sıkıştır → "Sıkıştırma tamamlanamadı. Belge okunamadı; kopya
  oluşturulamadı"; Döndür, Kırp, Filigran, Sayfa numarası → "İşlem
  tamamlanamadı. Belge okunamadı". Kullanıcının ekran görüntüsündeki desenle
  birebir.
- **Yeniden üretim:** 15 belgelik taramada belirti yalnız son trailer'ı
  `/Prev` taşıyan üç düzende çıktı: Word yapısında hibrit dosya (klasik xref +
  `/XRefStm` + nesne akışında yapı öğeleri), klasik artımlı güncelleme
  (Acrobat "Kaydet", imza, form doldurma) ve xref akışlı artımlı güncelleme
  → `Invalid file trailer`. Tek bölümlü dosyalar (Chrome, Quartz, PDFKit,
  tek bölümlü xref akışı) etkilenmedi.
- **Beklenen:** açılabilen her belge bu araçlardan geçer; çıktı katı
  okuyucuyla açılır.
- **Gerçekleşen:** çıktı yazılıyor ama trailer'ındaki `/Prev` yeni dosyanın
  ortasında anlamsız bir bayta işaret ediyor; `validated_pdf_bytes` onu
  yeniden açamıyor ve yayını — doğru olarak — durduruyor.
- **Kök neden:** lopdf 0.34 `Reader::read` son bölümün trailer'ını
  `document.trailer` olarak tutar; `/XRefStm`'i yalnız `/Prev` döngüsünde
  siler, `/Prev`'i hiç silmez. Tam yazıcı (`Writer::save_internal` →
  `write_trailer`) bu trailer'ı olduğu gibi yazar: kaynağın bayt ofseti yeni
  dosyaya taşınır. Sayfa seçerek kaydetme (`extract_page_range` +
  `merge_documents`) ve önizleme belgeyi yeni bir trailer'la kurduğu için
  etkilenmiyordu — belirtinin yalnız bazı araçlarda görünmesinin nedeni bu.
- **Düzeltme:** motorun tek yükleme noktası `pdf-core::pdf::tolerant::load_pdf_tolerant`
  yüklenen belgeyi kaynağın fiziksel yerleşiminden ayırır
  (`detach_from_source_layout`): `/Prev`, `/XRefStm` ve eski xref akışının
  kendi akış/kodlama girdileri silinir; `Root`, `Info`, `ID` korunur. Yayın
  kapısı (`validated_pdf_bytes`) değişmedi — hatayı yakalayan oydu.
- **Regresyon testi:** `incrementally_updated_documents_compress_rotate_and_number_like_any_other`.
  Üç düzen lopdf'in yazıcısından bağımsız, bayt bayt kurulur (CoreGraphics ve
  PDFKit üçünü de 3 sayfa + güncellemede eklenen notla açıyor). Sıkıştır
  Failed değil; Döndür ve Sayfa numarası yayınlanır; çıktı katı okuyucuyla
  açılır, `/Prev`/`/XRefStm` taşımaz, `/ID`'yi korur; güncellemedeki not ve
  sayfa sırası korunur; kaynak SHA değişmez.

**Kırmızı kanıt:** 8 yeni Rust testi düzeltmesiz `29ffb73` üzerinde ayrı bir
worktree'de koşuldu — 8'i de düştü (artımlı test tam `Invalid file trailer`
ile). Yalıtım: diğer bütün düzeltmeler varken yalnız `tolerant.rs` düzeltmesi
çıkarılınca artımlı test, yalnız kapalı-popup kuralı çıkarılınca popup testi
düşüyor; hepsi birlikte 44/44 geçiyor.

### Gerçek üretici taraması (sentetik sözleşme, gerçek yazıcılar)

| Üretici / fiziksel düzen | `29ffb73` Sıkıştır | Düzeltme: Sıkıştır · Döndür · Seçerek kaydet |
|---|---|---|
| Chrome/Skia, 8 s. (düz · üst/altbilgili · etiketli) | NoBenefit | NoBenefit · ✓ · ✓ |
| Chrome/Skia + PNG logo (SMask) + JPEG tarama | Compressed 1.279.471 → 545.234 B | aynı · ✓ · ✓ |
| macOS Quartz, 11 s., 27 bağlantı | NoBenefit | NoBenefit · ✓ · ✓ |
| Quartz + görseller (PDFKit yeniden kaydı) | Compressed 1.175.295 → 443.409 B (1.175.558 → 443.556 B) | aynı · ✓ · ✓ |
| PDFKit (Önizleme) + vurgu, not, popup | **Failed — B10** (kaydetme de ✗) | NoBenefit · ✓ · ✓ |
| Word yapısı: hibrit `/XRefStm` + `/Prev` | **Failed — B13** (Döndür de ✗) | NoBenefit · ✓ · ✓ |
| Klasik artımlı güncelleme | **Failed — B13** (Döndür de ✗) | NoBenefit · ✓ · ✓ |
| Xref akışlı artımlı güncelleme | **Failed — B13** (Döndür de ✗) | NoBenefit · ✓ · ✓ |
| Xref akışı, tek bölüm | NoBenefit | NoBenefit · ✓ · ✓ |
| PDFKit yeniden kayıt — Chrome'un etiketli PDF'i | **açılamıyor — B14** | düzeltilmedi (Kalan borç 8) |

Metin ağırlıklı belgede NoBenefit dürüst sonuçtur: marka payı ve lopdf'in
yeniden yazımı birkaç KB ekler, %3 kapısı çıktı yazdırmaz.

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

Kullanıcı raporu turu (B10–B13):

- `crates/pdf-core/src/pdf/tolerant.rs` — `detach_from_source_layout`: yüklenen
  belge kaynağın `/Prev`, `/XRefStm` ve xref akışı girdilerini taşımaz (B13).
- `crates/pdf-core/src/pdf/stamp.rs` — açıklama denetimi payı ekledikten sonra
  yalnız yeni şeride bakar; yerleştirilemeyen girdi, gizli açıklama,
  kenarlıksız bağlantı, kapalı popup açığa çıkamaz; uyumsuz `/Rotate` 0
  sayılır (B10).
- `crates/pdf-core/src/optimizer.rs`, `crates/ekler-core/src/toolbox.rs` —
  sonda ≤2 bayt fazlalık kırpılır, açıklanamayan uzunluk görseli atlatır,
  dolaylı boyut girdileri çözülür (B11).
- `apps/belge-shell/src/shared-ui/failure.ts` — okunmuş belgenin retleri kendi
  sebebiyle anlatılır (B12).
- Testler: `pdf_workspace_audit.rs` 36 → 44; `failure.test.ts` +1.

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
   (`extract_page_range` → `merge_documents`). Süre makul (100 sayfa < 100 ms)
   ama **boyut değil** — gerçek üretici taramasında ölçüldü: sayfa başına
   kopyalama, sayfaların PAYLAŞTIĞI fontu/görseli her sayfaya ayrı taşıyor.
   Sırala çıktısı Chrome 8 s. 135.205 → 507.786 B (3,8×), Quartz 11 s.
   135.702 → 923.419 B (6,8×), aynı görseli üç sayfada kullanan Quartz
   1.175.295 → 3.523.790 B (3,0×). `29ffb73`'te de aynı; bu turda
   değiştirilmedi. P1: kaynak başına tek kopya haritası gerekir (madde 5 ile
   aynı iş).
5. **Seçimde kalan sayfalar arası bağlantılar** artık `null` (önceden ölü +
   şişkin). Doğru çözüm: `merge_documents`/`extract_page_range` içinde kaynak
   başına tek id haritası ve kopyalamadan sonra `/Dest`/`/P` yeniden bağlama.
   P2.
6. **`/Type`süz sayfa sözlükleri** (lopdf'in de sayfa saymadığı) hâlâ derin
   kopyalanır; semptomu üretemez, kayda geçti.
7. Çok büyük DCT görselde (`image` crate 512 MiB ayırma sınırı) çözüm hatası
   Failed üretir; önceden de böyleydi.
8. **B14 — Önizleme'de yeniden kaydedilmiş etiketli PDF hiç açılamıyor**
   (bulundu, bu turda düzeltilmedi). Chrome/Google Docs çıktısı PDFKit ile
   kaydedilince (Önizleme'de imza, vurgu ya da yalnız "Kaydet") PDFKit
   `/StructTreeRoot /IDTree n 0 R` yazıyor ama nesneyi yazmıyor.
   `validate_document` Root'tan erişilen her eksik referansı reddettiği için
   belge yüklemede "Eksik nesne referansı" ile düşüyor; önizleme ve bütün
   araçlar kapalı. Spec 7.3.10: tanımsız nesneye başvuru hata değildir, null
   sayılır. Önerilen güvenli düzeltme: sayfa ağacı ve sayfa sözlüklerinden
   erişilen — görünümü etkileyen — grafikte eksik nesne ret olarak kalır;
   yalnız belge düzeyindeki meta yapılarda (yapı ağacı, anahat, ad ağaçları,
   XMP) null sayılır. Bir güvenlik doğrulayıcısının anlamını değiştirdiği için
   bilinçli karar ister; ayrı iş olarak işaretlendi. Kullanıcının belgesi bu
   aileden değil: o belge yükleme doğrulamasından geçip önizleniyordu.
