# Planlanan feature capability'leri — HENÜZ ETKİN DEĞİL

Bu dizin Tauri tarafından **okunmaz**. Buradaki dosyalar, her modül taşınırken
`capabilities/` içine taşınacak izin kümesinin önceden gerekçelendirilmiş taslağıdır.

Amaç (talep §12): birleşik uygulama, dört modülün izinlerinin **birleşimine
otomatik olarak sahip olmasın**. Özellikle DüzenEk'in geniş `opener:allow-open-path: "**"`
ihtiyacı, İkinciGöz'ün "yalnız dosya seçici, başka hiçbir şey" duruşunu bozmamalı.

Kaynak: dört uygulamanın bugünkü `capabilities/default.json` dosyaları (2026-09-06).

| Modül | Dosya erişimi | Opener | Ağ | Harici process |
|---|---|---|---|---|
| İkinciGöz | yalnız seçilen dosya (`dialog:allow-open/save/message`) | yok | **yok** | yok |
| Tavzih | yalnız seçilen dosya | `reveal-item-in-dir`, `open-path`, `open-url` **sadece `mailto:*`** | yalnız updater | yok |
| Değişikİş | webview'e sürüklenen/seçilen dosya | yalnız release URL'i | GitHub Releases API (güncelleme denetimi) | `textutil` (macOS) / Word COM (Windows) |
| DüzenEk | seçilen dosya + seçilen çıktı klasörü | `reveal-item-in-dir`, `open-path: "**"` | yok | `soffice`, `sandbox-exec`, `sips` |

**Default-deny kuralı:** bir feature kapatıldığında (cargo feature off) capability
dosyası da `capabilities/` dışına alınır. İzin, kodla birlikte gelir ve kodla birlikte gider.
