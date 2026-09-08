# Üçüncü Taraf Bildirimleri

Yüksekbaş Belge, aşağıdaki üçüncü taraf açık kaynak bileşenleri kullanır. Bu
bileşenlerin telif hakları ilgili hak sahiplerine aittir ve **lisans
bildirimleri kaldırılmamıştır**. Tam lisans metinleri her paketin kendi
dağıtımında ve yerel paket önbelleklerinde (`~/.cargo/registry`,
`node_modules`) bulunur.

Bu dosya, dört bağımsız uygulamanın ayrı kayıtlarının (Tavzih
`THIRD_PARTY_NOTICES.md`, İkinciGöz `THIRD_PARTY_LICENSES.md/.json`, Değişikİş
`THIRD_PARTY_NOTICES.md`) yerine geçen **tek canonical kayıttır** ve gerçek
bağımlılık denetiminden üretilmiştir.

Denetim tarihi: 2026-09-08 · `scripts/release-gate.sh` her sürümde yeniden koşar.

---

## Birinci taraf — üçüncü taraf kod içermez

Aşağıdaki bileşenler bu ürün için özgün olarak geliştirilmiştir ve **gömülü
üçüncü taraf kaynak kod içermezler**:

`document-core` · `pdf-core` · `ekler-core` · `ikincigoz-core` ·
`process-bridge` · `belge-shell` · `preflight`

Kaynak taraması: telif başlığı, SPDX bildirimi, "adapted/derived/ported from"
işareti veya vendored dosya bulunmadı. `document-core/src/udf/mod.rs` biçim
bilgisinin kara kutu gözleminden türetildiğini ve başka bir uygulamadan
türetilmediğini açıkça beyan eder.

Marka varlıkları (`brand-logo.svg`, `brand-logo.ops`, uygulama ikonları),
PowerShell yardımcıları ve test korpusu da birinci taraftır. **Uygulama hiçbir
font dosyası paketlemez**; CSS'te geçen font adları yalnız sistem yazı tipi
yedek listesidir, dağıtılan bir varlık değildir.

---

## Rust bağımlılıkları

Bağımlılık grafiğinde **520 üçüncü taraf crate** vardır. Lisansı bilinmeyen
veya beyansız paket **yoktur**. GPL veya AGPL lisanslı paket **yoktur**.

### Doğrudan bağımlılıklar

| Paket | Lisans |
|---|---|
| tauri, tauri-build, tauri-plugin-dialog, tauri-plugin-opener | MIT OR Apache-2.0 |
| serde, serde_json | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| image | MIT OR Apache-2.0 |
| sha2 | MIT OR Apache-2.0 |
| base64 | MIT OR Apache-2.0 |
| rayon | MIT OR Apache-2.0 |
| tempfile | MIT OR Apache-2.0 |
| libc | MIT OR Apache-2.0 |
| url | MIT OR Apache-2.0 |
| flate2 | MIT OR Apache-2.0 |
| once_cell | MIT OR Apache-2.0 |
| regex | MIT OR Apache-2.0 |
| lopdf | MIT |
| zip | MIT |
| quick-xml | MIT |
| tiff | MIT |

### Lisans dağılımı (520 crate)

Tamamı izin verici veya izin verici seçenek sunan çoklu lisanslardır:
MIT · Apache-2.0 · BSD-2-Clause · BSD-3-Clause · ISC · Zlib · 0BSD ·
Unlicense · CC0-1.0 · MIT-0 · Unicode-3.0 · Apache-2.0 WITH LLVM-exception
ve bunların ikili/üçlü birleşimleri.

### MPL-2.0 — açıkça bildirilir

Beş geçişli crate MPL-2.0 lisanslıdır ve Tauri'nin webview yığını üzerinden,
**değiştirilmeden** kullanılır:

    cssparser 0.36.0 · cssparser-macros 0.6.1 · dtoa-short 0.3.5
    option-ext 0.2.0 · selectors 0.36.1

MPL-2.0 dosya düzeyinde zayıf copyleft'tir. Bu crate'lerin kaynak dosyaları
değiştirilmediği ve ürünün kendi kaynak kodu bu dosyaların içine
karıştırılmadığı için, ürünün mülkiyet lisansı altında dağıtılmasına engel
değildir. Bu crate'lerden herhangi biri **değiştirilecek olursa**, MPL-2.0'ın
değiştirilen dosyaları yayımlama yükümlülüğü doğar.

---

## npm bağımlılıkları

`node_modules` ağacında toplam 141 paket bulunur; bunların **35'i uygulamayla
birlikte dağıtılır**, kalanı yalnız derleme ve test zamanında kullanılır.

### Dağıtılan paketler (35)

| Paket | Sürüm | Lisans |
|---|---|---|
| react, react-dom | 18.3.1 | MIT |
| scheduler | 0.23.2 | MIT |
| @tauri-apps/api | 2.11.1 | Apache-2.0 OR MIT |
| @tauri-apps/plugin-dialog | 2.7.3 | MIT OR Apache-2.0 |
| pdfjs-dist | 5.4.149 | Apache-2.0 |
| mammoth | 1.12.2 | BSD-2-Clause |
| lop, option, dingbat-to-unicode | — | BSD-2-Clause |
| sprintf-js | 1.0.3 | BSD-3-Clause |
| fflate | 0.8.3 | MIT |
| @xmldom/xmldom | 0.8.15 | MIT |
| xmlbuilder | 10.1.1 | MIT |
| underscore | 1.13.8 | MIT |
| bluebird | 3.4.7 | MIT |
| jszip | 3.10.1 | MIT OR GPL-3.0-or-later — **MIT seçilmiştir** |
| pako | 1.0.11 | MIT AND Zlib |
| duck | 0.1.12 | BSD (paket "BSD" olarak beyan eder; bkz. aşağıdaki not) |
| inherits | 2.0.4 | ISC |
| argparse, base64-js, core-util-is, immediate, isarray, js-tokens, lie, loose-envify, path-is-absolute, process-nextick-args, readable-stream, safe-buffer, setimmediate, string_decoder, util-deprecate | — | MIT |

**jszip** çoklu lisanslıdır (`MIT OR GPL-3.0-or-later`). Bu üründe **MIT
seçeneği seçilmiştir**; GPL koşulları uygulanmaz. Paket `mammoth` üzerinden
gelir.

**duck** paketi lisansını yalnız `"BSD"` olarak beyan eder; SPDX tanımlayıcısı
belirsizdir. Paketin kendi dağıtımındaki lisans metni BSD ailesindendir ve izin
vericidir. `mammoth` üzerinden gelir.

### Yalnız derleme/test zamanı

vite, typescript, vitest, @vitejs/plugin-react, @tauri-apps/cli, postcss,
@types/* ve bunların geçişli bağımlılıkları uygulamayla **dağıtılmaz**.

Bu grupta `caniuse-lite` paketi **CC-BY-4.0** lisanslıdır. Yalnız derleme
zamanında tarayıcı hedefi verisi olarak kullanılır ve dağıtılan ürünün parçası
değildir; bu nedenle CC-BY atıf yükümlülüğü doğurmaz. Burada yine de kayda
geçirilmiştir.

---

## Denetim kuralı

`scripts/release-gate.sh`, GPL/AGPL lisanslı ya da lisansı bilinmeyen bir paket
bulunduğunda **derlemeyi düşürür** (fail-closed). Çoklu lisanslarda izin verici
bir seçenek bulunması yeterlidir; bu kural hem Rust hem npm tarafında aynıdır.
