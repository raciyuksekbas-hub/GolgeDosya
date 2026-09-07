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
