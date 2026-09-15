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
| PDFKit yeniden kayıt — Chrome'un etiketli PDF'i (macOS 26.6.2) | **açılamıyor — B14** | 2026-09-15 turunda düzeltildi |

Metin ağırlıklı belgede NoBenefit dürüst sonuçtur: marka payı ve lopdf'in
yeniden yazımı birkaç KB ekler, %3 kapısı çıktı yazdırmaz.

## Final correctness pass — toleranslı yükleme + paylaşılan kaynak (2026-09-15)

Kapsam: iki açık kusur (Kalan borç 4, 5, 8). Tur içinde bulunan iki ek kusur
da kapatıldı: imzalı kaynaktan türetilen kopyanın imzalı gibi davranması (B16)
ve zincirleme türetmede marka birikimi (B17). Yeni özellik, arayüz değişikliği
ya da sıkıştırma algoritması değişikliği yok. Makine bu turda macOS 27.0'a
güncellenmişti; Xcode lisansı kabul edilmediği için bütün derleme ve git
komutları kurulu Command Line Tools ile (`DEVELOPER_DIR`) koşuldu.

### B14 — P1 · Belge düzeyinde eksik nesneye başvuran PDF hiç açılmıyordu

- **Symptom:** belge Önizleme'de, Chrome'da, PDFKit'te açılıyor; GölgeDosya'da
  önizleme, tarama fişi ve bütün araçlar "Belge okunamadı" /
  `Eksik nesne referansı: (n, 0)`.
- **Minimal reproduction:** iki sayfalık etiketli PDF, klasik xref.
  Catalog → `/StructTreeRoot << /K … /ParentTree … /IDTree 44 0 R >>`; 44
  numarası `/Size` içinde, xref girdisi serbest (`0000000000 65535 f`).
  Sayfa ağacı, içerik akışları, fontlar ve görseller eksiksiz.
  `load_pdf_tolerant` → `InvalidPdf("Eksik nesne referansı: (44, 0)")`.
- **Expected:** ISO 32000-1 §7.3.10 — tanımsız nesneye dolaylı başvuru hata
  değildir, null nesnesine başvuru sayılır. Sayfa görünümü tam olduğu için
  belge açılmalı; çıktılar sarkan başvuru taşımamalı.
- **Actual:** `validate_document` Root'tan erişilen HER nesneyi yürüyüp ilk
  eksik nesnede reddediyordu; eksik nesnenin sayfa görünümüne mi, belge
  düzeyindeki bir meta yapıya mı ait olduğunu ayırmıyordu.
- **Exact structural cause:** erişilebilir bir dolaylı başvurunun hedef nesne
  numarası xref'te kullanımda değil (serbest ya da tabloda yok). Başvuran
  anahtar görünüm dışı: `/StructTreeRoot /IDTree`. Üretici etiketi neden
  değildir; yapı neden budur. Gerçek motor kaydı: 2026-09-14'te macOS 26.6.2
  PDFKit, Chrome (Skia m152) çıktısını yeniden kaydederken nesne 69
  `<</Type /StructTreeRoot /K 68 0 R /ParentTree 70 0 R /IDTree 71 0 R>>`
  yazdı, 71'i yazmadı (`/Size 543`). **2026-09-15'te macOS 27.0 PDFKit aynı
  135.205 baytlık kaynaktan artık `/IDTree` üretmiyor** — bu yüzden kabul
  zinciri gerçek PDFKit çıktısına aynı yapının artımlı bir revizyonla
  eklenmesiyle koşuldu (aşağıda).
- **Loader change (toleranslı oku → normalize et → yeniden doğrula):**
  `validate::detach_dangling_references`, `tolerant::canonicalize`.
  - **Hâlâ ölümcül:** sayfa ağacında (`/Kids`), bir sayfanın `/Contents`inde,
    `/Parent` zinciri boyunca (kök düğümün üstü dâhil) miras alınan
    `/Resources`inde ve ondan erişilen her şeyde (font, `FontFile2`, Type3
    glifi, görsel, SMask, ICC, Form XObject ve onun kaynakları, ExtGState,
    desen, gölgeleme, işlev), `/Group`ta, sayfa kutularında ve Catalog
    `/OCProperties`te (hangi katmanın görüneceği) eksik nesne. Sözü:
    `Eksik nesne referansı: (n, 0) (sayfa N içeriği ya da kaynakları | sayfa
    ağacı | sayfa ağacından miras alınan kaynaklar | isteğe bağlı içerik
    yapılandırması)`.
  - **Sayfa ağacı yürüyüşü (şüpheci turu 2'den sonra):** ağaç düğüm düğüm
    yürünür; her düğümün KENDİ görünüm girdileri bir kez denetlenir, miras
    alınan doğrudan kaynak sözlüğü sayfa başına çoğaltılmaz. Kökün üstündeki
    `/Parent` zinciri var olduğu sürece miras verir; dosyada olmayan üst düğüm
    null sayılır ve zincir orada biter — sayfanın kendi içeriği ve kaynakları
    bundan bağımsız olarak ölümcül denetlenir.
  - **Ad haritaları:** `/Font /XObject /ExtGState /ColorSpace /Pattern /Shading
    /Properties /CharProcs` sözlüklerinin anahtarları yazarın seçtiği adlardır;
    her girdi izlenir. Yapısal sözlüklerde yalnız `/Metadata /PieceInfo
    /ToUnicode /CIDSet /StructParent(s)` izlenmez — çizimi değiştirmezler.
  - **Toleranslı:** bunun dışındaki her eksik nesne — açıklama ve görünüm
    akışı, popup, eylem, form alanı, gömülü dosya, yapı ağacı, anahat, ad
    ağacı, XMP, `/ToUnicode`, `/BoxColorInfo` — null semantiğine indirgenir
    (sözlük girdisi silinir, dizi öğesi ya da tümüyle başvuru olan nesne
    `null`), sonra DEĞİŞMEMİŞ katı `validate_document` koşar. Kaynak dosyaya
    dokunulmaz; sonuç `is_repaired`, `RepairStrategy::DanglingReferences` ve not
    taşır. Açıklamalar bilinçli olarak dışarıda: istenen tanım "sayfa ağacı /
    içerik / kaynaklar"dır ve eksik bir açıklamayı kaynağın kendi
    görüntüleyicisi de çizemez.
  - Şifreli belge normalize edilmez; reddi eski sözüyle kalır.
- **Regresyon testleri:**
  `dangling_reference_outside_page_graph_opens_previews_saves_and_reopens`
  (yedi eksik başvuru: `/IDTree`, `/ParentTree` dizisi, `/Outlines`, `/Popup`,
  bağlantı `/A`, görsel `/Metadata`, trailer `/Info`; artı Catalog'a uzanan
  imza alanı; aç → önizle → Seç/Sırala/Döndür → yeniden aç, Sıkıştır Failed
  değil, kaynak SHA değişmez), `missing_object_in_page_graph_is_still_fatal`
  (14 ölümcül hasar: içerik, font, gömülü font programı, görsel, `/Kids`
  sayfası, miras kaynak, kökün üstünden miras kaynak, `/P` adlı font, `/A`
  adlı Type3 glifi, `/B` adlı Form XObject'in fontu, `/OCProperties /D`,
  kökün `/Parent`ı dosyada yokken eksik içerik ve eksik font, kökün VAR olan
  üst düğümünün `/Parent`ı dosyada yokken eksik içerik; hepsi ret, çıktı yok,
  kaynak değişmez), `missing_object_outside_page_graph_opens_and_saves_clean`
  (9 düzensizlik: açıklama girdisi, açıklama görünümü, `/Annots`'taki kapalı
  popup, gömülü dosya, `/ToUnicode`, `/CIDSet`, `/BoxColorInfo`, widget
  `/MK /I` simgesi, eksik nesneye giden başvuru zinciri; hepsi açılır, Seç
  çıktısı temiz yeniden açılır) ve
  `inherited_resource_maps_are_checked_once_not_per_page` (kökte doğrudan
  1.000 girdili miras `/XObject`, 2.100 sayfa: açılır, onarımsız).

### B15 — P1 · Seç/Sırala/Sil paylaşılan kaynakları sayfa başına kopyalıyor, iç bağlantıları koparıyordu

- **Symptom:** 40 sayfalık belge sıralanınca çıktı 5×–30× büyüyor; 40→10
  seçim kaynaktan büyük; kalan sayfaya giden içindekiler bağlantısı ölü.
- **Minimal reproduction:** 40 sayfa, tek `FontFile2` akışını (40 KB)
  paylaşan tek font. Sırala (ters) → çıktıda 40 `FontFile2` akışı,
  1.678.907 B (kaynak 80.855 B, 20,8×). 1. sayfadan 5. sayfaya
  `/Dest [5 0 R /XYZ 0 800 0]`; Seç 1, 5 → çıktıda `/Dest [null /XYZ 0 800 0]`.
- **Expected:** sayfaların paylaştığı her dolaylı nesne çıktıda tek kanonik
  nesne; hedef sayfa kaldıysa bağlantı yeni sayfaya gider, kalmadıysa sarkan
  başvuru ya da `null` sayfalı hedef bırakılmaz.
- **Actual:** kopya sayısı, kaynağı kullanan sayfa sayısına tam eşit
  (Chrome D: 5 + 4 + 4 + 2 + 2 = 17 görsel akışı, kaynakta 7).
- **Exact structural cause:** `pdf::copy_object_recursive` her sayfa için
  `let mut id_map = BTreeMap::new()` açıyordu; eski→yeni nesne haritası
  sayfalar arasında paylaşılmadığı için aynı dolaylı nesne (font sözlüğü,
  `FontDescriptor`, `FontFile2`, görsel + SMask, ICC akışı, Form XObject,
  ExtGState, miras `/Resources`) her sayfada yeniden klonlanıyordu.
  `run_tool` bunu sayfa başına `extract_page_range(p, p)` + `merge_documents`
  ile çağırıyordu. Aynı harita yalnız o sayfayı içerdiğinden
  `is_foreign_page_node` kalan sayfaları da "yabancı" sayıp `null` yapıyordu.
- **Fix:** `pdf::extract_pages(doc, pages)` + `PageGraphCopy`:
  - kaynak belge başına TEK harita; çıktıda kalacak sayfalar kopyalamadan önce
    haritaya yazılır; ikinci kez görülen nesne klonlanmaz;
  - kopya özyinelemeyle değil bir iş listesiyle yürür (uzun zincir yığını
    taşıramaz);
  - başvuru nesnesi zinciri (`40 0 obj 12 0 R`) — bağlantı hedefinde de sayfa
    ağacının `/Kids`inde de — son nesnenin KİMLİĞİNE çözülür. lopdf 0.34
    `get_object` zinciri kendisi izleyip son nesnenin gövdesini döndürür ama
    kimliğini vermez; harita kimlikle sorgulandığı için zincir `objects`
    üzerinde yürünür;
  - sayfaya ait olmayan yapıya geçilmez, başvuru `null`: çıktıda olmayan sayfa
    ve sayfa ağacı düğümü, Catalog, yapı ağacı/öğesi, OBJR/MCR, anahat, makale
    zinciri/boncuğu, PDF 2.0 belge parçası (`/DPart`, `/DPartRoot`), imza
    değeri ve belge zaman damgası, kalan sayfaların `/Annots`'unda (ve onların
    `/Popup`ında) olmayan açıklama ve kalan bir widget'ın `/Parent` zincirinde
    olmayan form alanı düğümü. `/Type` isteğe bağlı olduğundan bu yapılar tür
    adı yazılmamışsa biçimleriyle de tanınır: boncuk (`/T /N /V /P /R`), yapı
    öğesi (`/S` adı + `/P` + `/K`|`/Pg`), belge parçası (`/DParts` ya da
    `/Start`+`/End` başvuruları), imza sözlüğü (`/Filter` + `/ByteRange` +
    `/Contents`), alan düğümü (`/FT` ya da `/T` ile `/Kids`|`/Parent`|`/V`);
  - kalan widget'ın alanı (`/FT /T /V /DA`) korunur; alanın `/Kids`inde yalnız
    çıktıda kalan çocuklar durur;
  - hedefi çıkarılmış bağlantının `/Dest`i ve hedefi çıkarılmış HER GoTo
    eylemi — `/A`, `/AA` girdileri, `/Next` — yazılmaz (bağlantı dikdörtgeni
    sayfada kalır); GoTo'nun ardına zincirlenmiş eylemler (`/Next`) onun
    yerine geçer; `null`a çözülen, dosyada olmayan ya da aralık dışı sayfa
    hedefi de çıkarılmış sayılır;
  - kalan sayfaya giden hedef yeni sayfaya bağlanır; adlandırılmış hedef açık
    hedefe çözülür (dize önce `/Names /Dests` ad ağacında, ad önce Catalog
    `/Dests`te — ISO 32000-1 §12.3.2.3); tamsayı sayfa numarası (standart dışı,
    pdf.js 0 tabanlı okur) sayfa başvurusuna çevrilir; ad ağacı bir kez, her
    düğümü bir kez ziyaret edilerek dizine çevrilir;
  - Catalog `/OCProperties` aynı haritayla taşınır (taban durumu `/OFF`
    olan yapılandırma açık `/OFF` listesine çevrilir, birleştirmede listeler
    birleşir) — aksi hâlde kaynakta gizli katman kopyada görünürdü.
  `merge_documents` her kaynak belge için aynı kopyalayıcıyı kullanır. `run_tool`
  Seç/Sırala/Sil tek geçiş. Önizleme ve Ekler paketi aynı fonksiyonları
  kullandığı için onlar da düzeldi.
- **Links / destinations:**

  | Bağlantı (1. sayfa → hedef) | Seç 1, 5, 20, 35 | Sırala (ters) | Sil 5 |
  |---|---|---|---|
  | açık `/Dest` → 5 | çıktı s.2 | çıktı s.36 | hedefsiz |
  | doğrudan GoTo → 20 | çıktı s.3 | çıktı s.21 | çıktı s.19 |
  | dolaylı GoTo nesnesi → 30 | hedefsiz (`/A` yok) | çıktı s.11 | çıktı s.29 |
  | `/Names /Dests` ad ağacı → 35 | çıktı s.4 (açık hedefe çözüldü) | çıktı s.6 | çıktı s.34 |
  | PDF 1.1 `/Dests` adı → 40 | hedefsiz | çıktı s.1 | çıktı s.39 |
  | dış URI | korunur | korunur | korunur |
  | 20 → 1 geri bağlantı | çıktı s.3 → s.1 | korunur | korunur |

  Birleştir (aynı belge ×2): ikinci belgenin bağlantısı 45. sayfaya, ilkinin
  5. sayfaya; belgeler karışmaz. Hiçbir çıktıda eksik nesneye başvuru ya da
  `null` sayfalı hedef yok.
- **Regresyon testleri:** `shared_resources_stay_single_{a_same_font,
  b_same_image, c_font_and_image, d_mixed_resources, with_inherited_resources}`
  (her fixture'da Seç 40→1, 40→10, Sırala 40→40, Sil 40→39: font, font
  programı, görsel, ICC, Form XObject ve ExtGState sayısı seçilen sayfaların
  kaynaktaki nüfusuna eşit; aynı baytlı ikinci akış yok; çıktı ≤ gereken akış
  baytı + 12 KB + 700 B/sayfa; tam belge çıktısı ≤ kaynağın %115'i),
  `internal_links_follow_retained_pages_and_leave_nothing_for_removed_ones`,
  `page_copy_carries_nothing_from_removed_pages`,
  `removed_goto_actions_do_not_survive_in_any_position`,
  `null_integer_and_named_destinations_resolve_like_viewers`,
  `page_listed_through_a_reference_object_keeps_its_links_and_stays_single`,
  `document_parts_threads_and_structure_of_removed_pages_do_not_leak`,
  `retained_widgets_keep_their_field_and_removed_fields_do_not_leak`,
  `looped_name_tree_cannot_hang_preview_or_select`,
  `long_object_chains_cannot_overflow_the_stack`,
  `hidden_optional_content_stays_hidden_in_page_copies` ve
  `page_copy_scales_linearly_with_flat_name_arrays_and_off_layer_lists`.

### Şüpheci turu 1 — ilk sürümün çürütülmesi (12 kusur)

İlk uygulama (tek harita + sayfa grafiği denetimi) bağımsız bir ajana
çürütülmek üzere verildi; 31 denemenin 23'ü düştü, 12 ayrı kusur yeniden
üretildi. Hepsi düzeltildi ve teste bağlandı:

| # | Kusur | Önem | Karar |
|---|---|---|---|
| 1 | Döngülü ad ağacı (`/Kids [7 0 R 7 0 R]`) önizleme ve Seç'i dakikalarca kilitliyor | P0 | ad ağacı araması ziyaret kümesi + bütçe |
| 2 | Uzun nesne zinciri (1.000+ halka) 2 MiB iş parçacığında yığın taşması → süreç `SIGABRT` (eski kopyalayıcıda da vardı) | P0 | kopya özyinelemesiz iş listesi |
| 3 | Çıkarılan sayfanın nesneleri çıktıya sızıyor: form alanı `/Parent → /Kids` (kardeş widget'ın gizli `/V`si), imza `/V → /Reference /Data → Catalog` (AcroForm, anahat, `/Perms`), `ResetForm /Fields`, `/Next Hide /T`, boncuk `/N`, `/SD` → yapı ağacı → OBJR | P0 | tür bazlı kesme (Catalog, yapı, anahat, boncuk, imza, alan düğümü, kalan sayfalarda olmayan açıklama) |
| 4 | Hedefi çıkarılmış GoTo `/Next`, bağlantı `/AA`, sayfa `/AA` içinde `/D`'siz kalıyor | P1 | hedefi çıkarılmış GoTo her konumda düşer |
| 5 | Dosyada olmayan sayfaya hedef `[null /Fit]` olarak yayınlanıyor | P1 | null sayfa = çıkarılmış hedef |
| 6 | Dize ad önce `/Dests`'te aranıyor (spec: ad ağacı) | P2 | dize → ad ağacı önce, ad → `/Dests` önce |
| 7 | Tamsayı sayfa numaralı hedef sırala sonrası yanlış sayfaya gidiyor | P2 | sayfa başvurusuna çevrilir |
| 8 | Atlama listesi ad haritalarına da uygulanıyor: `/P` adlı font, `/A` adlı Type3 glifi, `/B` adlı Form XObject'in fontu eksikken belge kabul ediliyor | P0 | ad haritalarında her girdi izlenir |
| 9 | Kök Pages düğümünün `/Parent`ından miras alınan eksik font kabul ediliyor | P2 | sayfa kaynakları çözülmüş (miras) sözlükten denetlenir |
| 10 | Eksik `/OCProperties /D` kabul ediliyor → gizli katman görünür | P2 | `/OCProperties` görünüm grafiğinde; kopyaya da taşınır |
| 11 | Görünüm dışı eksikler belgeyi açtırmıyor: `/Annots`'taki popup, gömülü dosya, `/ToUnicode`, `/CIDSet`, `/BoxColorInfo`, widget `/MK /I` | P1 | ölümcül kapsam "sayfa ağacı / içerik / kaynaklar"a daraltıldı |
| 12 | Tümüyle eksik nesneye başvuru olan nesne (`8 0 obj 9 0 R`) açılışı engelliyor | P2 | bu nesne de null olur |

İlk sürüm üzerinde (anlık kopya) yeni testlerin 7'si düşüyor; yığın testi
süreci `SIGABRT` ile öldürüyor; 20 hasar varyantının 14'ü yanlış tarafta
(5 ölümcül hasar kabul, 9 düzensizlik ret). Düzeltmeyle hepsi doğru tarafta.

### Kırmızı kanıt

Yeni testler düzeltmesiz `07e3ee6` üzerinde ayrı bir worktree'de koşuldu:
8 testin 7'si düştü — `Eksik nesne referansı: (40, 0)`, `L1-5: hedef sayfası
başvuru değil: [null, /XYZ, 0, 800, 0]`, `SameFont/sec-10: font_programs 10
(beklenen 1)` … Sekizincisi (`missing_object_in_page_graph_is_still_fatal`)
değişmezdir: HEAD'de de geçer, düzeltmede de geçmek zorundadır.

### Boyut ölçümleri — repo fixture'ları (A–D, 40 sayfa; bayt)

| Fixture | Kaynak | İşlem | Önce (`07e3ee6`) | Sonra | Gereken akış |
|---|---:|---|---:|---:|---:|
| A aynı font | 80.855 | Seç 1 | 45.520 (0,56×) | 45.520 (0,56×) | 40.782 |
| | | Seç 10 | 422.328 (5,22×) | 55.217 (0,68×) | 47.813 |
| | | Sırala 40 | 1.678.907 (20,76×) | 87.719 (1,08×) | 71.435 |
| | | Sil → 39 | 1.637.023 (20,25×) | 86.639 (1,07×) | 70.651 |
| B aynı görsel | 72.199 | Seç 1 | 46.403 (0,64×) | 46.403 (0,64×) | 41.940 |
| | | Seç 10 | 431.145 (5,97×) | 53.810 (0,75×) | 46.582 |
| | | Sırala 40 | 1.714.194 (23,74×) | 78.622 (1,09×) | 62.184 |
| | | Sil → 39 | 1.671.428 (23,15×) | 77.798 (1,08×) | 61.667 |
| C font + görsel | 124.703 | Seç 1 | 87.414 (0,70×) | 87.414 (0,70×) | 82.231 |
| | | Seç 10 | 841.264 (6,75×) | 97.458 (0,78×) | 89.511 |
| | | Sırala 40 | 3.354.898 (26,90×) | 131.126 (1,05×) | 113.968 |
| | | Sil → 39 | 3.271.114 (26,23×) | 130.007 (1,04×) | 113.156 |
| C, kaynaklar mirasla | 122.628 | Sırala 40 | 3.357.806 (27,38×) | 131.196 (1,07×) | 113.968 |
| D farklı kaynaklar | 849.236 | Seç 1 | 128.485 (0,15×) | 128.485 (0,15×) | 122.288 |
| | | Seç 10 | 956.809 (1,13×) | 179.622 (0,21×) | 169.628 |
| | | Sırala 40 | 6.440.384 (7,58×) | 855.808 (1,01×) | 834.613 |
| | | Sil → 39 | 5.833.016 (6,87×) | 854.598 (1,01×) | 833.774 |

"Gereken akış", seçilen sayfaların kaynakta eriştiği akışların bayt toplamıdır
— çıktının olabileceği en küçük gövde. Seç 1'in "önce" ve "sonra"sı aynıdır:
tek sayfada çoğaltılacak ikinci kullanıcı yoktur. B ve C'de 1 sayfalık çıktı
kaynağın %64–70'i: sayfa, belgenin en büyük nesnesi olan logoyu kullanır; bu
en küçük olası çıktıdır, çoğaltma değildir.

### Boyut ölçümleri — gerçek yazıcılar (Chrome/Skia m152, macOS 27 Quartz; bayt)

| Fixture | Kaynak | Sırala 40 önce → sonra | Sil → 39 önce → sonra | Seç 10 önce → sonra | FontFile2 / görsel (Sırala, önce → sonra) |
|---|---:|---|---|---|---|
| Chrome A aynı font | 206.034 | 2.053.869 → 183.310 | 2.002.607 → 180.014 | 516.058 → 84.509 | 80/0 → 2/0 |
| Chrome B aynı görsel | 105.418 | 2.634.598 → 93.984 | 2.568.820 → 93.354 | 661.258 → 75.086 | 40/40 → 1/1 |
| Chrome C font + görsel | 220.800 | 2.353.487 → 193.080 | 2.294.735 → 189.647 | 590.963 → 92.501 | 80/80 → 2/2 |
| Chrome D farklı | 1.621.408 | 8.170.414 → 1.588.413 | 7.544.970 → 1.585.691 | 2.084.063 → 829.685 | 120/17 → 3/7 |
| Quartz A aynı font | 122.935 | 1.606.068 → 131.015 | 1.566.011 → 128.782 | 404.025 → 63.654 | 80/0 → 2/0 |
| Quartz B aynı görsel | 90.148 | 2.750.150 → 98.859 | 2.681.481 → 98.174 | 690.126 → 78.320 | 40/40 → 1/1 |
| Quartz C font + görsel | 143.940 | 2.046.282 → 153.034 | 1.995.209 → 150.508 | 514.180 → 77.276 | 80/80 → 2/2 |
| Quartz D farklı | 1.561.360 | 8.395.209 → 1.570.799 | 7.762.503 → 1.568.689 | 2.140.058 → 831.456 | 200/17 → 5/7 |

Kaynak nüfusu: Chrome A 2 `FontFile2`, B 1 görsel, C 2/2, D 3/7; Quartz A 2,
B 1/1, C 2/2, D 5/7 — "sonra" sütunu kaynağa eşit.

### Kabul zinciri — Chrome → PDFKit kaydı → aç → önizle → kopya kaydet → yeniden aç

| Fixture | `07e3ee6` | Düzeltme |
|---|---|---|
| Chrome E (iç bağlantılı, etiketli) → PDFKit (macOS 27) kaydı · düz / vurgulu | ✓ | ✓ (Sırala 449.386 → 73.162 B) |
| Chrome F → PDFKit kaydı · düz / vurgu + not + popup | ✓ | ✓ |
| Chrome F → PDFKit + vurgu/not, artı macOS 26 yapısı: `/StructTreeRoot /IDTree 376 0 R`, 376 `/Size 377` içinde serbest (artımlı revizyon) | **aç ✗ · önizleme ✗ · kaydet ✗** `Eksik nesne referansı: (376, 0)` | aç ✓ (`DanglingReferences`) · önizleme 312.890 B PNG (yapısı eklenmemiş dosyayla aynı) · Seç 1 90.999 B · Sırala 127.160 B · yeniden aç onarımsız · kaynak değişmedi |

Zincir kabuğun çağırdığı motor fonksiyonlarıyla koşuldu (`scan_source_files`,
`raster::preview_page`, `run_tool_with_outcome`, `load_pdf_tolerant`); arayüz
tıklanarak DEĞİL.

Her iki boyut tablosu, aşağıdaki bütün düzeltmelerden SONRA son kodla yeniden
ölçüldü; "sonra" sütunlarındaki her değer bayt bayt aynı çıktı. Gerçek yazıcı
fixture'larının tamamında (Chrome A–F, PDFKit C/E/F ve `/IDTree` enjekteli F,
Quartz A–D) en büyük oran 1,10× (Quartz B Sırala: sabit marka payı + ICC).

### Şüpheci turu 2 — yeniden yazılan kopyalayıcı ve yükleyicinin çürütülmesi (10 kusur + 2 performans)

Şüpheci turu 1'in düzeltmeleri ikinci bir bağımsız ajana çürütülmek üzere
verildi. Her bulgu `run_tool_with_outcome` ya da `load_pdf_tolerant` üzerinden
başarısız bir denetimle yeniden üretildi; hepsi düzeltildi ve teste bağlandı.
"Düzeltmesiz" sütunu, testin ikinci tur öncesi kodun kopyası (imza temizliği
dâhil; kopyalayıcı ve yükleyici eski) üzerinde ayrı bir worktree'de verdiği
hatadır.

| # | Kusur | Önem | Düzeltme | Test | Düzeltmesiz |
|---|---|---|---|---|---|
| 1 | Çıkarılan sayfaların PDF 2.0 belge parçası meta verisi (`/DPM`) ve ilişkili dosyaları (`/AF` gömülü dosya) sızıyor: sayfa `/DPart` → yaprak → `/Parent` → `/DParts` → kardeşler; `/GoToDp` eylemi de | P0 | `/DPart`, `/DPartRoot` ve türsüz `/DParts`, `/Start`+`/End` kesilir | `document_parts_threads_and_structure_of_removed_pages_do_not_leak` | `çıkarılan sayfanın verisi sızdı: GIZLI-MUSTERI-2` |
| 2 | Kök Pages'in `/Parent`ı dosyada yokken eksik `/Contents` ya da eksik font kabul ediliyor; not "sayfa içeriğine ait olmayan 2 başvuru" diyor, Seç boş sayfa yayınlıyor (kökün `/Parent`ı var olan bir düğüm, onunki eksikken de) | P1 | sayfa ağacı düğüm düğüm; kökün üstü toleranslı, sayfanın kendisi ölümcül | `missing_object_in_page_graph_is_still_fatal` (+3 varyant) | `DanglingRootParentMissingContents: … eksik nesne kabul edildi`; `DanglingGrandparentMissingContents: …` |
| 3 | Hiçbir sayfa çıkmadığı Sırala ve Birleştir'de bile kalan widget'lar alanını kaybediyor (`/Parent` → `/FT /T /V /DA`) | P1 (regresyon) | kalan widget'ların `/Parent` zinciri korunur; alan `/Kids`i süzülür | `retained_widgets_keep_their_field_and_removed_fields_do_not_leak` | `alan null oldu` |
| 4 | `/Type` yazılmamış makale zinciri ve boncuklar (başlık, yazar) sızıyor | P1 | boncuk biçimi tanınır | `document_parts_…` | (1 ile aynı test) |
| 5 | `/Type` yazılmamış yapı öğeleri `/SD` üzerinden 2. sayfanın `/ActualText`/`/Alt`ını sızdırıyor | P2 | yapı öğesi biçimi tanınır | `document_parts_…` | (1 ile aynı test) |
| 6 | `SubmitForm /Fields`, `/FT`'si ve `/Kids`i olmayan alanın değerini (`/V (GIZLI-TCKN-99)`) sızdırıyor | P2 | alan düğümü `/T` + `/Kids`\|`/Parent`\|`/V` ile de tanınır | `retained_widgets_…` | (3 ile aynı test) |
| 7 | Hedefi çıkarılmış GoTo'yu düşürmek, ardına zincirlenmiş JavaScript eylemini de düşürüyor | P2 (aşırı kesme) | GoTo'nun yerine `/Next`i geçer | `removed_goto_actions_do_not_survive_in_any_position` | `kaldırılan GoTo'nun ardındaki eylem de düştü` |
| 8 | Başvuru nesnesi üzerinden gidilen kalan sayfaya bağlantı düşüyor: (a) `/Kids [p1 40 0 R p3]` + `/Dest [12 0 R /Fit]`, (b) `/Dest [40 0 R /Fit]` | P2 | kimlikler zincirin son nesnesine çözülür | (b) `null_integer_and_named_destinations_resolve_like_viewers`, (a) `page_listed_through_a_reference_object_keeps_its_links_and_stays_single` | (b) `no entry found for key`; (a) `left: None, right: Some("Sayfa 2")` |
| 9 | Sayfası null'a çözülen hedef (`/Dest [40 0 R]`, `40 0 obj 41 0 R`, 41 yok) yayınlanıyor | P2 | null/eksik sayfa = çıkarılmış hedef | `null_integer_…` | mutasyonla (`Removed` → `NotLocal`): `no entry found for key` |
| 10 | Kökte DOĞRUDAN 1.000 girdili miras `/XObject`, 2.100 sayfa: sağlam belge `PDF nesne sınırı aşıldı` ile reddediliyor; 150 sayfa × 40.000 girdide tepe bellek 1.084 MB | P1 (regresyon) | miras değerler düğüm başına bir kez denetlenir | `inherited_resource_maps_are_checked_once_not_per_page` | `sağlam belge reddedildi: InvalidPdf("PDF nesne sınırı aşıldı")` |
| perf-1 | Düz `/Names /Dests` dizisinde (Qt/wkhtmltopdf) bağlantı başına doğrusal arama: kopya karesel | perf | ad ağacı bir kez dizine çevrilir | `page_copy_scales_linearly_with_flat_name_arrays_and_off_layer_lists` | `8× sayfa → 30,9× süre` |
| perf-2 | Taban durumu OFF katman yapılandırmasında grup başına `/ON` listesinde doğrusal arama | perf | karma küme | aynı test | `8× grup → 53,4×` |

**Kapanış doğrulamasında bulunan iki ek kusur** (ikinci tur düzeltmeleri
testlerle kapatılırken):

- **8(a) ilk kapanış sürümünde kapanmamıştı.** `resolve_chain`
  `get_object` üzerinden yürüyordu; lopdf 0.34'te `get_object` zinciri kendisi
  izlediği için döngü hiç ilerlemiyor, harita zincirin başındaki kimlikle
  sorgulanıyordu. 8(b) testi ilk koşuda bu yüzden düştü; (a) varyantı ise
  testte hiç yoktu. `objects` üzerinde yürüyen çözüm ve `get_pages`'in verdiği
  kimliklerin çözülmesiyle kapandı; (a) için yeni test eklendi.
- **İmza temizliği hâlâ kullanılan bir nesneyi boşaltabiliyordu.** Aynı lopdf
  davranışı yüzünden `reachable_ids` zincirin son nesnesini "erişilen" diye
  kaydetmiyordu. Sayfaların fontu `/F1 40 0 R` (`40 0 obj 7 0 R`) üzerinden
  kullanılırken imzanın `/Reference /Data`sı fonta doğrudan gidiyorsa font
  `null` oluyordu. Test:
  `signature_cleanup_keeps_objects_still_used_through_reference_chains`;
  düzeltmesiz: `dondur: sayfa 1 fontu boşaltıldı: null`.

Kırmızı kanıt (ikinci tur): yukarıdaki 8 test ikinci tur öncesi kod üzerinde
8/8 düştü; güncel kodda hepsi geçer. Ölçeklenme testi o kodda 30,9× / 53,4×
(eşik 20×), güncel kodda tek başına 8,4× / 8,4×, tam paralel pakette 8,4–9,0×.

Ajanın kıramadığı saldırılar da kayda geçti. Kopyalayıcı tarafında korunanlar:
yalnız `/Popup`'tan erişilen popup, `/Annots`'taki açıklamaya `/IRT`,
FileAttachment'ın gömülü dosyası, birleştirilen alanın `/V`si, imza widget'ının
`/AP`si, dolaylı `/Annots`, OCG `/Usage`, kendi kaynaklı Type3 glifi.
Bağlantılar doğru işleniyor: tamsayı hedef -1/99 düşer, dolaylı GoTo `/D`
dizisi yeniden bağlanır, açıklama `/AA /U` ve sayfa `/AA /C` doğru, `/Next`
dizisinde yalnız çıkarılan öğe düşer, aynı belge ×2 birleştirmede her kopyanın
bağlantısı kendi sayfasına gider. Katman yapılandırmasında BaseState OFF + ON,
iç içe `/Order`, `/RBGroups` ve `/Locked` korunur. Yükleyici şunlarda ölümcül
kalıyor: Kids → başvuru → başvuru → eksik, Form içindeki Pattern, ExtGState
SMask `/G`, kök üstü `/Parent` döngüsü, türsüz ara Pages düğümü.

### Performans — ikinci tur öncesi ve sonrası (release, `extract_pages`, en iyi 3)

| Belge | İşlem | Önce | Sonra |
|---|---|---:|---:|
| düz `/Names`, 5.000 sayfa, sayfa başına adlı bağlantı | tüm sayfalar ters | 0,072 s | 0,023 s |
| aynı, 20.000 sayfa | tüm sayfalar ters | 0,938 s | 0,101 s |
| aynı, 40.000 sayfa | tüm sayfalar ters | 3,648 s | 0,200 s |
| BaseState OFF, 20.000 grup (yarısı `/ON`) | Seç 1 | 0,235 s | 0,013 s |
| aynı, 80.000 grup | Seç 1 | 3,648 s | 0,061 s |
| aynı, 160.000 grup | Seç 1 | 14,700 s | 0,118 s |

Ölçüm notu: ilk denemede iki worktree aynı hedef dizinini paylaştı. Cargo
paketi çalışma alanına göreli yolla tanıyıp dosya zamanına baktığı için ana
ağacın pdf-core derlemesini "taze" saydı ve iki taraf da aynı kodu ölçtü (iki
sütun eşit çıktı). Kaynaklara dokunulup iki taraf ayrı ayrı yeniden derlenerek
tekrarlandı; günlüklerde `Compiling pdf-core (<worktree yolu>)` görülüyor.
Yukarıdaki değerler bu ikinci ölçümdür.

### B16 — P1 · İmzalı kaynaktan türetilen kopya imzalı gibi davranıyordu

- **Symptom:** onayla türetilen kopya, görüntüleyicide bozuk bir imza
  listeliyor; GölgeDosya'nın tarayıcısı kopyayı yeniden "imzalı" sayıyor.
- **Minimal reproduction:** `signed_pdf()` — 3 sayfa; DocMDP sertifika imzası
  (`/Type /Sig`, `/ByteRange`, `/Contents <PKCS#7>`, `/Reference`), belge zaman
  damgası (`/Type /DocTimeStamp`), `/Type`'sız UR3 imzası, `/DSS` sertifikası,
  görünür imza widget'ı (`/Lock`, `/AP`) ve zaman damgası widget'ı,
  `/AcroForm /SigFlags 3`, Catalog `/Perms << /DocMDP /UR3 >>`. Benzer görünen
  ama imza OLMAYAN yapılar da var: metin alanı `/V (Ali Veli)`, `/PieceInfo`
  içinde `/Filter /Reference /Contents` taşıyan özel sözlük, boncuğun `/V`si,
  sayfaların `/Contents` akışları. Döndür 90 (onaylı) → çıktı taranır.
- **Expected:** yeniden yazılan dosyada `/ByteRange` artık imzalanan baytları
  göstermez; imza kriptografik olarak geçersizdir, çıktı imzasız olmalı. Görünür
  imza görünümü kalmalı (basılı, imzalı bir belgenin kopyası gibi), benzer
  yapılar dokunulmamalı.
- **Actual:** Döndür, Kırp, Numara, Filigran ve Sıkıştır belgeyi yerinde
  yeniden yazdığı için imza sözlüklerini, `/Perms`, `/DSS` ve `/SigFlags`'i
  aynen taşıyordu. Sayfa kopyasında (Seç/Sırala/Sil/Birleştir) imza alanı
  `/V` taşımaya devam ediyordu (M1: temizlik kaldırılınca ilk düşen işlem
  Seç).
- **Exact structural cause:** yayın yolunda imza semantiğine bakan bir adım
  yoktu. İmza, ISO 32000-1 §12.8.1 sözlüğü (`/Filter`, `/ByteRange`,
  `/Contents`) ile, onu `/V`de taşıyan `/FT /Sig` alanı, `/AcroForm /SigFlags`
  bitleri (1: SignaturesExist, 2: AppendOnly), Catalog `/Perms` (DocMDP, UR3)
  ve `/DSS` doğrulama verisi üzerinden temsil edilir; hepsi kopyalanıyordu.
- **Fix:** `pdf::strip_signatures`. Araç kutusu her PDF'i yükledikten sonra
  çağırır; imzasız belgede hiçbir şey yapmaz. İmza sözlüğü BİÇİMİYLE tanınır:
  `/Type /Sig`, `/Type /DocTimeStamp` ya da `/Filter` adı + `/ByteRange`
  dizisi + `/Contents` bayt dizesi BİRLİKTE. Anahtar adı tek başına yetmez;
  `detect_signature` de aynı yüklemi kullanır. Temizlik adımları:
  - imza sözlükleri `null` olur;
  - yalnız `/FT /Sig` alanlarının `/V`si silinir;
  - Catalog `/Perms`, `/DSS` ve `/AcroForm /SigFlags` (doğrudan ya da dolaylı)
    kaldırılır;
  - imza malzemesinden erişilen nesnelerden, temizlikten SONRA belgeden artık
    erişilemeyenler `null` olur (paylaşılan nesne yerinde kalır; başvuru
    zincirinin her halkası sayılır).

  İmza widget'ı ve görünümü kalır.
- **Semantik değişmezler** — `derived_copies_of_signed_pdfs_carry_no_signature_semantics`,
  dokuz işlemin her birinde (Seç, Sırala, Sil, Döndür, Kırp, Numara, Filigran,
  Sıkıştır — gerçekten `Compressed` —, Birleştir):

  | Değişmez | Nasıl denetlenir |
  |---|---|
  | eski kriptografik imza erişilemez | trailer'dan erişilen hiçbir sözlük §12.8.1 biçiminde değil |
  | `/ByteRange` ve imza `/Contents`i erişilemez | erişilen sözlüklerde `/ByteRange` yok; dört imza malzemesinin baytları (düz, büyük/küçük hex) dosyada hiç yok |
  | Catalog `/Perms` ve `/DSS` eski imzayı taşımıyor | Catalog'da ikisi de yok; DSS sertifikası dosyada yok |
  | `/AcroForm /SigFlags` imzalı belge bildirmiyor | `/SigFlags` yok |
  | tarayıcı imzasız görüyor | `scan_source_files(...).is_signed == false`, `detect_signature == false` |
  | PDF yeniden açılıyor | katı yükleme, `is_repaired == false` |
  | normal sayfa içeriği değişmiyor | her çıktı sayfasının içeriği kaynaktaki karşılığını bayt dizisi olarak içerir |
  | görünür imza görünümü korunuyor | `Imza1` widget'ının `/AP /N` akışı kaynakla bayt bayt aynı |
  | imza alanı kalıyorsa yalnız boş alan | iki `/FT /Sig` alanı sayfada, ikisinde de `/V` yok |
  | kaynak SHA-256 değişmiyor | her işlemde öncesi/sonrası karşılaştırılır |
  | anahtar adına göre global silme yok | metin alanı `/V (Ali Veli)` kalır; özel sözlüğün `/Reference` ve `/Contents`i kalır; yerinde yazımda boncuğun `/V`si kalır; sayfa `/Contents`i değişmez |

- **Mutasyon kanıtı** (her biri tam düzeltmenin üzerinde tek değişiklik):

  | Mutasyon | Sonuç |
  |---|---|
  | M1 imza temizliği hiç yok | `sec: imza alanı değer taşıyor` |
  | M2 yalnız imzaya ait nesneler boşaltılmıyor | `dondur: imza malzemesi dosyada kaldı: GIZLI-DSS-SERTIFIKA` |
  | M3 `/V` her sözlükten siliniyor | metin alanının `/V`si kayıp (`DictKey`) |
  | M4 imza anahtar adıyla tanınıyor (`/ByteRange` ya da `/Contents`) | sayfalar null → `Sayfa listesi boş, yinelenmiş veya sınır dışında` |
  | M5 erişim kümesi `get_object` ile (zincir halkası atlanıyor) | `dondur: sayfa 1 fontu boşaltıldı: null` |

### B17 — P2 · Türetilmiş kopyadan türetilen kopya markayı biriktiriyordu (kabul zincirinde bulundu)

- **Symptom:** Quartz C üzerinde sırala → döndür → sil zinciri: 1. sayfanın
  MediaBox'ı `[0 -34 595 842]` → `[0 -34 629 842]` → `[0 -34 663 842]`; marka
  formu 2 → 4 → 6. Her türetme bir pay ve bir logo daha ekliyordu.
- **Exact structural cause:** `apply_branding_for_output` her çağrıda yeni
  vektör + marka formu oluşturup HER sayfaya yeni pay ekliyordu; sayfada
  görünür bir işaretin zaten durup durmadığına bakmıyordu.
- **Fix:** marka formu ada değil baytlarına bakılarak tanınır
  (`q /BrandAlpha gs /Mark Do Q` + logo akışı). Sayfa içeriği `q/Q/cm` ile
  yürünür; formu çizen `Do`nun kutusu CropBox/MediaBox içinde görünüyorsa
  sayfaya dokunulmaz. Gerekirse var olan form yeniden kullanılır. Kırpma
  işareti görünür alanın dışında bıraktıysa sayfa yeniden işaretlenir.
- **Test:** `branding_is_idempotent_across_chained_derivations`. İki eski test
  birikimi sabitliyordu (`rotate_per_page_left_right_180_and_repeated`,
  `chain_a_open_rotate_reorder_delete_save_reopen_each_step`); beklentileri
  tek paya güncellendi, çünkü birikim kusurdu.

### Son kabul zinciri — open → preview → reorder → rotate → delete → save → reopen → compress → reopen

Sentetik içerik, gerçek motorlar: Chrome/Skia m152 (`chrome-D`), macOS 27
PDFKit kaydı + macOS 26 `/IDTree` yapısı (`pdfkit-F-annotated-idtree`), macOS
27 Quartz (`quartz-C`). Her adımın çıktısı bir sonrakinin girdisidir.

| Adım | chrome-D (40 s.) | pdfkit-F + `/IDTree` (8 s.) | quartz-C (40 s.) |
|---|---|---|---|
| kaynak SHA-256 önce | `1165758c…97129d` | `218ea1fe…23716d` | `9f4b4286…185e61` |
| open | 1.621.408 B · Strict | 159.623 B · `DanglingReferences` (onarıldı) | 143.940 B · Strict |
| preview | 595×842 PNG 128.848 B | 595×842 PNG 312.890 B | 596×842 PNG 154.257 B |
| reorder (ters) → aç | 1.588.413 B · 40 s. · her sayfa içeriği kaynağın tersi | 127.160 B | 153.034 B |
| rotate (s.1 +90°) → aç | 1.588.429 B · `/Rotate` +90 | 127.175 B | 153.050 B |
| delete (orta) + save → reopen | 1.585.600 B · 39 s. · dönüş korunur | 123.400 B · 7 s. | 144.218 B · 39 s. |
| compress → reopen | `Compressed` 1.585.600 → 940.051 B (3 görsel) · 39 s. · dönüş korunur | `NoBenefit` (aday +5 B), çıktı yok · son kayıt açılır | `NoBenefit` (aday +7 B), çıktı yok · son kayıt açılır |
| marka | tek pay (`[0 -34 …]`), 2 form nesnesi (form + vektör) her adımda | aynı | aynı |
| kaynak SHA-256 sonra | **aynı** | **aynı** | **aynı** |

Her adımda girdi dosyasının SHA-256'sı da öncesi/sonrası karşılaştırıldı;
fixture dosyalarının diskteki özgün kopyaları da işlem sonrası aynı özeti
verdi.

### Sürüm kapısı

`scripts/release-gate.sh --fast` bütün düzeltmelerden sonra baştan koşuldu
(2026-09-15 15:17:59): **geçen 22 · başarısız 0 · atlanan 2** (paketleme ve
taze klon, `--fast` gereği). Kapsam: `cargo fmt --check`, `cargo clippy
--workspace --all-targets -D warnings`, `tsc --noEmit`, arayüz sözleşmeleri,
vitest, mimari yönü (`pdf-core → document-core`), 6 şekilli feature matrisi,
`cargo test --workspace --locked`, ağ bağımsızlığı, lisanslar, migration
değişmezleri ve kurulu kullanıcı verisinin parmak izi. `pdf_workspace_audit`
67 test (`07e3ee6`'da 44).

## Final rapor — dört zorunlu başlık

### 1. Removed-page privacy

Çıkarılan bir sayfaya ait veri, kalan sayfadan başvuruyla erişilebilse bile
çıktıya yazılmaz. Kopyalayıcı sayfa grafiğinden belge düzeyine geçen her
kapıda durur. Kesilenler:

- çıktıda olmayan sayfa ve sayfa ağacı düğümleri, Catalog;
- yapı ağacı, öğeleri, OBJR/MCR;
- anahat, makale zinciri ve boncukları, PDF 2.0 belge parçaları;
- imza ve zaman damgası sözlükleri;
- kalan sayfalarda olmayan açıklamalar;
- kalan widget'ın atası olmayan form alanları (kalan alanın `/Kids`i
  süzülür);
- hedefi çıkarılmış GoTo eylemleri.

`/Type` isteğe bağlı olduğundan tür adı yazılmamış yapılar da biçimleriyle
kesilir. Kanıt testleri gizli işaretçileri ham çıktı baytlarında arar:
`GIZLI-MUSTERI-2/3`, `GIZLI-KAYIT-DOSYASI-2/3`, `GIZLI-MAKALE`,
`GIZLI-YAZAR`, `GIZLI-P2-METIN`, `GIZLI-ALT`, `GIZLI-W3`, `GIZLI-TCKN-99`.
Yükleyici normalleştirmesi de yalnız null semantiği ekler; hiçbir yapıyı
çıktıya taşımaz.

### 2. Retained document semantics

Kalan sayfalar için korunanlar:

- içerik akışları bayt bayt;
- paylaşılan kaynaklar tek nesne olarak;
- açıklamalar ve popup'ları, widget'lar ve alanları (`/FT /T /V /DA`);
- dış URI ve JavaScript eylemleri (çıkarılan GoTo'nun ardındakiler dâhil);
- kalan sayfaya giden açık, adlı, dolaylı ve tamsayı hedefler (yeni sayfaya
  bağlanır);
- katman görünürlüğü (`/OCProperties` ON/OFF/Order/Locked/RBGroups);
- miras alınan kaynak, kutu ve dönüş.

İmzalı kaynaktan türetilen kopya imzasızdır: görünür imza görünümü kalır ama
imza değeri, `/Perms`, `/DSS` ve `/SigFlags` taşınmaz; çıktı imza
geçerliliğini koruyormuş gibi davranmaz (B16). Marka, türetilmiş kopyada
birikmez (B17). Sayfa kopyalarına taşınMAYAN belge düzeyi yapılar "Kalan
borç" 9'da açıkça listelendi; `07e3ee6`'da da taşınmıyordu, regresyon
değildir.

### 3. Before/after size matrix

Ayrıntılı tablolar: "Boyut ölçümleri — repo fixture'ları" ve "Boyut ölçümleri
— gerçek yazıcılar". Özet:

- **Repo fixture'ları (A–D ve miras C, 40 sayfa):** `07e3ee6`'da Sırala 40 /
  Sil → 39 kaynağın 6,87×–27,38×'i, Seç 10 1,13×–6,87×'i. Son kodla Sırala /
  Sil 1,01×–1,09×, Seç 10 0,21×–0,80×, Seç 1 değişmez (0,15×–0,71×).
- **Gerçek yazıcılar (Chrome A–F, PDFKit C/E/F, Quartz A–D):** `07e3ee6`'da
  Sırala (tüm sayfalar) 3,32×–30,51×, Sil 1 2,94×–29,75×. Son kodla Sırala
  0,77×–1,10×, Sil 1 0,75×–1,09×. `/IDTree` enjekteli PDFKit F'nin "önce"si
  yok: `07e3ee6` onu açamıyordu.
- Her çıktıda font, `FontFile2`, görsel, ICC ve Form XObject sayısı, seçilen
  sayfaların kaynaktaki nüfusuna eşit; aynı baytlı ikinci akış yok.
- 2× ve üzeri anlamsız büyüme hiçbir satırda yok.

### 4. Recoverable vs fatal PDF corruption policy

**Ölümcül (belge reddedilir, çıktı yazılmaz, kaynak değişmez).** Şu
yerlerde eksik nesne:

- sayfa ağacı (`/Kids`);
- bir sayfanın `/Contents`i;
- sayfanın ya da `/Parent` zincirinden miras aldığı `/Resources` ve ondan
  erişilen her şey: font, font programı, Type3 glifi, görsel, SMask, ICC, Form
  XObject ve kaynakları, ExtGState, desen, gölgeleme;
- `/Group`, sayfa kutuları;
- Catalog `/OCProperties`.

Kaynak ad haritalarının her girdisi izlenir (`/P` adlı font da). Şifreli PDF
politika gereği reddedilir.

**Toleranslı (belge açılır, `DanglingReferences` + onarım notu).** Bunun
dışındaki eksik nesne ISO 32000-1 §7.3.10'a göre null sayılır: sözlük girdisi
silinir, dizi öğesi ya da tümüyle başvuru olan nesne `null` olur. Bu
kapsamdakiler:

- açıklama ve görünüm akışı, popup;
- eylem, form alanı, gömülü dosya;
- yapı ağacı (`/IDTree`, `/ParentTree`), anahat, ad ağacı;
- XMP, `/ToUnicode`, `/CIDSet`, `/BoxColorInfo`, `/Info`;
- kökün üstündeki dosyada olmayan `/Parent`.

Ardından DEĞİŞMEMİŞ katı `validate_document` koşar. Kaynağa yazılmaz;
çıktılar onarımsız yeniden açılır.

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
- **İmzalı kaynak:** onaysız reddedilir; onayla yayınlanan kopya imzasızdır,
  görünür imza görünümü kalır (B16).

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
| Open | Chrome F → PDFKit + eksik `/IDTree` nesnesi | açılır, onarım notu, kaynak aynı | ✓ `DanglingReferences` |
| Open | sayfa ağacı/içerik/kaynakta eksik nesne (14 varyant) | reddedilir, çıktı yok | ✓ |
| Signed | imzalı kaynak × 9 araç | çıktı imzasız, görünüm korunur | ✓ |
| Branding | sırala → döndür → sil zinciri | tek pay, tek form | ✓ |
| Perf | 40.000 sayfa düz `/Names`, tümü ters (release) | doğrusal | ✓ 0,200 s (önce 3,648 s) |

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

Final correctness pass (B14, B15):

- `crates/pdf-core/src/pdf/mod.rs` — `extract_pages`, `PageGraphCopy` (tek
  harita, önceden eşlenen sayfalar, zincir kimliği çözümü, belge düzeyi
  kesmeleri ve biçim tanıma, alan ataları ve `/Kids` süzme, hedef yeniden
  bağlama/bırakma, GoTo `/Next` ekleme, ad dizini, katman kopyası);
  `extract_page_range` ve `merge_documents` aynı kopyalayıcıyı kullanır; sayfa
  başına harita açan `copy_object_recursive` kaldırıldı. `strip_signatures`,
  `is_signature_dictionary` (§12.8.1 biçimi; `detect_signature` de kullanır).
- `crates/pdf-core/src/pdf/validate.rs` — `detach_dangling_references`,
  `require_complete_page_graph` (düğüm başına yürüyüş, toleranslı kök üstü),
  `OUTSIDE_APPEARANCE`, `NAME_MAPS`. `validate_document` değişmedi.
- `crates/pdf-core/src/pdf/tolerant.rs` — `canonicalize` (iki katmanda da
  normalize → katı doğrulama), `RepairStrategy::DanglingReferences`.
- `crates/pdf-core/src/pdf/stamp.rs` — idempotent marka:
  `existing_brand_forms`, `brand_visible_on_page` (B17).
- `crates/ekler-core/src/toolbox.rs` — Seç/Sırala/Sil tek `extract_pages`
  geçişi; her yüklenen PDF'te `strip_signatures` (B16).
- Testler: `pdf_workspace_audit.rs` 44 → 67.

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
4. ~~Select/Reorder/Delete paylaşılan kaynağı sayfa başına kopyalıyor~~ —
   **2026-09-15'te kapandı (B15).** Kök neden sayfa başına açılan id
   haritasıydı; tek harita + tek geçiş.
5. ~~Seçimde kalan sayfalar arası bağlantılar `null`~~ — **2026-09-15'te
   kapandı (B15).** Kalan hedef yeniden bağlanır, çıkarılan hedef yazılmaz,
   adlandırılmış hedef açık hedefe çözülür.
6. **`/Type`süz sayfa sözlükleri** (lopdf'in de sayfa saymadığı) hâlâ derin
   kopyalanır; semptomu üretemez, kayda geçti.
7. Çok büyük DCT görselde (`image` crate 512 MiB ayırma sınırı) çözüm hatası
   Failed üretir; önceden de böyleydi.
8. ~~B14 — belge düzeyinde eksik nesneye başvuran PDF açılmıyor~~ —
   **2026-09-15'te kapandı.** Düzeltme: görünüm grafiği ölümcül, dışı null
   sayılır, katı doğrulama yeniden koşar. Not: bu maddenin ilk yazımındaki
   "Önizleme'de imza, vurgu ya da yalnız Kaydet" genellemesi fazlaydı. Kanıt
   yalnız macOS 26.6.2 PDFKit'in Chrome çıktısını yeniden kaydetmesiydi;
   macOS 27.0 PDFKit aynı kaynaktan bu yapıyı üretmiyor.
9. **Sayfa kopyaları belge düzeyi yapıları taşımaz** (Seç/Sırala/Sil,
   Birleştir, Ekler paketi). Taşınmayanlar:
   - `/AcroForm` (`/Fields`, `/DR`, `/DA`, `/NeedAppearances`);
   - anahat;
   - yapı ağacı (etiketli kaynaktan etiketsiz çıktı; `/SD` yapı hedefleri
     düşer);
   - `/PageLabels`;
   - `/Names`'in hedef dışı ağaçları (gömülü dosyalar, belge JavaScript'i);
   - `/Threads`, `/DPartRoot`, `/ViewerPreferences`, `/OpenAction`, XMP
     `/Metadata`;
   - `/OCProperties /Configs` ve `/AS`.

   Kalan widget alanını `/Parent` üzerinden taşır ama `/AcroForm /Fields`'ta
   listelenmez. Görünüm akışı çizilir; bazı görüntüleyiciler onu etkileşimli
   alan olarak tanımayabilir. `07e3ee6`'da da taşınmıyordu (yeni Catalog
   yalnız `/Type /Pages`); regresyon değil, kapsam kararı bekliyor. Yerinde
   yazan araçlar (Döndür, Kırp, Numara, Filigran, Sıkıştır) bu yapıları korur;
   yalnız imza verisi çıkarılır (B16).
10. **Miras kaynak denetimi temkinli:** kök ya da ara Pages düğümünün
    `/Resources`inde eksik nesne, her sayfa kendi `/Resources`ini taşısa ve
    mirası hiç kullanmasa bile ölümcül sayılır. Bilinçli tercih: yanlış kabul
    yerine olası yanlış ret.
11. **Şifreli PDF** politika gereği normalize edilmez, reddedilir; şifresiz
    kopya istenir (önceden de böyleydi).
12. **Ölçeklenme testi süre oranına dayanır**
    (`page_copy_scales_linearly_with_flat_name_arrays_and_off_layer_lists`).
    Eşik 20×; güncel kod paralel pakette 8,4–9,0×. Aşırı yüklü bir makinede
    yanlış alarm olasılığı düşük ama sıfır değil. Düşerse önce tek başına
    yeniden koşulmalı.
13. **İmzalı kaynaktan kopya, basılı imzalı bir belge gibi görünür**
    (görünür imza görünümü kalır) ama imzasızdır. Onay kutusu bunu söylüyor
    ("Türetilmiş PDF kaynak elektronik imzanın doğrulanabilirliğini
    taşımaz"); çıktının kendisinde ayrıca bir "imza doğrulanamaz" işareti
    yok. Ürün kararı.

---

# v0.2.0 — Düşmanca sağlamlaştırma & işkence testi (2026-09-15)

Kapsam **dağıtım değil, DAYANIKLILIK**. Yeni özellik, yeni arayüz, yeni PDF
algoritması yok. Amaç: bütün mevcut özellikleri beklenmedik girdi, ardışık
işlem, büyük/bozuk belge, tekrar, kullanıcı hatası, state geçişi ve dosya
sistemi kusuru altında zorlayıp kırılan yerleri sağlamlaştırmak. Soru her
adımda: *"Bunu nasıl bozabilirim?"* Bütün fixture'lar koddan üretilir; **hiçbir
gerçek kullanıcı belgesi kullanılmadı**, yalnız `tempfile::tempdir()` altına
yazıldı, ağ çıkışı yok, eski kullanıcı-veri dizinlerine dokunulmadı.

## Sürüm
Tek canonical ürün sürümü `0.0.1` → `0.2.0` (workspace Cargo → `CARGO_PKG_VERSION`
/Hakkında/Cargo.lock, tauri.conf.json → Info.plist/DMG, package.json, iki
package-lock kökü, geliştirme IPC taklidi, QA fixture). `tavzih-core` 1.0.1 ve
preflight 0.0.0 ürün sürümünü temsil etmez, dokunulmadı.
`tests/version_consistency.rs` kaynaklar arası fark ya da `0.0.x` yer tutucusunda
düşer.

## Temel (baseline)
Bütün fixler işlendikten sonra ölçülen yeşil temel (bu makinede, CLT
araç zinciri, `DEVELOPER_DIR=/Library/Developer/CommandLineTools`):
`cargo test --workspace --locked` → 49 takım, **719 test, 0 başarısız**, çıkış 0;
frontend `vitest` → 27 dosya, **254 test**, `tsc --noEmit` temiz.
`version_consistency` yeşil (0.2.0). Kaynak değişmezliği: `be49507` HEAD'in atası.

## Test katmanları
- **A — Deterministik regresyon:** mevcut workspace testleri + bu turda eklenenler
  (yukarıdaki 719 + 254 buna dâhil).
- **B — Değişmez/property:** tohumlu fuzz, soak zinciri, boyut/kaynak nüfusu.
- **C — Adversarial fixture:** sentetik corpus (~55 fixture + yükleyici-kabul
  seti), her biri aç/reddet kararını AÇIKÇA taşır; deterministik olmayan ya da
  panic/hang üreten → başarısızlık.
- **D — Soak:** 600 zincir işlem, 200/500 aç-işle-kapat, fd/temp sızıntısı.

Yeni test dosyaları: `crates/ekler-core/tests/hardening_pdf_corpus.rs`,
`hardening_pdf_fuzz.rs`, `hardening_soak.rs`, `hardening_loader_acceptance.rs`
(+ `tests/hardening/` koşum takımı: rng/check/corpus/raw); document-core
`tests/corpus.rs` alan-kodu + kapatılmamış-instrText testleri; belge-shell
`atomic` birim testleri + `ikincigoz_parity_tests.rs` clobber testi.

## Şüpheci turları (≥3 bağımsız tur; "reproduce before fixing")
Her tur "mevcut testlerin kaçırdığı bug bul" göreviyle koştu. Her ajan bulgusu
HİPOTEZ kabul edildi; **kendim yeniden üretmeden (kırmızı test) düzeltme
yapılmadı**, kritik düzeltmeler red-proof edildi (fix öncesi kırmızı → sonrası
yeşil).

- **Tur 1–3 (uygulama sırasında):** yapısal çökme (H1), damga/araç geometrisi
  (H2–H4), state/veri güvenliği (H5–H7), gizlilik/başarısızlık (H8).
- **Tur 4 (bağımsız ajan · PDF structural):** yükleyicinin iki aşırı-reddi
  (H11, H12); H13 (damga yazma yolu) benim agresif uçtan-uca assertion'ımla
  türedi. Ajan araç tarafını (Crop→Rotate→Watermark→Number→Compress, 40 adım
  rastgele zincir, merge dedup, 3-derin Pages ağacı) **kıramadı** — sağlam.
- **Tur 5 (bağımsız ajan · state/publication):** İkinciGöz apply-to-copy'nin
  sessiz clobber + atomik-olmayan yazımı (H10, P0).
- **Tur 6 (bağımsız ajan · privacy/convert):** Word complex-field `instrText`
  alan-kodu sızıntısı (H9).
- **Tur 7 (kendi düzeltmelerime karşı):** H9'un `in_instr` bayrağının
  kapatılmamış instrText'te takılıp metin yutması (H15); `/Contents`
  dereference-etmeme deseninin sistematik taraması (6 tüketici) → boş-sayfa
  tespitinde aynı hata (H14). Diğer tüm /Contents tüketicileri temiz doğrulandı.
  `/Contents` kendine-başvuru döngüsü (yükleyici + damga) kilitlenme üretmiyor
  (lopdf `dereference` döngü korumalı, guard'lı zaman aşımıyla doğrulandı).

## Bulunan ve düzeltilen (ID · önem · özellik · kök neden · fix · red-proof)

| ID | Önem | Özellik | Kök neden | Fix | Red-proof |
|---|---|---|---|---|---|
| H1 | P0 çökme | Yükleyici | lopdf 0.34 iç içe dizi/sözlük ve dolaylı `/Length` zincirini özyinelemeli çözer; derin girdi komut yığınını taşırıp süreci `abort` eder (yakalanamaz) | `pdf/guard.rs` ayrıştırma öncesi doğrusal tarama: yuvalanma>100 ve `/Length` zinciri>50 reddedilir | pre-fix SIGABRT; guard'la kontrollü ret |
| H2 | P1 bozuk işlem | Damga/araçlar | Ters köşeli kutu (`[urx ury llx lly]`) sayfa genişliğini negatif yapıp "sığmıyor" veriyor, tüm araçları düşürüyor | `stamp.rs`+`toolbox.rs` köşe normalizasyonu | ters-köşe fixture kırmızı |
| H3 | P1 bozuk işlem | Damga | Gerçekten küçük sayfaya işaret sığmayınca bütün işlem düşüyor | `apply_stamp_to_page` `Ok(false)` döner, sayfa işaretsiz atlanır | tiny-page fixture kırmızı |
| H4 | P1 yanlış sonuç | Döndür | `/Rotate` dolaylı (`12 0 R`) ya da ondalık (`90.0`) → `as_i64` 0 sayar; 45 (90 katı değil) taşınır | `existing_rotation`: dereference + Integer/Real + 90 katına normalize | rotate fixture kırmızı |
| H5 | P1 veri kaybı | Ayarlar | `load_from` tek yanlış-tipli alanda TÜM ayarları düşürüyordu | Katı yol + alan-bazlı toleranslı kurtarma | pre-fix text_scale 100 (150 beklenir) |
| H6 | P1 veri kaybı | Ayarlar/Sözlük | Yerinde `fs::write`; yarım yazma sonraki okumada varsayılana düşürüp ilk kayıtta kalıcı yapıyordu | `atomic::write` (geçici + fsync + rename) | atomiklik yapısal; litter-guard test |
| H7 | P1 yanlış sonuç | Denetle | `findingKey` `charStart` (motor `char_start`) → tek onay birden çok düzeltmeyi uyguluyor | Anahtar iki yazımı + aralık + mesaj okur | pre-fix `TYPO_SPACE:p3:-1` çakışması |
| H8 | P1 OOM | Dönüştür | DOCX/UDF okuyucu beyan edilen boyuta güvenip `read_to_end` (cap yok); yalan zip bomb GB'larca açılır | `security::read_entry_capped` (sert sınır) | cap kaldırılınca test düşer |
| H9 | P1 gizlilik/yanlış sonuç | Dönüştür | Word complex-field `instrText` alan kodu (HYPERLINK hedefi, REF/PAGEREF yer imi, MERGEFIELD veri adı) bastırılmadan gövde metnine sızıyordu; Word'de gösterilmez | docx reader `in_instr` bastırma durumu | HYPERLINK/REF/MERGEFIELD gövdede; test kırmızı |
| H10 | **P0 veri kaybı** | Denetle | `ikincigoz_apply_fixes` düz `fs::write`: düzenlenmiş önceki `… - İkinciGöz` kopyasını sessizce ezer; atomik değil (yarım→bozuk zip) | `atomic::write_new_unique` (no-clobber + fsync + " (2)" türet) | komut fs::write'a döndürülünce 2. çağrı adı yeniden kullanıp testi kırar |
| H11 | P1 aşırı-ret | Yükleyici | `validate_flate_stream` `total_in != len` eşitliği: FlateDecode akışı sonundaki (üreticinin `/Length`'e kattığı) EOL baytı temiz çözmeyi reddediyordu | temiz çözme kabul; artık baytlar yok sayılır (kesik akış hâlâ çözme hatasıyla ret) | pre-fix "uzunluk tutarsız" |
| H12 | P1 aşırı-ret | Yükleyici | `validate_document` `/Contents` dolaylı→dizi (`5 0 R`→`[6 0 R]`) dereference edilmeden tek stream sanılıp reddediliyordu | `doc.dereference` ile karardan önce çöz | pre-fix "Contents stream eksik" |
| H13 | P1 bozuk çıktı | Damga/araçlar | `apply_stamp_to_page` yazma yolu `/Contents`'i dereference etmiyordu; dolaylı→dizi girdide çıktı doğrulamada düşüyor | yazma yolunda `doc.dereference` (okuma yolu zaten çözüyordu) | uçtan-uca filigran testi kırmızı |
| H14 | P1 yanlış geri bildirim | Düzenle | `detect_likely_blank_pages` dolaylı→dizi `/Contents`'i çözmeden 0 bayt sayıp DOLU sayfayı "boş" (silme adayı) işaretliyor | dizi/stream kararından önce `doc.dereference` | pre-fix dolu sayfa `[1]` boş işaretlendi |
| H15 | P1 veri kaybı | Dönüştür | H9'un `in_instr` bayrağı yalnız `</w:instrText>` ile sıfırlanıyor; bozuk/kapatılmamış instrText sonraki tüm görünür metni yutuyor | run sonunda (`</w:r>`) da sıfırla | pre-fix üç paragraf `"\n\n\n"`e çöktü |

## Fuzz stratejisi ve sonuç
Geçerli corpus fixture'larından tohumlu tek-mutation: bayt çevir, `/Length` boz,
dolaylı başvuruyu olmayana çevir, `/Parent`/`/Contents`/`/Resources` kır, kes,
`xref`/`endobj` boz. Her mutant guard'lı iş parçacığında (2 MiB yığın) + zaman
aşımı ile: **kurtar VEYA güvenle reddet; panic/abort/kilitlenme YOK**. 2500
tohum, deterministik (tohum raporlanır). Sonuç: **0 panik, 0 kilitlenme, 0
sızıntı, 0 kaynak-değişimi**; açılan her mutantta araç ya geçerli çıktı üretti
ya da kaynağı bozmadan/artık bırakmadan kontrollü başarısız oldu. (H11 sonrası
"açıldı" oranı hafif arttı; test aç/reddet sayısına değil değişmezlere bakar.)

## Soak stratejisi ve sonuç
Aynı sentetik PDF üzerinde 100+ işlem zinciri (Seç/Sırala/Sil/Döndür×3/Kırp/
Filigran/Numara), çıktı bir sonrakinin girdisi. Her adım: geçerli+katı yeniden
açılış, doğru sayfa sayısı, İLK kaynak ve her ara girdi SHA-256 değişmez.
5 tohum × 120 = 600 işlem, hepsi yeşil. fd: 4→4 (200 tur / 500 döngü). Geçici
dosya artığı yok.

## Çökme / timeout / bellek değişmezleri
Hiçbir girdi panic/abort/yığın taşması/sonsuz döngü/kilitlenme üretmedi
(guard.rs + fuzz + corpus + döngü probları). Döngülü ad ağacı, dev başvuru
tabloları, derin zincirler ve kendine-başvuran `/Contents` zaman aşımı içinde
güvenle işlenir. Bellek: zip-bomb cap (H8); guard patolojik yuvalanmayı reddeder.

## Özellik bazında durum
- **Düzenle (PDF araçları):** Seç/Sırala/Sil/Döndür/Kırp/Filigran/Numara/
  Sıkıştır/Birleştir — geometri, rotasyon, paylaşılan kaynak, dolaylı/dizi
  `/Contents` ve büyük/derin yapılar altında sağlam; boş-sayfa tespiti düzeltildi
  (H14). 40-adım rastgele zincir + soak yeşil.
- **Dönüştür (DOCX/UDF ↔ PDF):** alan-kodu sızıntısı (H9) ve kapatılmamış-alan
  veri kaybı (H15) kapandı; zip-bomb cap (H8). Batch adlandırma çakışmasız.
- **Karşılaştır:** çekirdek diff deterministik; büyük N·M'de UI donması bilinen
  frontend borcu (aşağıda P2).
- **Denetle (İkinciGöz):** tek-onay-çoklu-düzeltme (H7) kapandı; apply-to-copy
  artık no-clobber + atomik (H10) — kaynak ve önceki kopyalar korunur.
- **Ayarlar/Sözlük/Recents:** toleranslı yükleme (H5) + atomik yazım (H6).
- **Yükleyici:** görüntüleyicinin açtığı iki PDF sınıfı artık açılıyor (H11, H12);
  yapısal çökme guard'ı (H1).

## Kalan borç (bilinçli kabul ya da kapsam dışı)
- **P0:** yok.
- **P1:** yok (bulunanların tümü kapatıldı).
- **P2 / bilinçli:**
  - Yükleyici kalan aşırı-retleri (yanlış `/Length` uzunluğu, kaymış ofset,
    baştaki çöp, metadata döngüsü): nesne-tarayan yeniden kurulum bilinçli
    kapalı — "güvenle reddediyoruz" sınırı; kaynağı asla bozmaz.
  - Tauri sync-command panic → app abort (mimari): girdi-kaynaklı tek onaylı
    abort (yığın taşması) kapandı; kalan yüzeyler için `catch_unwind` ayrı iş.
  - Karşılaştır O(N·M) UI thread donması (frontend cap/timeout gerekir).
  - process-bridge kill yalnız doğrudan çocuk (killpg yok); pencere kapanışında
    orphan olasılığı.
  - `in_deleted` de teorik olarak kapatılmamış `<w:del>`'de takılabilir (H15 ile
    aynı sınıf); Word bunu üretmez, kayda geçti.
  - Araç değiştirmek düzenlemeyi sıfırlar (üstteki "Kalan borç" md. 2).

## Git
Bu turun commit'leri (baseline `be49507` ata olarak korunur):
`0df5ce6` (guard+stamp+rotate), `9b0063b` (settings+atomic+findingKey),
`bf84f21` (zip cap+soak), `e5cb0a2` (0.2.0), `a9ff3a9` (H9), `efc7a6e` (H10),
`a9859f8` (H11–H13), `49176f0` (H15), `5cb124c` (H14). Push/tag/release YOK
(izin bekleniyor).

## Durum
**v0.2.0 HARDENING ACCEPTED** — P0=0, P1=0, ≥3 bağımsız şüpheci turu + kendi
düzeltmelerime karşı ek tur yapıldı, her bulgu yeniden üretilip (kırmızı) sonra
düzeltildi, regresyon/fuzz/soak yeşil, kaynak değişmezliği (`be49507` ata)
korundu, açıklanamayan çökme/sızıntı yok. Kalanlar bilinçli P2.
