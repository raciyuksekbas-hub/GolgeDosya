# GölgeDosya

Dört bağımsız masaüstü uygulamasının — **Tavzih**, **DüzenEk**, **Değişikİş**,
**İkinciGöz** — tek bir yerel belge çalışma ortamında birleştirilmesi.

> **`Yuksekbas-Belge` ve `belge-shell` GEÇİCİ TEKNİK ADLARDIR.**
> Ürün markası kararı verilmedi. Kod, `AppInfo.nameIsProvisional = true` ile
> bunu açıkça bildirir.

## Durum — mimari tamamlandı, GUI bilinçli olarak ertelendi

| Faz | İş | Durum |
|---|---|---|
| 0 | Snapshot + temiz taban | ✅ |
| 1 | Tavzih ↔ DüzenEk `document-core` tekilleştirmesi | ✅ |
| 2 | Birleşik workspace + minimal kabuk | ✅ |
| 3 | Tavzih migration'ı | ✅ |
| 4 | İkinciGöz migration'ı (mevcut parser korunarak) | ✅ |
| 5 | Değişikİş migration'ı (TS diff motoru korunarak) | ✅ |
| 6 | DüzenEk migration'ı (dondurulmuş temele karşı) | ✅ |
| 7 | Final mimari konsolidasyon | ✅ |
| — | **GUI tasarımı ve GUI kabulü** | ⏸ **bilinçli olarak ertelendi** |

**Dört bağımsız uygulama çalışmaya devam ediyor.** Hiçbiri emekliye ayrılmadı;
dördü de dondurulmuş referans olarak `*-premerge-2026-09-07` etiketlerinde
duruyor. **Bu deponun derlenmesi artık onlara bağlı değildir.**

## Kurulum ve derleme

Sıra zorunludur. Tauri, `generate_context!` sırasında `dist/` dizinini binary'ye
gömer; `dist/` git'te tutulmadığı için frontend derlemesi Rust derlemesinden
**önce** gelmelidir. Aksi hâlde taze bir klonda `error: proc macro panicked`
alırsınız.

```bash
cd apps/belge-shell && npm ci && npm run build   # dist/ üretir
cd ../.. && cargo build --workspace
cargo test --workspace
```

Bu sıra `scripts/check-fresh-clone.sh` tarafından her koşuda doğrulanır: depo,
eski dört depo erişilemezken klonlanıp derlenir ve test edilir.

## Lisans

**Proprietary / All Rights Reserved** — © 2026 Raci Çetin Yüksekbaş.
Bkz. `LICENSE`.

Yedi birinci taraf bileşenin tamamı bu ürün için özgün geliştirilmiştir ve
gömülü üçüncü taraf kaynak kod içermez. Üçüncü taraf açık kaynak bağımlılıklar
kendi lisanslarına tabidir; tek canonical kayıt `THIRD_PARTY_NOTICES.md`.

Bağımsız Tavzih ve DüzenEk depolarının yayımlanmış MIT sürümleri bu kararla
değişmez; karar yalnız birleşik ürünün bundan sonraki rejimidir.

## Kapılar

| Betik | Ne kanıtlar |
|---|---|
| `scripts/check-architecture.sh` | katman yönleri, süreç sınırı, capability yüzeyi, legacy yolu izolasyonu |
| `scripts/check-external-dependencies.py` | depo dışı yol bağımlılığı **sıfır** (mandal) |
| `scripts/check-feature-matrix.sh` | altı derleme şeklinin hepsi derlenir, yedincisi reddedilir |
| `scripts/check-command-parity.py` | taşınan 21 DüzenEk komutunun gövdesi bağımsız depoyla aynı |
| `scripts/check-licensing.py` | Proprietary rejimi, LICENSE, üçüncü taraf kaydı ve bağımlılık lisansları — fail-closed |
| `scripts/check-fresh-clone.sh` | taze klon → kur → derle → test, eski depolar olmadan |
| `scripts/release-gate.sh` | hepsi + lisans, ağ bağımsızlığı, paketleme, kullanıcı verisi değişmezliği |

## Yapı

```
Cargo.toml                          workspace
apps/belge-shell/                   Tauri 2 uygulaması
  src/shell/                        rota, yerleşim, navigasyon
  src/shared-ui/                    tema, duyurucu, odak tuzağı, token'lar
  src/features/                     modül yer tutucuları
  src-tauri/src/                    kabuk, ayarlar, eski ayar migration'ı, feature flag
  src-tauri/capabilities/           ETKİN izinler (yalnız kabuk)
  src-tauri/capabilities-planned/   modül izinleri — taşınırken etkinleşir
crates/document-core/               Tavzih çekirdeği — TEK kopya (Phase 1)
crates/                             pdf-core, office-bridge (Phase 6)
scripts/check-architecture.sh       mimari değişmez denetimi
scripts/release-gate.sh             sürüm kapısı
scripts/bundle-macos.sh             imzalama + notarization + karantina provası
docs/MIGRATION.md                   faz kayıtları, açık sorular, rollback
```

## Mimari değişmez

```
pdf-core  →  document-core     SERBEST
document-core  →  pdf-core     YASAK
```

Bu kural bugün zaten geçerli: Tavzih'in çekirdeğinde tek bir PDF referansı yok,
DüzenEk'in çekirdeği onu yalnız beş API noktasından tüketiyor.
`scripts/check-architecture.sh` kuralı kilitler.

## Feature-level rollback

Her modül iki kapı arkasında:

* **derleme zamanı** — `cargo build --features feature_tavzih`
  (hiçbiri `default` değil; mimari denetimi bunu doğrular)
* **çalışma zamanı** — `BELGE_DISABLE_TAVZIH=1`

Bir modül sorun çıkarırsa yalnız o rota kapanır; uygulamanın tamamı geri alınmaz.

## Komutlar

```sh
cd apps/belge-shell && npm install
npm run build                      # tsc + vite
cargo test --workspace             # 51 takım · 736 test
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check-architecture.sh
bash scripts/release-gate.sh --fast
BELGE_ADHOC=1 sh scripts/bundle-macos.sh    # imzasız yerel paket
```
