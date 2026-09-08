# Mimari kararlar

Final consolidation'a girerken açık olan yapısal kararlar. Her biri ölçümle
gerekçelendirilmiştir; "plan öyle diyordu" bir gerekçe değildir.

Ölçüm tarihi: 2026-09-07 · `ekler-core` @ `duzenek-premerge-2026-09-07`

---

## 1. `pdf-core` ayrılsın mı? — **Öneri: Seçenek A (ayrılsın)**

### Önceki gerekçenin düzeltilmesi

Phase 6 raporunda bu bağ "çift yönlü bağımlılık" olarak tanımlanmış ve
ayrımın "tip taşıma ve davranışsal refactor" gerektirdiği söylenmişti. Modül
grafiği ölçüldüğünde bunun **fazla karamsar** olduğu görüldü. Gerçek grafik:

```
pdf   → error, model
model → optimizer
optimizer → error
```

Bu bir DAG'dır; modül düzeyinde döngü **yoktur**. Döngü ancak önerilen crate
sınırı çizilince doğar: `pdf` ve `optimizer` `pdf-core`'a, `model` uygulama
katmanına konursa `pdf-core::pdf → app::model → pdf-core::optimizer` olur.
Yani engel bir tasarım döngüsü değil, **iki tipin yanlış katmanda durması**.

### Bağın gerçek büyüklüğü

Tam olarak iki satır:

| Bağ | Yer | İçerik |
|---|---|---|
| `model → optimizer` | `model.rs:203` | `ExportPlan.optimization: OptimizationLevel` |
| `pdf → model` | `pdf/stamp.rs:2` | `use crate::model::{StampConfig, StampPosition}` |

Taşınacak tipler ve büyüklükleri:

| Tip | Tanım | Referans |
|---|---|---|
| `StampConfig` | 6 alanlı struct + `Default` | 12 (3 dosya) |
| `StampPosition` | 4 varyantlı enum | 12 (2 dosya) |
| `OptimizationLevel` | 4 varyantlı enum | 18 (5 dosya) |

Üçü de **PDF alanına ait** kavramlardır. Bir damga konumu ya da bir PDF
optimizasyon düzeyi, dava dosyası alanının (`model`) kavramı değildir; bugün
orada olmaları tarihsel bir kaza. Taşıma davranışsal değil, derleyicinin
tamamını doğrulayacağı mekanik bir yer değiştirmedir.

### Öneri

**Seçenek A.** Üç tip `pdf-core`'a taşınır, `model` tek yönlü olarak
`pdf-core`'a bağlanır, döngü doğmadan ayrım tamamlanır. Maliyet ~30 satır tip
tanımı ve 42 referansın güncellenmesidir.

Phase 6'da **yapılmadı**, çünkü Phase 6'nın hedefi migration parity idi ve bu
turda ölçülen davranış kanıtı (`tests/duzenek_parity.rs`) taşıma öncesi
duruma aittir. Final consolidation'da yapılmalı, ardından aynı parity matrisi
yeniden koşulmalıdır.

---

## 2. `office-bridge` ayrılsın mı? — **Öneri: Seçenek A, fakat farklı sınırla**

### Ölçüm

Sorunun kendisi yanlış çerçevelenmişti. Dış süreç başlatan tek modül `office`
değil, **iki** modül:

| Modül | Başlatılan süreç | Platform |
|---|---|---|
| `office.rs:201` | `/usr/bin/sandbox-exec` | macOS |
| `office.rs` | seçilen `soffice` çalıştırıcısı | tümü |
| `image/mod.rs` | `/usr/bin/sips` | macOS |
| `image/mod.rs` | `powershell.exe` | Windows |

`office` yalnız `pdf`'e bağlıdır (tek yön, döngü yok), yani ayrılması teknik
olarak kolaydır. Ama `office`'i ayırmak **dış süreç yüzeyini kapatmaz**:
`image` de süreç başlatır ve `toolbox`, `scanner`, `pipeline` üçü birden
`image`'a bağlıdır.

### Öneri

Ayrılacak sınır "LibreOffice köprüsü" değil, **dış süreç sınırı** olmalı:
`office` + `image`'ın dönüştürücü yolu tek bir kapıdan geçmeli. Böylece
"hiçbir dış süreç başlatmayan bir derleme" iddiası ölçülebilir hâle gelir —
bugün ölçülemez, çünkü yüzey iki modüle dağılmıştır.

Bu bir güvenlik mimarisi kararıdır, estetik değil. Yapılana kadar geçerli
olan durum kayıt altındadır: `check-architecture.sh` LibreOffice kodunun
`document-core`'a sızmadığını her sürüm kapısında doğrular; bu gerekli ama
yeterli değildir.

### Ayrıca: `sandbox-exec` kullanımdan kalkmış bir araçtır

Apple `sandbox-exec`'i deprecate etti. Bugün çalıştığı ölçüldü (preflight
`sandbox_exec` kontrolü, ağ reddi etkin). Yerine geçecek mimari kararı bu
belgeye bağlıdır ve migration sırasında **icat edilmedi**.

---

## 3. `ekler-core` ve `ikincigoz-core` nerede yaşamalı? — **Öneri: birleşik depo**

Bugün birleşik uygulamanın derlenmesi iki dış deponun diskte bulunmasına
bağlıdır:

| Tüketici | Bağımlılık | Yol |
|---|---|---|
| `belge-shell` | `ekler-core` | `~/Projects/DuzenEk/crates/ekler-core` |
| `belge-shell` | `ikincigoz-core` | `~/İkinciGöz/crates/ikincigoz-core` |
| `tools/preflight` | `ekler-core` | `~/Projects/DuzenEk/crates/ekler-core` |

Bu, migration boyunca **doğru** tercihti: tek kaynak/iki tüketici modeli
parity'yi tanım gereği garantiliyordu. Final mimari için doğru değildir:
temiz bir kopya klonlanıp derlenemez.

`scripts/check-external-dependencies.sh` bu sayıyı mandal olarak tutar —
artamaz, azalmalıdır. Hedef sıfırdır ve final consolidation'ın çıkış
kapısıdır: taze klon → kur → derle → test, dört bağımsız depo olmadan.

Bağımsız DüzenEk dondurulmuş baseline olarak kalır.

---

## 4. Bilinen ve süreli riskler

### Sürüm hattı bu makinede uçtan uca koşamıyor

`scripts/release-gate.sh` (tam hâli, `--fast` değil) **1 kapıda başarısız**:
"macOS paketleme zinciri". Sebep koddan bağımsızdır:

```
Kullanılabilir notarytool anahtarlık profili yok.
  xcrun notarytool store-credentials belge --apple-id <id> --team-id <team>
```

İki yarıyı ayırmak gerekir:

| Yarı | Durum |
|---|---|
| İmzalama | **çalışıyor** — anahtarlıkta Developer ID Application sertifikası var, hardened runtime doğrulandı (`flags=0x10000(runtime)`) |
| Noterleme | **koşamıyor** — saklı notarytool profili yok |

Bu, migration'ın açtığı bir kusur değildir; ama `--fast` ile koşulduğunda
paketleme atlandığı için **daha önce görünmüyordu**. Tam kapı bu makinede
bugüne kadar uçtan uca geçmemiştir ve dağıtım öncesi kapatılması gerekir.
Profil saklamak Apple kimlik bilgisi ister; betikler parolayı hiç görmez.

Kapı bilerek "atlandı" durumuna çevrilmedi: fail-closed bir kapıda gerçek bir
boşluğu atlamaya çevirmek, boşluğu gizlemek olur.

### `sandbox-exec` kullanımdan kalkmış
Bkz. §2. Bugün çalışıyor, ölçüldü; yerine geçecek mimari ayrı iştir.

---

# Final consolidation kararları — 2026-09-08

## 5. İkinciGöz parser'ı `document-core`'a taşınsın mı? — **HAYIR, kalıcı karar**

### Ölçüm

İkinciGöz'ün lint motoru metni **serileştirilmiş kabın içindeki adresle** bulur:

| Kap | Adres | Nerede üretiliyor |
|---|---|---|
| DOCX | `docx:t:{ordinal}` — akış hâlinde XML ayrıştırmada karşılaşılan **N.** `<w:t>` düğümü | `parser/docx.rs:460` |
| UDF | `udf:off:{offset}:{utf16_len}` — ham içerik blob'una offset | `parser/udf.rs:309,390` |

`writeback.rs` bu adresleri kullanarak **hiçbir şeyi yeniden serileştirmez**: yalnız
kabul edilen değişikliğin dokunduğu metin düğümlerini yamalar, kabın diğer her
baytını olduğu gibi kopyalar. Stil, numaralandırma, üstbilgi, görsel ve
metadata tam olarak oldukları gibi kalır.

`document-core` ise bir **dönüştürücüdür**: okur, modele alır, yeniden yazar.
Modelinde kap içi adres yoktur ve olmasına da ihtiyacı yoktur.

İkisinin çıkardığı şey de aynı değil. `image/table/section/numbering/header/footer`
makinesine değen satır sayısı:

| | Değinme |
|---|---|
| `ikincigoz-core/parser/docx.rs` | **2** |
| `document-core/docx/reader.rs` | **74** |

İkinciGöz düz metin + kaba biçim + adres çıkarır; `document-core` tam sadakatli
tur atmak için tablo, görsel, bölüm, üstbilgi/altbilgi ve numaralandırma çıkarır.
Ortaklık ZIP açmak ve XML akıtmaktan ibarettir; modeller ve sözleşmeler ayrıdır.

### Neden birleştirme güvenli değil

Birleştirmek için `document-core`'un okuyucularına, İkinciGöz'ünkiyle **birebir aynı
sırada** `<w:t>` ordinal'i ve **birebir aynı** UTF-16 offset'i üretmesi eklenmeliydi.
Sayımın herhangi bir noktada ayrışması — alan kodları, köprüler, `smartTag`'ler,
XML olayına bölünmüş `<w:t>`'lerin birleştirilmesi, üstbilgi/altbilginin gezilip
gezilmemesi — o noktadan sonraki **bütün adresleri sessizce kaydırır**.

Bu hata gürültülü değildir: yanlış metin düğümü yamalanır. Aradaki tek koruma
writeback'in yeniden açıp karşılaştıran doğrulama adımıdır — yani hata, olduktan
sonra yakalanır.

Ayrıca `push_synthetic` (docx.rs:444) bilerek `container_path: None` üretir:
arkasında düzenlenebilir düğüm olmayan metin **adreslenemez** olarak işaretlenir.
Bu ayrım da yeniden türetilmek zorunda kalırdı.

### Karar

**İki parser kalıcı olarak ayrı kalır.** Bu bir borç değil, bir sınır kararıdır:
farklı sözleşmelere hizmet ediyorlar. "Duplikasyon var, kaldıralım" gerekçesiyle
müvekkil belgesinin sessizce yanlış yerinden yamanma riski alınmaz.

Bugünkü koruma: 121 golden fikstür + writeback doğrulama turu, ikisi de birleşik
depoda ve her sürüm kapısında koşuyor.

---

## 6. Değişikİş extraction'ı `document-core`'a taşınsın mı? — **HAYIR, bounded context**

### Ölçüm

Değişikİş'in çıkarıcısı webview içinde TypeScript olarak çalışıyor:

| Girdi | Araç |
|---|---|
| DOCX | `mammoth` (DOCX → HTML) |
| PDF | `pdfjs-dist` |
| UDF | `fflate` + `udf.ts` |

`document-core`'da **PDF okuma yoktur** ve olamaz: mimari değişmez bunu açıkça
yasaklar (`document-core -> pdf` YASAK, `check-architecture.sh` her sürüm
kapısında doğrular). Yani Değişikİş'in üç girdisinden biri oraya taşınamaz.

DOCX yolu taşınabilirdi, ama karşılaştırma motorunun blok sınırları ve
normalizasyonu tam olarak `mammoth`'un ürettiği çıktıya göre kalibre edilmiş
durumda. Girdiyi değiştirmek 1.834 satırlık diff motorunun **ürünü olan çıktıyı**
değiştirir.

### Karar

**Çıkarıcı bounded context olarak kalır.** Kazanç: paketten bir DOCX okuyucu
eksilirdi. Bedel: pdfjs zaten taşınamıyor (yani ikinci bir yol yine kalırdı) ve
bütün diff fikstürlerinin yeniden doğrulanması gerekirdi. Değer üretmiyor.

### Canonical source

TS karşılaştırma motorunun canonical kaynağı **birleşik depodur**. Standalone
Değişikİş dondurulmuş baseline'dır (`degisikis-premerge-2026-09-07`).

Altı motor dosyasından beşi (`normalize.ts`, `structure.ts`, `wordDiff.ts`,
`udf.ts`, `types.ts`) standalone ile **birebir aynı**. `compare.ts`'in tek farkı,
kabuğun `noUnusedParameters` ayarı için iki kullanılmayan parametrenin `_`
önekini almasıdır; davranış aynıdır ve dosyada gerekçesiyle yazılıdır.

---

## 7. `pdf-core` — Seçenek A **uygulandı**

§1'deki öneri gerçekleştirildi. Taşınanlar:

| Kaynak | Hedef |
|---|---|
| `ekler-core/src/error.rs` | `pdf-core/src/error.rs` |
| `ekler-core/src/optimizer.rs` | `pdf-core/src/optimizer.rs` |
| `ekler-core/src/pdf/` (4 dosya) | `pdf-core/src/pdf/` |
| `model.rs` içindeki `StampPosition`, `StampConfig` | `pdf-core/src/pdf/stamp.rs` |
| `assets/brand-logo.ops` | `pdf-core/assets/` |

`OptimizationLevel` zaten `optimizer.rs` içindeydi ve onunla birlikte gitti.

### Genel API değişmedi
`ekler-core` üç modülü yeniden dışa aktarıyor (`pub use pdf_core::{error, optimizer, pdf};`)
ve `model.rs` üç tipi yeniden dışa aktarıyor. Böylece `ekler_core::pdf::…`,
`ekler_core::OptimizationLevel`, `ekler_core::StampConfig` yolları aynen çalışıyor
ve **taşınan 21 komut gövdesinin hiçbiri değişmedi** — komut gövdesi eşitliği
kapısı bunu doğruluyor.

### Derleyicinin ortaya çıkardığı, plandaki iki eksik
1. **`brand-logo.ops`** — `stamp.rs` markayı `include_bytes!` ile gömüyor. Varlık
   damgayı çizen kodla birlikte gitti. `brand-logo.svg` `ekler-core`'da kaldı:
   onu UDF yolu kullanıyor. Her varlık tüketicisini izledi.
2. **`decode_image`** — `optimizer.rs` içinde `pub(crate)` idi ve
   `ekler-core::toolbox` sıkıştırma kalitesini ölçerken çağırıyordu. Artık crate
   sınırını geçtiği için `pub`. Bir PDF görsel akışını çözmek zaten bu crate'in
   işidir; API genişletmesi değil, sınırın doğru tarafı.

### Kabul edilen tek pürüz
`EklerError` bu crate'e taşındı ama `InvalidUdf` ve `InvalidImage` gibi PDF dışı
varyantlar taşıyor. Hata tipini alanlara bölmek her `?` noktasında dönüşüm
gerektirirdi; bugün hiçbir davranış kazancı yok ve bu fazın hedefi sınır, tip
cerrahisi değil. Bilinçli olarak ertelendi.

---

## 8. Lisans beyanı — **sahibinin kararı bekleniyor**

Birleşme, farklı lisans beyan eden dört depoyu tek workspace altında topladı:

| Kaynak | Beyan |
|---|---|
| Tavzih | `MIT` |
| DüzenEk | `MIT` |
| İkinciGöz | `Proprietary` |
| Değişikİş | Rust workspace lisans satırı yok |
| **Birleşik workspace** | **`Proprietary`** |

Sonuç: `license.workspace = true` kullanan her crate — `ekler-core` (DüzenEk'ten)
dâhil — artık **Proprietary** beyan ediyor. `document-core` ise Tavzih'ten
gelirken kendi `license = "MIT"` satırını koruduğu için MIT kalmış durumda.

Yani bugün aynı depoda, aynı ürünün içinde, biri MIT biri Proprietary iki
çekirdek var; ve DüzenEk'in motoru bağımsız depoda MIT iken burada Proprietary.

**Bu bir mühendislik kararı değildir.** Kod sahibinin lisans kararıdır ve bu
turda hiçbir beyan değiştirilmedi. `scripts/check-licensing.py` mevcut durumu
tablo hâlinde tutar ve bir beyan sessizce kayarsa kapıyı kapatır.

Kapatılması gereken iki madde:
1. Dört motorun tek bir lisans altında mı toplanacağı (ve hangisi).
2. Birleşik depoda **LICENSE dosyası yok**; dört bağımsız depoda vardı.
   `THIRD_PARTY_NOTICES` / `THIRD_PARTY_LICENSES` dosyaları da taşınmadı.

Üçüncü taraf bağımlılıklar temiz: 527 paket incelendi, GPL/AGPL yok, lisansı
bilinmeyen paket yok, git kaynaklı bağımlılık yok — hepsi paket kayıtlarından.
