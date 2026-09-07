# Yüksekbaş Belge

Dört bağımsız masaüstü uygulamasının — **Tavzih**, **DüzenEk**, **Değişikİş**,
**İkinciGöz** — tek bir yerel belge çalışma ortamında birleştirilmesi.

> **`Yuksekbas-Belge` ve `belge-shell` GEÇİCİ TEKNİK ADLARDIR.**
> Ürün markası kararı verilmedi. Kod, `AppInfo.nameIsProvisional = true` ile
> bunu açıkça bildirir.

## Durum — Phase 1, 2, 3, 4, 5 tamamlandı

| Faz | İş | Durum |
|---|---|---|
| 0 | Snapshot + temiz taban | ✅ |
| 1 | Tavzih ↔ DüzenEk `document-core` tekilleştirmesi | ✅ |
| 2 | Birleşik workspace + minimal kabuk | ✅ |
| 3 | Tavzih migration'ı | ✅ |
| 4 | İkinciGöz migration'ı (mevcut parser korunarak) | ✅ |
| 5 | Değişikİş migration'ı (TS diff motoru korunarak) | ✅ |
| 6 | DüzenEk migration'ı (dondurulmuş temele karşı) | ⏭ **sıradaki** |
| 7 | Provenance üzerinden parser konsolidasyonu araştırması | ⏸ |
| 8 | Ayar migration'ı + gizlilik/capability sıkılaştırma | 🟡 okuma katmanı hazır |
| 9 | Tam sürüm/notarization kapısı | 🟡 hat kuruldu, kimlik bilgisi gerekiyor |
| 10 | Legacy uygulamaların emeklilik değerlendirmesi | ⏸ |

**Dört bağımsız uygulama çalışmaya devam ediyor.** Hiçbiri emekliye ayrılmadı,
hiçbirine bu çalışmada dokunulmadı.

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
cargo test --workspace             # 22 test
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check-architecture.sh
bash scripts/release-gate.sh --fast
BELGE_ADHOC=1 sh scripts/bundle-macos.sh    # imzasız yerel paket
```
