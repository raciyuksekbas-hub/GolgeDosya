# GölgeDosya — marka

Bu dizin markanın **tek kaynağıdır**. `src-tauri/icons/` altındaki her şey
`appicon.svg`'den `build-icons.sh` ile türetilir; o dizin elle düzenlenmez.
Marka değişirse burada değişir ve betik yeniden çalıştırılır.

## İşaret — "Kat"

Tek siluet: bir sayfa. Sağ alt köşesi kaldırılmış ve altındaki katman
görünüyor. Sayfa kullanıcının belgesi; kaldırılan köşenin altındaki pirinç
yüzey, belgeye eşlik eden ikinci katman — ürünün kendisi. Gölge burada
karanlık değil, **alttaki yardımcı katman**.

İki şekil ve tek bir diyagonal: 16 px'te de okunur, tek renge indirgenebilir,
gravüre ve damgaya çevrilebilir. Terazi, tokmak, sütun yok.

### Değerlendirilen diğer iki yön

**İki Kat** — üst üste kaymış iki sayfa; öndeki mürekkep, arkadaki pirinç bir
şerit olarak görünüyor. Adı doğrudan anlatıyor ama her arayüzdeki "kopyala"
glifiyle aynı forma düşüyor; ürün kendi işaretine sahip olamıyor.

**Hiza** — uzunlukları azalan üç satır, ortadaki kaymış, yanında pirinç bir
ayar çentiği. Düzenle/Karşılaştır/Denetle'yi iyi anlatıyor; 16 px'te hamburger
menüye dönüşüyor ve belge fikri kayboluyor.

**Kat** seçildi: adı taşıyor, küçük boyutta ayakta kalıyor, ürünün dört
işlevinin ortak fikrini (belgeye eşlik eden ikinci katman) tek formda söylüyor.

## Renk

| Rol | Ad | Açık tema | Koyu tema |
|---|---|---|---|
| Mürekkep / zemin | Sumi | `#1C1C1E` | `#1C1C1E` |
| Kâğıt | Fuji | `#F5F4F1` | `#F5F4F1` |
| Vurgu | Pirinç | `#8A6A3E` | `#C9A472` |

Pirinç markanın tek kromatik taşıyıcısıdır: kat, seçili durum, birincil eylem,
küçük durum işaretleri. Geniş alan boyamaz. Mavi-SaaS tonu, petrol yeşili ve
sert turkuvaz kullanılmaz. Kırmızı yalnız uyarıdır, marka rengi değildir.

Kontrast: `#8A6A3E` kâğıt üzerinde 5.0:1, beyaz metinle dolgu olarak 5.2:1.
`#C9A472` sumi üzerinde 8.4:1.

## Tipografi

Sistem fontu — macOS'ta SF Pro. Font paketlenmez; ürünün kendi tasarım kararı
budur ve kelime işareti de aynı yüzü kullanır.

- Kelime işareti: 600 ağırlık, `-0.015em` harf aralığı.
- `GölgeDosya` tek kelime, ortada büyük D ile yazılır. `Gölge Dosya`,
  `Gölgedosya` veya `GÖLGEDOSYA` kullanılmaz.
- Türkçe karakterler (ö, ü, ğ, ş, ı, İ) SF Pro'da yerleşiktir; büyük harfe
  çevirmek gerekiyorsa `lang="tr"` zorunludur.

**Not:** Bu dizindeki `*.png` kelime işaretleri `sips` ile üretilmiştir ve
`sips` sistem fontunu çözmez — PNG'lerde yedek yüz görünür. SVG'ler doğru
yüzü taşır ve uygulamada, tarayıcıda ve tasarım araçlarında doğru render
edilir. Baskı için harfler bir tasarım aracında outline'a çevrilmelidir.

## Dosyalar

| Dosya | Kullanım |
|---|---|
| `mark.svg` / `.png` | Yalnız işaret, açık zemin |
| `mark-dark.svg` / `.png` | Yalnız işaret, koyu zemin |
| `mark-mono.svg` / `.png` | Tek renk (`currentColor`) |
| `horizontal.svg` / `-dark` | Yatay kilit: işaret + kelime işareti |
| `stacked.svg` / `-dark` | Dikey kilit |
| `appicon.svg` | Uygulama ikonu karosu (1024) |
| `splash-dark.svg` / `-light` | Açılış ekranı |
| `build-icons.sh` | `src-tauri/icons/` üretimi (sips + iconutil) |

## Kullanım kuralları

- İşaretin çevresinde en az kendi genişliğinin dörtte biri kadar boşluk kalır.
- İşaret döndürülmez, eğilmez, gölge/parlaklık/gradyan almaz.
- Kat her zaman sağ alttadır; aynalanmaz.
- Kelime işareti işaretten ayrı renklendirilmez.
- 16 px altında yalnız `mark-mono` kullanılır.
- Fotoğraf üzerine yerleştirilmez; düz sumi veya düz kâğıt zemin ister.
