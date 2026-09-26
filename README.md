# GölgeDosya

**GölgeDosya**, belge üzerinde günlük çalışmayı tek bir masaüstü uygulamasında toplayan, **yerel-first** bir belge çalışma ortamıdır.

PDF düzenleme ve sıkıştırma, belge dönüştürme, sürüm karşılaştırma, belge denetimi ve dilekçe eklerini hazırlama işlemleri tek uygulama içinde yürütülür. Belge içeriği işlenmek üzere bir bulut servisine gönderilmez; temel işlemler cihaz üzerinde gerçekleştirilir.

> **Güncel sürüm:** 0.3.1 Beta  
> Windows x64 ve Apple Silicon macOS paketleri için [Releases](https://github.com/raciyuksekbas-hub/GolgeDosya/releases) sayfasına bakın.

## Araçlar

| Araç | Ne yapar? |
|---|---|
| **Düzenle** | PDF sayfalarını görüntüler ve düzenler; döndürme, silme, sıralama, birleştirme, sıkıştırma ve ilgili PDF işlemlerini tek çalışma alanında toplar. |
| **Ekler** | Belgeleri **Ek-1, Ek-2…** mantığıyla gruplandırır, dilekçe eklerini düzenler ve çıktı paketini hazırlar. |
| **Dönüştür** | Hukuk pratiğinde kullanılan belge biçimleri arasında yerel dönüştürme işlemlerini yürütür; DOCX/UDF iş akışını destekler. |
| **Karşılaştır** | İki belge sürümünü karşılaştırır; eklenen, silinen ve değişiklik içeren bölümleri gösterir. Eş zamanlı kaydırma ve değişiklikleri kopyalama araçları içerir. |
| **Denetle** | Belgedeki yapısal ve yazımsal tutarlılık sorunlarını; boşluk, noktalama, tekrar, numaralandırma ve terim tutarlılığı gibi başlıklarda denetler. |

## Temel yaklaşım

GölgeDosya üç ilkeye göre geliştirilir:

- **Yerel çalışma:** Belge içeriği varsayılan olarak cihazdan çıkmaz.
- **Açıklanabilir işlem:** Belge üzerinde yapılan işlem kullanıcı tarafından görülebilir ve denetlenebilir olmalıdır.
- **Masaüstü disiplini:** Arayüz; araç kalabalığından, gereksiz animasyondan ve web sitesi hissinden kaçınır. Klavye kullanımı ve erişilebilirlik birincil gereksinimdir.

Uygulama belge içeriği için LLM/üretken yapay zekâ servisi kullanmaz ve belge içeriğine ilişkin telemetri toplamaz.

## Platformlar

### Windows

- Windows 10/11 x64
- Kurulum paketi: `GolgeDosya_0.3.1_x64-setup.exe`
- Taşınabilir sürüm: `GolgeDosya_0.3.1_x64.exe`
- Yerel PDF küçük resim ve sayfa önizlemesi Windows'un kendi PDF altyapısı üzerinden desteklenir.

**İmza notu:** Mevcut Windows paketinde Authenticode imzası bulunmamaktadır. Bu nedenle SmartScreen ilk açılışta uyarı gösterebilir.

### macOS

- Apple Silicon (arm64)
- Disk görüntüsü: `GolgeDosya_0.3.1_aarch64.dmg`
- Uygulama Developer ID ile imzalıdır ve Hardened Runtime kullanır.

**Notarization notu:** Mevcut macOS paketi notarize edilmemiştir. Bu nedenle başka bir Mac'te normal ilk açılış sırasında Gatekeeper uyarısı veya engeli oluşabilir.

## İndirme ve bütünlük denetimi

Yayımlanmış paketler ve `SHA256SUMS` dosyası [Releases](https://github.com/raciyuksekbas-hub/GolgeDosya/releases) bölümündedir.

İndirdiğiniz dosyaları doğrulamak için:

```bash
shasum -a 256 -c SHA256SUMS
```

Windows paketleri GitHub-hosted gerçek Windows runner üzerinde derlenip kurulum, açılış ve kaldırma smoke testlerinden geçirilir. macOS paketi aynı kaynak commit'inden arm64 olarak üretilir.

## Kaynaktan derleme

### Gereksinimler

- Rust toolchain
- Node.js / npm
- Tauri 2'nin platform gereksinimleri

Frontend, Tauri'nin `generate_context!` aşamasından **önce** derlenmelidir:

```bash
cd apps/belge-shell
npm ci
npm run build

cd ../..
cargo build --workspace
cargo test --workspace --locked
```

Temel kalite kapıları:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked --no-fail-fast
bash scripts/release-gate.sh --fast
```

## Mimari

GölgeDosya, Tauri 2 tabanlı masaüstü kabuğu ile Rust çekirdeklerinden oluşur.

```text
apps/belge-shell/          Tauri masaüstü uygulaması
crates/document-core/      belge işleme çekirdeği
crates/pdf-core/           PDF işlemleri
crates/ekler-core/         Ekler alan mantığı
crates/ikincigoz-core/     Denetim motoru
crates/process-bridge/     dış süreç sınırı
tools/preflight/           ön kontroller
```

Belge çekirdekleri ile platform kabuğu arasındaki sınırlar otomatik mimari ve komut-paritesi testleriyle korunur.

## Gizlilik

GölgeDosya'nın tasarımında belge içeriği **yerel veri** olarak kabul edilir.

- Belge içeriği işlenmek üzere buluta gönderilmez.
- Belge içeriğine ilişkin telemetri bulunmaz.
- Geçici dosyalar ve çıktı işlemleri yerel dosya sistemi üzerinde yürütülür.
- Son belgeler görünümü kullanıcı tarafından kapatılabilir.

Harici bir uygulama veya işletim sistemi hizmeti kullanılan işlemlerde ilgili platformun kendi davranışları ayrıca geçerlidir.

## Durum

0.3.1, gerçek Windows ve macOS paketleri üretilmiş **beta** sürümdür. Sürüm; saha geri bildirimleri, Windows-native testler, PDF render testleri ve paket smoke testleri üzerinden geliştirilmektedir.

Bilinen paketleme kısıtları:

- Windows paketi henüz Authenticode imzalı değildir.
- macOS paketi Developer ID ile imzalıdır ancak henüz notarize edilmemiştir.

## Lisans

**Proprietary / All Rights Reserved**  
© 2026 Raci Çetin Yüksekbaş.

Kaynak kodun bu depoda erişilebilir olması; kopyalama, değiştirme, yeniden dağıtma veya türev çalışma oluşturma izni vermez. Ayrıntılar için [LICENSE](LICENSE) dosyasına bakın.

Üçüncü taraf açık kaynak bileşenleri kendi lisanslarına tabidir. Canonical kayıt [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) dosyasındadır.
