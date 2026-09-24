# Saha geri bildirimi turu — 39 madde (2026-09-24)

Kaynak: bir hukukçunun Windows'ta uzun süreli gerçek kullanımından 39 madde.
Taban: `905a537` (0.3.0). Ekran görüntüsü EKLENMEMİŞTİ; görüntüye bağlı
maddelerde tahmin yapılmadı.

Yöntem: her madde önce kaynaktan yeniden üretildi ve sınıflandı (6 alt sistem
denetçisi + her denetim için ayrı şüpheci doğrulayıcı), sonra düzeltildi.
Windows'taki davranış paketlenmiş uygulama üzerinde, gerçek WebView2 ve gerçek
klavye/fare girdisiyle ölçüldü (Kapı 6, `apps/belge-shell/qa/windows/`). AYNI
ölçüm, ürün düzeltmeleri olmayan bir derlemede de koşuldu (yeniden üretim
kanıtı). Ekran görüntüleri iki bağımsız inceleyiciyle okundu (ikincisi
çürütmeye çalışarak); ikisi de görmediyse "görsel doğrulandı" yazılmadı.

Durum sözlüğü: **FIXED** (düzeltildi, test var), **VERIFIED** (Windows'ta
gerçek pencerede ölçüldü ve/veya ekran görüntüsünde iki inceleyiciyle
görüldü), **ALREADY FIXED** (bu turdan önce düzeltilmişti, sahada yeniden
kanıtlandı), **DESIGN PREFERENCE / DEFERRED**, **INSUFFICIENT EVIDENCE**.
"FIXED" tek başına Windows'ta görsel doğrulama İDDİA ETMEZ; "jsdom" arayüz
bileşenlerinin sahte arka uçla koşan testidir, paketlenmiş uygulama değildir.

## Kanıt kaynakları

| | Koşu | Commit | Sonuç (KABUL outcome) |
|---|---|---|---|
| Sonra (ürün düzeltmeleri + harness) | [36022730363](https://github.com/raciyuksekbas-hub/GolgeDosya/actions/runs/36022730363) | `5ece12d` | TÜM KAPILAR GEÇTİ |
| Önce (`905a537` ürünü + aynı harness) | [36022734650](https://github.com/raciyuksekbas-hub/GolgeDosya/actions/runs/36022734650) | `f5e4893` | Yalnız Kapı 6 düştü (saha maddeleri) |
| Uzun yol, düzeltmeden önce | [36007332135](https://github.com/raciyuksekbas-hub/GolgeDosya/actions/runs/36007332135) | `5ea3f2a` | Kapı 4: os error 3 |

Önce/sonra ayrımı: üç harness dosyasının git blob kimliği iki dalda aynı
(`native.ps1` 25cf3375…, `saha-e2e.mjs` 41b9baf6…, `cdp.mjs` 830ddb21…); Kapı 6
iş akışı adımı birebir aynı; önce dalı `905a537`'e göre YALNIZ
`apps/belge-shell/qa/windows/` ve `.github/` dosyalarını değiştirir (ürün
dosyası farkı: yok); sonra dalı önce dalına göre 91 ürün dosyası değiştirir.
İş akışı dosyasının iki dal arasındaki tek farkı, Kapı 6 dışındaki "sürüm
metadata" adımındaki kurulum dil tablosu ölçümü (madde 4) ve iki yorum satırı.

Not: `continue-on-error` kullanan adımların GitHub REST "conclusion" değeri
başarısızlıkta da `success` döner. Bu belgedeki bütün kapı sonuçları KABUL
adımının okuduğu gerçek `outcome` değerleridir. (Bu tur içinde bir ara durum
raporunda bu ayrım gözden kaçmış ve `ff803b6` için Kapı 4–5 yanlışlıkla
"geçti" denmişti; o koşuda Kapı 4 uzun yol testinde düşmüştü.)

## Kök neden kümeleri

1. **Tarayıcının kendi yüzeyi.** Windows'ta sayfaya sağ tıklayınca WebView2'nin
   varsayılan menüsü açılıyordu (runner'da görüldü: Back / Refresh / Save as /
   Print / More tools; kullanıcının Windows'unda Türkçe). F5 / Ctrl+R /
   Ctrl+Shift+R sayfayı yeniden yüklüyordu. Çalışma durumu bellekte yaşadığı
   için bu, işi uyarısız siliyordu (32, P0).
2. **Platform bilgisizliği.** Arayüz Windows'ta çalıştığını bilmiyordu: macOS
   trafik ışığı bandı (1), "Finder'da Göster" (12), "⌘" kısayolları, yalnız `/`
   ile bölünen yollar (tam `C:\Users\…` yolu başlıkta ve son belgelerde).
3. **Taşımada düşen davranış (Değişikİş → Karşılaştır).** Eş zamanlı kaydırma
   işleyicisi yeniden yazılmıştı (22), anahtar (28) ve kopyala (33) düşmüştü,
   "değiştirilen" rengi (24) ve tek kaydırıcı düzeni (20) kaybolmuştu.
4. **Yaşam döngüsü ve kaybettiren eylemler.** Ekler'de bitişten sonra yeni
   işlem yoktu (19); hiçbir yerde "kaydedilmemiş iş" modeli ve onay yoktu (§54).
5. **Windows yol sınırı (Kapı 4'ün bulduğu).** 260 karakteri aşan klasöre
   kayıt "os error 3" ile düşüyordu: `tempfile` yayında yolu Win32'ye `\\?\`
   öneki olmadan veriyor (`885e0b8`).

## 39 madde

| # | Durum | Yapılan | Test | Kalan |
|---|---|---|---|---|
| 1 | FIXED · VERIFIED | Trafik ışığı bandı yalnız macOS'ta (`f3b9586`) | platform.test.ts; Kapı 6: önce bant 40 px FAIL → sonra 0 px PASS; görsel (önce bant var, sonra yok) | Kullanıcının kastettiği "simge" görüntüsüz kesinleşmedi |
| 2 | ALREADY FIXED · VERIFIED | `8e52e1e` (Windows.Data.Pdf) | Kapı 5; Kapı 6 önce/sonra PASS; görsel: küçük resim + büyük önizleme | Kullanıcının kendi dosyasıyla deneme |
| 3 | ALREADY FIXED · VERIFIED | Aynı | Kapı 6: Edge metin/görsel PDF'i, vektör, taranmış, sayfa değiştirme; görsel | — |
| 4 | FIXED | NSIS `["English","Turkish"]`, seçim penceresi yok (`24dcc59`) | installer.test.ts; CI: makensis "2 language table" | Türkçe Windows'ta kurulum ekranı görülmedi (runner İngilizce) |
| 5 | INSUFFICIENT EVIDENCE (+2 ölçülebilir düzeltme) | Ek damgası metni kutuda ortalı (`a2ed22a`); belge satırı ızgarası (`827fe0a`) | stamp_badge.rs (sapma 8,0/38,2 pt → ≤1 pt) | Hangi kutu kastedildi: görüntü gerekli |
| 6 | FIXED · VERIFIED | Ekler boş durumu: başlık + açıklama + "Belge Ekle" (`827fe0a`) | annexWorkspace.test.tsx; görsel (08) | Başlık emir kipinde değil (tasarım kuralı): "Dilekçe ekleri, tek pakette" |
| 7 | INSUFFICIENT EVIDENCE | — | — | Metin/görüntü yok |
| 8 | FIXED · VERIFIED | `color-scheme`, açılır liste (1,00:1 idi), tipsiz alanlar, tanımsız token, pencere çerçevesi (`9e1f30e`) | darkNative.test.ts; Kapı 6: `color-scheme` önce normal → sonra dark; başlık çubuğu DWM önce açık → sonra koyu (değer 1); görsel: 5 kip × 3 boyut + yüklü Karşılaştır/Denetle | Ray gezinme oku koyu temada 2,4:1 (<3:1), önce de aynı (P2) |
| 9 | FIXED | Sonuç yüzeyinde iki dosya ayrı satır (`827fe0a`) | annexLifecycle.test.tsx (jsdom) | Windows'ta sonuç ekranı görüntüsü yok |
| 10 | FIXED · VERIFIED | "Klasörü Aç" paketin kendisini açar (`827fe0a`) | ekler.rs; Kapı 6 + görsel: Gezgin paket içinde (EK-01, EK-02, EKLER_LISTESI, manifest) | Önce tarafı ölçülemedi: eski derlemede komut yoktu |
| 11 | FIXED | "Son belgeleri göster" (`78d0ae8`) | settings.rs, lib.rs, recents.test.tsx | Windows görüntüsü yok |
| 12 | FIXED | "Klasörde Göster", ⌘/Ctrl platforma göre (`727daa2`, `f3b9586`) | convertWorkspace.test.tsx | Tamamlanma ekranı görüntüsü yok |
| 13 | FIXED · VERIFIED | Çıktı klasörünün kendisi, dosya seçili (`727daa2`) | reveal_plan; Kapı 6 + görsel: önce üst klasör `GölgeDosya` → sonra `Dönüştürülen Belgeler`, `Dilekçe (temiz).udf` seçili | — |
| 14 | Dokunulmadı (olumlu sinyal) | — | Mevcut korpus | — |
| 15 | FIXED (kısmen) / DEFERRED | Uyarı dürüst, bilgi satırı alarm değil (`8c1cdcf`) | corpus.rs c34; convertCalm.test.tsx | Hiza korunmuyor: UDF'nin hizayı nasıl kodladığını gösteren örnek yok |
| 16 | FIXED | "Yeni Dönüştürme" (`727daa2`) | newConversion.test.tsx (jsdom) | Canlı ikinci dönüştürme Kapı 6'da yok |
| 17 | FIXED · VERIFIED | Akış ortada, taşma yok (`8c1cdcf`) | convertCalm.test.tsx; görsel (900×600, 1280×800, tam ekran) | — |
| 18 | INSUFFICIENT EVIDENCE | — | — | Hangi ekran: görüntü gerekli |
| 19 | FIXED | Yaşam döngüsü, "Yeni Ekler İşlemi", onay (`827fe0a`) | annexLifecycle.test.tsx (jsdom) | **Kapı 6'da kanıt YOK** (canlı akış yerel dosya iletişim kutusu ister) |
| 20 | FIXED · VERIFIED (gerileme) | Tek kaydırma sahibi (`ba3b546`) | compareVisual.test.ts; Kapı 6: önce `main.content-body` de kayıyor (685>675) → sonra yalnız iki belge paneli; görsel (açık/koyu) | Rayda "04" etiketi çakışmada kayboluyor (önce de aynı, P2) |
| 21 | FIXED · VERIFIED | Belge yüzeyinde Ctrl+V (`87701ee`) | clipboard.rs, paste.test.tsx; Kapı 6 + görsel: önce FAIL → sonra `vektör.pdf` açıldı | Açık karşılaştırmada yapıştırma yok (Değiştir kullanılır) |
| 22 | FIXED (gerileme) | Değişikİş işleyicisi geri (`5cacb43`) | paneSync.test.ts (kare döngüsü modeli) | Windows dokunmatik yüzeyle elle deneme yapılmadı |
| 23 | DESIGN PREFERENCE | Değişiklik yok (değerlendirme aşağıda) | — | — |
| 24 | FIXED · VERIFIED (gerileme) | Değiştirilen kendi renginde, boş halka (`ba3b546`) | compareVisual.test.ts; görsel: üç tür açık ve koyu temada ayırt ediliyor | — |
| 25 | FIXED · VERIFIED | Dinlenen işaretler (`ba3b546`, `9e1f30e`) | compareVisual.test.ts; görsel (açık/koyu) | Ray gezinme okları soluk (P2, önce de aynı) |
| 26 | INSUFFICIENT EVIDENCE — kök neden yüzeyi kaldırıldı (VERIFIED) | Sayfa menüsü yok (`9a3f307`) | Kapı 6 + görsel: önce menü (44 376 px değişim) → sonra yok (0 px, olay engellendi) | Metin alanı menüsü "Import passwords", "More tools" gösteriyor (P2) |
| 27 | DESIGN PREFERENCE (ihtiyaç karşılandı) | Paylaş yok; "Değişiklikleri Kopyala" | syncAndCopy.test.tsx | — |
| 28 | FIXED · VERIFIED (gerileme) | "Eş zamanlı kaydır" (`5cacb43`) | Kapı 6 + görsel: önce yok → sonra var | Aç/kapa davranışı jsdom'da |
| 29 | FIXED | Ürün işareti ana sayfaya, bekçiden geçer (`564086c`) | leaveWiring.test.tsx (jsdom) | Windows'ta tıklanmadı |
| 30 | DESIGN PREFERENCE / DEFERRED (spec gereği uygulanmadı) | Değerlendirme aşağıda | — | — |
| 31 | FIXED | "Değişiklik içeren paragraf" (`99c93bd`) | terminology.test.ts | — |
| 32 | FIXED · VERIFIED (P0) | Tarayıcı menüsü ve F5/Ctrl+R/Ctrl+Shift+R kaldırıldı (`9a3f307`); onay (`564086c`) | Kapı 6: önce üçü de durumu siliyor → sonra korunuyor | — |
| 33 | FIXED · VERIFIED (gerileme) | "Değişiklikleri Kopyala" + seçilebilir metin (`5cacb43`) | Kapı 6: `user-select: text` + düğme; görsel | Panoya gerçek kopyalama jsdom'da |
| 34 | DESIGN PREFERENCE | Değişiklik yok (ölçüm aşağıda) | — | — |
| 35 | FIXED · VERIFIED | Boşluk bulgusu görünür (`68c7db0`); satır sonu "çift boşluk" değil (`1133f5a`) | whitespaceHighlight.test.tsx, soft_break.rs; görsel (Denetle yüklü, açık/koyu) | Vurgu tonu düşük kontrastlı, noktalı işaret + alt çizgiyle seçiliyor (P2) |
| 36 | FIXED | Türkçe harf farkında öneri (`4f941ce`) | consistency.rs, term_fix.rs | Birleşik/büyük harf çeşitleri (P2) |
| 37 | FIXED (beklenti) / DEFERRED (yazım denetimi) | Metin yalnız yapılanı söyler (`b4a8fe3`) | copy_promise.rs; denetle-olcum.md | Genel yazım denetimi ürün kararı |
| 38 | FIXED · VERIFIED | "Save as" menüden kalktı; "zaten var" mesajı (`9540b64`); 260+ karakterlik klasöre kayıt (`885e0b8`) | Kapı 6: menü önce var → sonra yok; Ctrl+S iletişim kutusu açmıyor (önce de açmıyordu); Kapı 4 (Windows): önce os error 3 → sonra PASS | — |
| 39 | INSUFFICIENT EVIDENCE — sayfadan kaldırıldı | Sayfa menüsü (ve "More tools") yok | Kapı 6 + görsel | Metin alanlarında "More tools" hâlâ var (P2, bkz. 26) |

Ek (denetimde bulunan, maddelerin dışında): Karşılaştır "Değiştir" yolsuz dosya
alıyor, eski belge kiplere taşınıyordu (`dd69ecb`); Windows kayıt/mesaj yol
sızıntısı (`9540b64`); Denetle dosyasındaki ham NUL baytları (`68c7db0`);
Windows uzun yol (`885e0b8`). Karşılaştır motorunda binlik ayraçta cümle bölme
şüphesi ayrı işe ayrıldı.

## Ayrım (§63)

**Doğrulanmış hatalar (düzeltildi):** 32 (P0, veri kaybı), 13, 10, 21, 38
(uzun yol, tarayıcı "Save as"), 1, 8, 12, 15 (yanıltıcı uyarı), 35, 36, 9, 11,
16, 19.

**Düzeltilen gerilemeler (Değişikİş'ten):** 20, 22, 24, 28, 33.

**UX iyileştirmeleri:** 6, 17, 25, 29, 31, 37 (metin), 4.

**Ertelenen ürün tercihleri:** 23, 30, 34, 27 (paylaş yerine kopyala), 37
(genel yazım denetimi), 15 (hiza).

**Kanıt yetersiz:** 5, 7, 18, 26, 39.

Açık kalan P0/P1: **yok**. Açık P2'ler (hiçbiri gerileme değil; önce
derlemesinde de var): metin alanı menüsündeki tarayıcı öğeleri (26/39); rayda
etiket atlama, soluk gezinme okları (koyu temada 2,4:1) (20/25/8); küçük resim
şeridinde iki vurgu; boşluk vurgusunun düşük kontrastlı tonu (35); birleşik
yazım terim çeşitleri (36).

## Değerlendirmeler (kod yok)

**23 — Önce özet, sonra metin?** Özet → ayrıntı zaten panelde (sayılar,
seçili fark, süzgeç, liste). Avukat karşılaştırmaya doğrudan metinden
başlıyor; eski Değişikİş de öyleydi. Öneri: yeniden sıralama yok.

**30 — Ana sayfa + araç kartları.** Artısı: ilk kullanımda keşfedilebilirlik.
Eksisi: bütün gün kullanılan bir araçta fazladan gezinme katmanı; her zaman
görünen kip değiştiricinin kaybı; ürünün tasarım kuralı (kart yığını yok) ve
masaüstü alışkanlığıyla çelişki. Öneri: kenar çubuğu kalsın;
keşfedilebilirlik boş durum metinleriyle (6) çözüldü.

**34 — İki yana yaslı metin.** Ölçüldü: 301 fark vurgusunun 276'sını geriyor
ve dar panellerde Türkçe heceleme olmadan geniş boşluk açıyor. Varsayılan sola
yaslı kaldı (P2).

## Gerçek akışlar (§43 / §64)

| Akış | Önce (aynı harness) | Sonra | Canlı (Windows, paketlenmiş uygulama) | Kalan sürtünme |
|---|---|---|---|---|
| A. Ekler | Hazırla komutu yoktu; "Klasörü Aç" üst klasörü açıyordu (kod) | Hazırla → paket → Klasörü Aç → Yeni Ekler İşlemi | Hazırla (Unicode profil, 2 kaynak, 2 ek) PASS; Klasörü Aç paketin kendisi PASS | Yükleme/atama/dağıtma ve "Yeni Ekler İşlemi" yalnız jsdom'da |
| B. Dönüştür | "Klasörde Göster" üst klasör | Çıktı klasörü, dosya seçili | DOCX→UDF PASS, kaynak bayt bayt aynı; Gezgin doğru klasör PASS | Tamamlanma ekranı + ikinci dönüştürme yalnız jsdom'da |
| C. Karşılaştır | İki kaydırıcı; anahtar ve kopyala yok | Tek kaydırıcı, anahtar, kopyala, seçilebilir | İki DOCX, açık ve koyu: PASS | Aç/kapa, Değiştir, kirli durumla yenileme jsdom'da; paylaş yok (27) |
| D. Denetle | Boşluk bulgusu görünmüyordu | Görünür, terim önerisi | Yüklü ekran (açık/koyu): bulgular ve vurgu görülüyor | Genel yazım denetimi yok (37) |
| E. Windows PDF | Önizleme zaten çalışıyordu | Aynı | 5 PDF: küçük resim, büyük önizleme, sayfa değiştirme PASS; yeni PDF kaydı PASS | Döndür → kaydet UI'dan sürülmedi (motor testleri Windows'ta PASS) |

## Denetle ölçümü (§65)

Ayrıntı: `docs/audit/2026-09-24-denetle-olcum.md`. 10 kategori, yaklaşık 45
yerleştirilmiş kusur; 19 bulgu (17'si doğru yerde). Temiz metinde 1 yanlış
pozitif (`sınır`/`sinir`; artık düzeltme önerilmiyor). Yazım: 18 yaygın
yanlıştan 4'ü (yalnız bitişik/ayrı yazım listesi); genel yazım denetimi yok ve
metin bunu artık vaat etmiyor. Terim önerisi: 2 grubun 2'si doğru yazımı
öneriyor (önce ikisi de doğru yazımı işaretliyordu).

## Windows (§66)

| Konu | Sonuç | Kanıt |
|---|---|---|
| Önizleme | PASS | Kapı 5 (Windows.Data.Pdf gerçek render testleri); Kapı 6: 5 PDF, küçük resim + etkin sayfa + sayfa değiştirme; görsel |
| NSIS | PASS | Tek paketleme geçişi, yeniden deneme yok, HTTP 5xx yok |
| Türkçe kurulum | PASS (ölçüm) / görsel yok | makensis "2 language table"; Türkçe Windows'ta ekran görülmedi |
| Kurulum smoke | PASS | Kur, 20 sn çalıştır, kaldır; dizin temiz |
| Unicode yollar | PASS | Profil `Çağrı Şahin`, klasör `Müvekkil'in Dosyası`: dönüştürme, Ekler, açma, Gezgin |
| 260+ karakterlik yol | PASS | Kapı 4 (Windows): 3 test; önce os error 3 (koşu 36007332135) |
| Klasörü göster | PASS | 13 ve 10: Gezgin penceresinin yolu ölçüldü + görsel |
| Save As | PASS | Tarayıcının "Save as"ı menüyle birlikte kalktı; uygulamanın kaydı gerçek Windows yollarında PASS; Ctrl+S runner'da önce de iletişim kutusu açmıyordu (yeniden üretilemedi) |
| Yerel testler | PASS | `cargo test --workspace --locked` Windows'ta 71 grup, 0 düşen |
| Manuel görsel | YAPILMADI | Gerçek bir Windows makinesinde insan gözüyle kontrol yok; otomatik görüntüler iki inceleyiciyle okundu |

Harness hijyeni: HKLM `…\WebView2\AdditionalBrowserArguments` değeri iş
sonunda yok (harness kontrolü + iş akışının bağımsız kontrolü, iki koşuda).
Test verisi yalnız `RUNNER_TEMP\saha` altında ve ayrı kanıt artifact'ında;
ürün artifact'ı yalnız 5 dosya, boyutları Kapı 6 öncesiyle aynı, SHA-256
runner = yerel. Ekran çözünürlüğü gerçekten değişti (1024×768 → 1920×1080,
ChangeDisplaySettings sonucu 0): 1280×800 görüntüler tam pencere.
NSIS yeniden denemesi yalnız `failed to bundle project: \`http status: 5xx\``
deseninde çalışır; gerçek HTTP 500 günlüğünde eşleşiyor, derleme/test/imza/404
hatalarında eşleşmiyor (ilk denemede düşer). Yeniden deneme yolu canlı
tetiklenmedi (geçici hata olmadı).

Saha adayı: `dist/candidates/5ece12d/windows/GolgeDosya_0.3.0_5ece12d_x64-setup.exe`
(SHA-256 `e1c07416…f7d0`), taşınabilir `GolgeDosya_0.3.0_5ece12d_x64-portable.exe`
(`74b3f25f…4b77`). GitHub Release oluşturulmadı.

## Arayüz (§67)

| Kip | Açık | Koyu | Not |
|---|---|---|---|
| Ekler | PARTIAL | PARTIAL | Yalnız boş durum görüntülendi (3 boyut, iki inceleyici); hazırlama ve sonuç ekranları yok |
| Dönüştür | PARTIAL | PARTIAL | Yalnız boş durum; tamamlanma ekranı yok |
| Karşılaştır | PASS | PASS | Boş + iki belge yüklü; koyu: gezinme oku 2,4:1 (P2) |
| Denetle | PASS | PASS | Boş + yüklü belge, bulgular ve vurgu; vurgu tonu düşük kontrastlı (P2) |

## Sürüm önerisi (§59)

Sürüm değiştirilmedi. Öneri: **0.3.1**. Tur saha düzeltmeleri ve geri
getirilen gerilemelerden oluşuyor; ayar dosyası geriye uyumlu
(`show_recents` varsayılanlı), yeni modül yok. Yeni yetenekler (Ctrl+V ile
açma, son belgeleri gizleme, değişiklikleri kopyalama) küçük kolaylıklar.

## Durum

`REAL-USER FIELD FEEDBACK PARTIAL`

Açık P0/P1 yok; Windows CI yeşil; veri kaybı davranışı (32) Windows'ta
önce/sonra kanıtlı olarak kapandı; matris tam. ACCEPTED denmemesinin tek
nedeni kanıt eksiği, bilinen bir hata değil: §43, sentetik testi yeterli
saymıyor. A (Yeni Ekler İşlemi), B (ikinci dönüştürme) ve C (Değiştir, aç/kapa,
kirli durumla yenileme, panoya kopyalama) adımları paketlenmiş uygulamada
sürülmedi, yalnız jsdom'da geçti. Gerçek bir Windows makinesinde insan
kontrolü de yapılmadı. Saha adayıyla bu adımların elle denenmesi ya da Kapı
6'ya dosya iletişim kutusu sürücüsü eklenmesi durumu ACCEPTED'a taşır.
