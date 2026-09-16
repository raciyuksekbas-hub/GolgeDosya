# GölgeDosya — sıfırdan ürün kabulü (2026-09-16)

Bu tur, önceki kabul raporlarını **doğru varsaymadan** yapıldı. Ne testlerin
yeşil olması, ne `docs/audit/` altındaki "ACCEPTED" kayıtları, ne kodda bir
fonksiyonun bulunması, ne de arayüzde bir düğmenin görünmesi kanıt sayıldı.
Her iddia ya gerçek komut çalıştırılarak, ya gerçek çıktı üretilip yeniden
açılarak, ya da çıktı ekrana render edilip **gözle görülerek** doğrulandı.

## 1. Baseline (kod değiştirilmeden önce)

```text
branch                 main
HEAD                   d60ea28
git status             temiz
sürüm                  0.2.0  (package.json · workspace Cargo.toml · tauri.conf.json — üçü tutarlı)
bundle identifier      tr.yuksekbas.golgedosya
productName            GölgeDosya
mimari taban           be49507 → HEAD'in atası ✅
platform               macOS 27.0 · arm64
araç zinciri           rustc 1.97.1 · node v26.7.0 · DEVELOPER_DIR=CommandLineTools
harici bağımlılık      LibreOffice yok (Office→PDF yolu bu makinede ölçülemez)
```

v0.2.0 hedef sürümü üç kaynakta da doğru. Çalışma ağacı temizdi; hiçbir
mevcut değişiklik ezilmedi.

## 2. Yöntem

* **Ürün kabul takımı** (`apps/belge-shell/src-tauri/tests/product_acceptance.rs`)
  yazıldı: motor fonksiyonlarını değil, arayüzün gerçekten çağırdığı
  `#[tauri::command]` yüzeyini çağırır → gerçek dosya üretir → çıktıyı yeniden
  açar → semantiğini ve kaynak SHA-256 değişmezliğini doğrular.
* **Görsel doğrulama**: üretilen PDF'ler Quartz (`qlmanage`) ile PNG'ye render
  edilip incelendi. Filigran bulgusu ancak böyle görülebilirdi (aşağıya bakınız).
* **Paralel envanter**: 8 salt-okunur ajan sekiz kullanıcı yüzeyini kod
  düzeyinde haritaladı (UI → handler → command → motor → çıktı), ardından her
  bulgu bağımsız şüphecilere çürütülmek üzere verildi. 120 ajan, 90 ham bulgu →
  **62 onaylandı, 28 elendi**. Ajan bulguları HİPOTEZ kabul edildi; hiçbiri
  kendim yeniden üretmeden düzeltilmedi.

## 3. Kullanıcı yüzeyi envanteri ve UI → motor haritası

Sekiz yüzey haritalandı; **244 kontrol** için UI → handler → command → motor
zinciri tek tek izlendi:

```text
tam bağlı (full)      155
yarım bağlı (partial)  62   ← handler var ama sonuç kullanılmıyor / state'e yazılmıyor
ölü (dead)             27   ← hiçbir kullanıcı yolundan ulaşılamıyor
```

Komut yüzeyi bütünlüğü (kodla ölçüldü, iddia değil):

```text
backend #[tauri::command]                50
frontend'den invoke edilen               26
arayüzden HİÇ ulaşılamayan               24
backend'de OLMAYAN invoke (runtime kırık) 0   ✅
```

**0 hayalet çağrı** — frontend'in çağırdığı her komut backend'de var ve
`invoke_handler`'a kayıtlı. Ad/argüman uyuşmazlığından doğan sessiz runtime
kırılması **yok**. `lib.rs`'teki altı `invoke_handler` bloğu birbirini ezmiyor:
feature kombinasyonlarına göre karşılıklı dışlayan `#[cfg]` kapıları ve
desteklenmeyen kombinasyonu derleme zamanında reddeden bir `compile_error!`
var — doğru kurulmuş.

Ulaşılamayan 24 komut ölü kod değil, **bağlanmamış yetenek**: Düzenle'nin
17 komutu (birleştir/böl/sil/döndür/görsel→PDF/boş sayfa tespiti/proje
kaydet-yükle/UYAP paketi/Office→PDF), Denetle'nin 6 sözlük-profil-kural komutu
ve `tavzih_inspect_file`. Bunlar P1 olarak aşağıda kayıtlı.

## 4. GölgeDosya filigranı (ayrı başlık — bu turun kilit bulgusu)

**Semptom.** Düzenle'nin ürettiği HER PDF'in sağ alt köşesine eklenen marka
işareti hâlâ birleşme öncesi kimliği taşıyordu: eski logo + **"DüzenEk"**
kelime işareti. Kullanıcının ürettiği her belgede görünüyordu.

**Neden daha önce yakalanmadı.** Bu bug **metinsel değil GÖRSELDİ**. Eski
kelime işareti VEKTÖR YOLLARI olarak çiziliyordu — çıktı baytlarında
`"DüzenEk"` DİZESİ HİÇ GEÇMİYORDU. Dize arayan her legacy-name taraması
(önceki turlar dâhil) bunu kaçırdı. Ancak gerçek çıktı üretip **render
ederek** görülebildi.

```text
eski kaynak    crates/pdf-core/assets/brand-logo.ops   6718 bayt
               (ikon + "DüzenEk" kelime işareti, vektör dış hat)
yeni kaynak    aynı dosya                               173 bayt
               ("gölge dosya" ikonu: üst üste iki yaprak +
                Helvetica-Bold "GölgeDosya" kelime işareti)
BBox           290×72 — DEĞİŞMEDİ, yerleşim matematiği aynı
font           base-14 Helvetica-Bold, gömülü font yok
```

Kelime işareti artık gerçek metin olduğu için çıktıda **aranabilir ve
kopyalanabilir**; asset 6718 → 173 bayta indi.

**Gerçek çıktıyla doğrulama** (hepsi üretildi, açıldı, render edildi):

| Senaryo | Sonuç |
|---|---|
| 1 sayfa | ✅ sağ altta, dik, okunur "GölgeDosya" |
| 10 / 100 sayfa | ✅ tek paylaşılan form; sayfa başına çoğalmıyor |
| dikey / yatay | ✅ doğru konum |
| `/Rotate` 90 / 180 / 270 | ✅ işaret DİK ve okunur — sayfayla ters dönmüyor (görsel) |
| karışık sayfa boyutu | ✅ |
| idempotentlik (filigran→döndür→sıkıştır→döndür) | ✅ marka birikmiyor, tek katman |
| **eski DüzenEk markalı belge yeniden işlenirse** | ✅ işaret YERİNDE yükseltiliyor; **çift filigran yok** (görsel) |
| boyut etkisi | 48 208 → 45 743 bayt (**küçüldü**) |
| kaynak SHA-256 | değişmiyor |
| eski ad taraması (çözülmüş akışlar dâhil) | "DüzenEk"/"DuzenEk" **yok** |

**Çift filigran riski ve çözümü.** Marka formu baytlarıyla tanınır. Asset
değiştiği için eski sürümle markalanmış bir belgedeki işaret "marka yok"
sayılacak ve üstüne İKİNCİ bir GölgeDosya işareti eklenecekti — yani yasaklanan
"DüzenEk + GölgeDosya" çifti. `upgrade_legacy_brand_marks` eski çizimi YERİNDE
canonical çizime yükseltir: hem çift işaret önlenir hem eski kimlik kullanıcı
çıktısında hiç kalmaz.

**Regression**: `crates/ekler-core/tests/brand_identity.rs` (5 test). Dizeye
değil **ÇİZİME** bakar. **Mutation proof**: asset legacy'ye geri konunca 5
testten 4'ü kırmızıya düşüyor.

İç tanımlayıcılar da taşındı: `DuzenEkBrand` → `GolgeDosyaBrand`,
`DuzenEkStamp` → `GolgeDosyaStamp`.

## 5. Bulunan ve düzeltilen buglar

| ID | Önem | Özellik | Semptom / kök neden | Regression | Mutation proof |
|---|---|---|---|---|---|
| U1 | **P1** | Marka | Türetilen her PDF'in sağ alt işareti "DüzenEk" (vektör çizim; dize taramaları kaçırıyordu) | brand_identity.rs (5) | ✅ 4/5 kırmızı |
| U2 | **P1** | Marka | Eski markalı belge yeniden işlenince DüzenEk + GölgeDosya çift filigranı oluşacaktı | brand_identity.rs | ✅ |
| U3 | **P1** | Düzenle | Türkçe filigran reddediliyordu ("GİZLİ", "ÖRNEKTİR"); metin `Tj`'ye ham UTF-8 veriliyordu, WinAnsi fontta mojibake olurdu — ASCII kısıtı bunu gizliyordu | product_acceptance.rs + pdf_workspace_audit | ✅ (test eski kısıtı kilitliyordu) |
| U4 | **P1** | Ayarlar | Tercihler'de tema değiştirmek, aynı oturumda seçilen çıktı klasörünü ve kabul edilen kullanım koşullarını sessizce siliyordu (`save_settings` blind overwrite) | lib.rs birim testi | ✅ kırmızı → yeşil |
| U5 | **P1** | Migration | Bozuk eski `settings.json`, SÖZLÜK ve PROFİL devrini iptal ediyordu (erken `return`, kopyalamadan önce) — yükseltmede kalıcı kayıp | legacy.rs 2 birim testi | ✅ kırmızı |
| U6 | **P1** | Düzenle | Filigran metni ile PDF→Görsel biçim seçici AYNI state'i paylaşıyordu; JPG seçip filigrana geçen kullanıcı belgeye **"jpg"** damgalıyordu | typecheck + vitest | — (frontend) |
| U7 | **P1** | Düzenle | Araç değiştirmek açık belgeleri, sayfa sırasını ve dönüşleri UYARISIZ siliyordu | vitest | — |
| U8 | **P1** | Dönüştür | Toplu işte motor ZIP üretip tekil çıktıları siliyor; sonuç ekranı diskte **olmayan** dosya adlarını listeliyordu | vitest | — |
| U9 | **P1** | Düzenle | Önizleme, `/Rotate` dolaylı başvuru ya da ondalıkken dönüşü kaybediyordu → **ekranda görülen ile kaydedilen ayrışıyordu** | mevcut rotate fixture'ları | — |
| U10 | **P1** | Erişilebilirlik | Tercihler'de her ayar değişikliğinde klavye odağı ilk sekmeye fırlıyordu (odak tuzağı her render'da yeniden kuruluyor) | typecheck + vitest | — |
| U11 | **P1** | Marka | Karşılaştırma raporu (DOCX/HTML/MD/JSON) baştan sona "Değişikİş" markalıydı; dosya adı `DegisikIs-Rapor-*` | frontend testleri güncellendi | — |
| U12 | **P1** | Marka | Kullanım Koşulları (hukuki metin) ürünü "Tavzih" diye tanıtıyor ve sorumluluğu o ad adına reddediyordu | — | — |
| U13 | **P1** | Marka | Üretilen adlar: `~/Documents/Tavzih/…`, `Tavzih_Dönüşüm_*.zip`, `… - İkinciGöz.docx`, `DuzenEk-Gorseller-…`, `DuzenEk-Markdown-…`, `DuzenEk-<proje>-…` | mevcut testler güncellendi | — |
| U14 | **P1** | Düzenle | Görseller → PDF aracında HER küçük resim "önizleme oluşturulamadı" hatası gösteriyordu (PDF önizleme motoru görsele uygulanıyordu) | typecheck + vitest | — |
| U15 | P2 | Dönüştür | `tavzih_inspect_file` yalnız UZANTIYA bakıyordu: `.docx` uzantılı düz metin "Word (.docx) → UDF" diye listelenip çıktı adı vaat ediliyordu (sahte vaat) | product_acceptance.rs | ✅ kırmızı |
| U16 | P2 | Denetle | Kullanıcıya gösterilen iki kural gerekçesi "İkinciGöz" adıyla konuşuyordu | — | — |

## 6. Mod bazında test matrisi (gerçekten çalıştırılanlar)

| Mod | Senaryo | Girdi | Beklenen | Gerçekleşen | Sonuç |
|---|---|---|---|---|---|
| Düzenle | Seç / Sırala / Sil / Döndür / Kırp / Filigran / Numara — **production komutundan** | 6 sayfa sentetik PDF | çıktı açılır, sayfa sayısı/semantik doğru, kaynak değişmez | hepsi | ✅ |
| Düzenle | Sırala gerçekten sırayı değiştiriyor mu | 4 sayfa, ters sıra | 1. sayfa "Sayfa 4" olmalı | öyle | ✅ |
| Düzenle | Döndürme çıktıya yansıyor mu | Rotate 90 | `/Rotate 90` | öyle | ✅ |
| Düzenle | Birleştir / Böl / Sil / Döndür (ayrı komutlar) | 5+3 sayfa | 8 / 2 / 3 sayfa, iki kaynak da değişmez | öyle | ✅ |
| Düzenle | Boş sayfa tespiti | 3 sayfa, 2. boş | `[2]` | `[2]` | ✅ |
| Düzenle | PDF → PNG | 2 sayfa | gerçek PNG dosyaları | üretildi | ✅ |
| Düzenle | Türkçe filigran | GİZLİ / ÖRNEKTİR / SURETİDİR / ÇĞİÖŞÜ | kabul + doğru glif | render'da doğrulandı | ✅ |
| Düzenle | Yazılamayan karakter (emoji) | "GİZLİ 🔒" | açık ret | açık ret | ✅ |
| Dönüştür | Gerçek DOCX tanıma | örnek dilekçe | DOCX→UDF | öyle | ✅ |
| Dönüştür | Sahte `.docx` (düz metin) | 17 baytlık metin | açık ret | (fix sonrası) ret | ✅ |
| Karşılaştır | Belge okuma komutu | gerçek DOCX | içerik döner | döner | ✅ |
| Karşılaştır | Olmayan dosya | yok.docx | açık hata | açık hata | ✅ |
| Denetle | Analiz → kopyaya uygula → yeniden aç | hatalı örnek dilekçe | düzeltme uygulanır, çıktı açılır, **kaynak değişmez** | öyle | ✅ |
| Ayarlar | Modül yazımından sonra tercih kaydı | çıktı klasörü + tema | ikisi de korunur | (fix sonrası) korunur | ✅ |
| Migration | Bozuk legacy settings | bozuk JSON + sözlük + profil | sözlük/profil yine taşınır | (fix sonrası) taşınır | ✅ |
| Migration | Tek yanlış tipli alan | textScale="buyuk" | sağlam alanlar taşınır | taşınır | ✅ |
| Marka | 1/10/100 sayfa, yatay, 90/180/270, karışık | sentetik | tek GölgeDosya işareti, dik | görsel doğrulandı | ✅ |
| Marka | Eski markalı belge | DüzenEk formu gömülü | yükseltilir, çift işaret yok | görsel doğrulandı | ✅ |

## 7. Eski ürün adı denetimi — şu anki durum

Kullanıcıya görünen 13 nokta tek tek denetlendi, **hepsi temiz**:
varsayılan çıktı kökü, toplu ZIP adı, Denetle çıktı adı, PDF→Görsel klasörü,
UDF→Markdown paketi, ek paketi dizini, marka XObject adı, damga font adı,
Kullanım Koşulları metni, karşılaştırma raporu markası (HTML/MD/JSON/DOCX),
rapor dosya adı öneki, marka çizimi.

Dokunulmayanlar (doğru olan): crate ve modül adları, migration kimlikleri
(`tr.yuksekbas.duzenek` vb. — bunları değiştirmek kullanıcının eski verisini
bulunamaz yapardı), tarihçe/lisans metinleri, test fixture'ları, kod yorumları.

## 8. Düşmanca turlar

* **Tur 1 — işlevsel doğruluk** ("UI'de var görünüp gerçekte çalışmayan ne var?"):
  8 yüzey envanteri + komut yüzeyi çapraz denetimi. 0 hayalet çağrı, 24
  ulaşılamayan komut, kopuk state bulguları.
* **Tur 2 — veri bütünlüğü / PDF / dosya sistemi**: marka, filigran kodlaması,
  önizleme↔çıktı ayrışması, ZIP sonrası silinen dosyalar, no-clobber.
* **Tur 3 — state / migration / gizlilik / eşzamanlılık**: ayar ezilmesi,
  migration kayıpları, odak tuzağı, paylaşılan state.
* **Çürütme turu**: her ham bulgu bağımsız bir şüpheciye verildi; 28 bulgu
  **elendi** (yanlış alarm ya da bu turda zaten düzeltilmiş). Elenenler arasında
  benim düzelttiklerim de var — şüpheci kodu yeniden okuyup "artık gerçek değil"
  dedi, bu da düzeltmelerin bağımsız doğrulaması oldu.

## 9. Testler

```text
cargo test --workspace --locked      51 takım · 736 test · 0 başarısız · çıkış 0  ✅
frontend vitest                      27 dosya · 254 test                          ✅
frontend qa/ux (node --test)         49 test                                       ✅
tsc --noEmit (iki proje)             temiz                                         ✅
                                     ─────────────────────────────────────────────
toplam                               1039 test, tümü yeşil
```

Bu turda eklenen testler: `product_acceptance.rs` (9 — production komut
yüzeyinden uçtan uca), `brand_identity.rs` (5 — marka kimliği, çizim baytlarına
bakar), `legacy.rs` (2 — migration veri kaybı), `lib.rs` (1 — ayar birleştirme).

## 10. Fresh clone

Final HEAD'den (`6dc5b75`) `--no-hardlinks` klon alındı ve **sıfırdan kurulup
derlendi ve test edildi**; klon sonunda silindi.

```text
git clone                       ✅  21 MB, kendi kendine yeterli
npm ci                          ✅  çıkış 0
npm run build                   ✅  çıkış 0 — dist/ üretildi
npm test (qa/ux + vitest)       ✅  49 + 254 = 303 test
cargo test --workspace --locked ✅  çıkış 0 — 51 takım · 736 test · 0 başarısız
repo dışına uzanan path bağımlılığı: YOK
```

**Derleme sırası zorunludur ve belgelenmiştir.** İlk denemede `cargo test`
frontend derlemesinden ÖNCE çalıştırıldı ve beklenen hatayı verdi:

```text
error: proc macro panicked
  --> apps/belge-shell/src-tauri/src/lib.rs:383  .run(tauri::generate_context!())
  = help: The `frontendDist` configuration is set to `"../dist"` but this path doesn't exist
```

Bu bir ürün kusuru DEĞİLDİR: `dist/` git'te tutulmaz (doğru), Tauri onu
`generate_context!` sırasında binary'ye gömer, ve README bu sırayı — hatta bu
tam hata mesajını — açıkça belgeliyor (README.md "Kurulum ve derleme").
Depo ayrıca kendi `scripts/check-fresh-clone.sh` kanıtını taşıyor. Hatayı
benim ilk betiğim belgelenen sırayı ihlal ettiği için aldı; doğru sırayla
klon tamamen yeşil.

## 11. Kalan işler

### Kalan P0
**Yok.** Kaynak mutasyonu, bozuk çıktı, çökme, gizlilik sızıntısı ya da
"başarılı denip kullanılamayan dosya" üreten bir yol bulunamadı. Kaynak
değişmezliği her mutasyon testinde SHA-256 ile ölçüldü.

### Kalan P1 (14 — bu turda kapatılmadı)
1. **24 komut arayüzden ulaşılamıyor.** En ağırı: Karşılaştır'ın rapor/dışa
   aktarma motoru (4 biçim, 2 kayıtlı komut, testli) hiçbir düğmeye bağlı
   değil. Düzenle'nin 17 komutu (birleştir/böl/boş sayfa tespiti/proje
   kaydet-yükle/UYAP) ve Denetle'nin 6 sözlük-profil-kural komutu da öyle.
2. Karşılaştır: fark navigasyonu belgeyi **kaydırmıyor**, yalnız seçimi değiştiriyor.
3. Karşılaştır: sürüm değiştirmede hangi belgenin hangi tarafta olduğu görünmüyor.
4. Karşılaştır: büyük belge için sınır/timeout/worker yok; hizalama O(n×m) ve
   render sırasında senkron çalışıyor, ErrorBoundary yok.
5. Denetle: motor aynı aralık için çelişen iki düzeltme üretebiliyor →
   "Tümünü Seç + Kopyaya Uygula" tümden reddedilebiliyor.
6. Denetle: uygulama hatası tüm çalışma alanını (bulgular + seçim) yok ediyor.
7. Denetle: kaydetme diyaloğu dosya adı istiyor ama yazılan ad atılıyor.
8. Denetle: sözlük/profil/kural için hiçbir arayüz yok; üç lint kuralı yeni
   kullanıcıda asla tetiklenemiyor.
9. Denetle: aynı yazım hatası iki ayrı bulgu olarak sayaçları şişiriyor.
10. Kabuk: kip geçişinde "yalnız ilk belge taşınır" kararı hesaplanıp atılıyor;
    motor kabuğun bıraktığı belgeyi de alıyor.
11. Ayarlar: `rendererPath` ölü alan — LibreOffice seçimi hiç kalıcı değil ve
    migration'ın "yeniden seçin" mesajının işaret ettiği kontrol UI'de yok.
12. Ayarlar: "Çıktı klasörü" genel ayar gibi sunuluyor ama yalnız Dönüştür
    kullanıyor; Denetle/Karşılaştır/Düzenle yok sayıyor.
13. Dönüştür: kip sözleşmesi tek belgelik (`needs:1`) ama kabuk tüm diziyi
    geçiriyor; bar bir belge gösterirken iki belge dönüştürülüyor.
14. Migration: önceki birleşik "Belge" kurulumundan yükseltmede İkinciGöz
    sözlüğü ve profilleri taşınmıyor.

### Kalan P2 (~38)
Yutulan hata yolları (düğmeye basılır, hiçbir şey olmaz), sahte başarı
gösterimleri (tamamen başarısız dönüşümde "Dönüştürme tamamlandı"; LOSS
uyarısında yeşil ✓), erişilebilirlik boşlukları (sekme rolleri, basılı/seçili
durum, panel adları), koyu temanın webview'e bildirilmemesi, "Listeyi Temizle"
kipten bağımsız silmesi, silinmiş son belgenin listeden düşmemesi, Karşılaştır
fark sayacının filtreyle tutarsızlığı, IPC'de belge baytlarının JSON sayı
dizisi olarak taşınması. Tam liste denetim çıktısında kayıtlı.

## 12. Commitler

```text
5a399ea  Marka: türetilen PDF'lerin sağ alt işareti artık canonical GölgeDosya
f6e08a4  Düzenle: Türkçe filigran desteği (GİZLİ, ÖRNEKTİR) + ürün kabul testleri
fe5c2e7  Ayarlar: tercih kaydı artık birleştiriyor, diğer modüllerin yazdığını ezmiyor
b1a2c92  Marka: kullanıcıya görünen eski ürün adlarını canonical GölgeDosya'ya taşı
07a61ae  Düzenle: filigran metni ile görsel biçimini ayır; araç değişiminde kaybı söyle
8f63767  Dönüştür: toplu iş sonucunda gerçekte üretilen arşivi göster
b538b4f  Önizleme/çıktı dönüş eşitliği ve Tercihler'de odak sıçraması
be371c8  Düzenle: görsel kaynaklarda PDF önizlemesi denenmesin
ef30d39  Migration: bozuk eski ayar dosyası sözlük ve profil devrini iptal etmesin
```

push / tag / release: **YAPILMADI** (talimat gereği ayrıca istenmedikçe yok).

## 13. Durum

```text
FULL AUDIT PARTIAL
```

**Neden `GÖLGEDOSYA v0.2.0 FULL PRODUCT ACCEPTED` değil.** Kabul kapısı
P1 = 0 şartını koyuyor; **14 P1 açık**. En ağırı, 24 kayıtlı komutun arayüzden
ulaşılamaması — özellikle Karşılaştır'ın tamamlanmış ve testli rapor/dışa
aktarma motorunun hiçbir düğmeye bağlı olmaması. Bunları bu turda kapatmak
yeni arayüz tasarlamayı gerektirirdi; bu tur ise açıkça "yeni UI tasarlama"
diyor. Dolayısıyla dürüst statü PARTIAL.

**Kapanan kapılar** (§47 stop-condition'ları tek tek):

```text
kaynak belge değişiyor mu               HAYIR  — her mutasyon testinde SHA-256 ölçüldü
çıktı bozuk mu                           HAYIR  — her çıktı yeniden açılıp doğrulandı
önizleme/çıktı ayrışması                 KAPANDI (U9)
kaydetme güvenilir mi                    EVET   — no-clobber + atomik + yeniden doğrulama
dönüşümde sahte başarı                   KAPANDI (U15 sahte vaat; U8 sahte dosya listesi)
migration kullanıcı ayarını eziyor mu    KAPANDI (U4, U5)
ciddi state kaybı                        KAPANDI (U7 artık söyleniyor, U6 ayrıldı)
kullanıcıya görünen eski ürün adı        KAPANDI — 13 noktanın 13'ü temiz
GölgeDosya filigranı doğru mu            EVET   — görsel olarak doğrulandı (U1, U2)
açıklanamayan çökme/panic                YOK    — 736 test, 0 başarısız
taze klon                                YEŞİL  — kur+derle+test tamamı klonda koştu
görünen bir kontrol çalışmıyor           KISMEN — 24 komut ulaşılamıyor (kalan P1 #1)
```

**§46 filigran stop-condition'ı tamamen kapalı:** hiçbir çıktıda `DüzenEk`
görünmüyor, eski logo yok, tekrarlı işlemde marka birikmiyor, konum doğru,
döndürülmüş sayfada ters dönmüyor, dosyayı büyütmüyor (küçültüyor), ve
kullanıcıya görünen hiçbir çıktıda `DüzenEk` dizesi kalmadı.

Bu tur bir **release turu değildir**: push, tag ve release yapılmamıştır.
