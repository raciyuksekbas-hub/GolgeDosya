# Referans analizi — NöbetçiTakvim → Yüksekbaş Belge

Phase A. Bu belge, ekli NöbetçiTakvim ekran görüntüsünün ölçümü ve o disiplinin
Yüksekbaş Belge'ye **tercümesidir**. Kopya değil: iki ürün aynı aileden görünmeli,
ama biri zamanı, diğeri belgeyi merkezine alır.

## 1. Ölçüm

Ölçümler ekran görüntüsü pikseliyle yapıldı ve menü çubuğu yüksekliği cetvel
alınarak nokta (pt) değerine çevrildi. Kaynak görüntü ölçeklenmiş olduğundan
mutlak değerlerde ±%10 belirsizlik var; **oranlar** ve **karakter** güvenilir.

| Yüzey | Ölçülen | Çıkarım |
|---|---|---|
| Pencere | ~1710 × 895 pt (tam ekran) | — |
| Sol kenar çubuğu | ~230–260 pt, pencerenin %15'i | sabit genişlik; 1280 pencerede de aynı |
| Üst yardımcı bar | ~48–52 pt | tek satır; içinde **sayfa başlığı yok** |
| Sağ panel ("Yaklaşan İşler") | ~320–370 pt, %20 | yüzeye **gömülü değil**: üst/sağ/alt 12–16 pt içeriden, 1 px hairline çerçeve, ~8–10 radius, hafif yükseltilmiş ton |
| Kenar çubuğu satırı | ~32–35 pt yükseklik | ikon 16, etiket 13–14, iç yatay dolgu 8, dış dolgu 12 |
| Kimlik bloğu | trafik ışıklarının altında; işaret 24 + ad 14/600 + slogan 11 soluk | pencere başlığı gizli; ad kenar çubuğunda |
| Grup ayrımı | 1 px hairline + 11 pt BÜYÜK HARF etiket, 0.06em aralık, 20–24 pt üst boşluk | "HAFTALIK RAPOR", "BİLGİ KARTI" |
| Seçili durum | tonal yüzey (+%6–8 beyaz), satırın solunda ince dikey işaret, metin birincil, 500 ağırlık | vurgu rengi **alanı boyamıyor** |
| Alt bölüm | Tema (güneş/ay), Ayarlar, Hakkında, Menüyü Daralt; üstünde hairline | dev çizgiyle koparılmamış |
| Workspace başlığı | 22/600, üstten ~28–32 pt; altında 13 pt ikincil **durum** satırı ("9 Eylül 2026 · Kayıt yok"); sağda ikincil eylem | başlık **içeriğin** parçası, chrome bandı değil |
| Boş durum | sütun yüksekliğinin ~%38–40'ında yatay ortada; 24 pt işaret, 15/600 başlık, ≤44ch iki satır 13 pt ikincil açıklama, kompakt buton | matematiksel merkez değil, **optik üst-orta** |
| Alt bölge listesi | hairline + "YARIN ⟨2⟩" etiketi; satırlar: vurgu renkli saat, 13 pt başlık, 12 pt soluk alt satır, sağda çerçeveli tür rozeti; listenin solunda 2 px vurgu çizgisi | kart değil, liste |
| Sağ panel satırı | ~64–72 pt; sol sütun (kalın gün, soluk "2 gün kaldı", saat) + kısaltılmış başlık + çerçeveli rozet; hairline ayırıcı | orta yoğunluk |
| Butonlar | birincil: kompakt (~28 pt), 8 radius, yüksek kontrast; ikincil: tonal + hairline; rozetler: hairline pill 11 pt | her şey birincil değil |
| Arama alanı | gömük ton, hairline, 8 radius, ~300 pt, "Ara (⌘K)" | |
| Koyu yüzeyler | chrome ≈ #1c1c1e; workspace aynı ton ya da bir adım derin; panel ≈ #202023; hairline ≈ beyaz %8; metin ≈ #e6e6e8 / #9a9aa1 / #6c6c72 | saf siyah yok; ton farkları küçük |
| Tipografi | sistem fontu; ~5 boy (22, 15, 13, 12, 11); 600/500/400 | küçük ölçek, sıkı aralık |
| Gölge / gradient | yok | çizgi ve ton yeterli |

## 2. Yoğunluk ayrımı

NöbetçiTakvim'in ferahlık hissi **eşit boşluktan gelmiyor**; dört yüzeyin dört ayrı
yoğunluğu var ve ilişki bu farktan doğuyor:

| Yüzey | Yoğunluk |
|---|---|
| Kenar çubuğu | kompakt (32 pt satır, 12 pt dolgu) |
| Yardımcı bar | kompakt (48 pt, dikey ortalı, 12–16 pt yatay dolgu) |
| Workspace | ferah (28–32 pt kenar dolgusu, başlık altı 24, bölümler arası 32) |
| Sağ panel | orta (12–14 pt dolgu, 64 pt satır) |

Mevcut Yüksekbaş Belge kabuğunda bu ayrım yoktu: her yer aynı 24–32 pt boşluktu
ve büyük koyu alan "ma" değil, yalnızca boşluktu.

## 3. Tercüme — Yüksekbaş Belge'ye taşınanlar

Aile kimliğini taşıyan, **olduğu gibi** alınanlar:

- Dört yüzeyli mekânsal model: kenar çubuğu · yardımcı bar · workspace · bağlamsal sağ panel.
- Kenar çubuğu disiplini: kimlik bloğu, 32 pt satır, 16 pt ikon, 13 pt etiket,
  hairline + BÜYÜK HARF grup etiketi, tonal seçili durum + ince sol işaret.
- Yardımcı bar: tek satır, sayfa başlığı taşımaz; sol bağlam, sağ birincil eylem.
- Sağ panel: içeriden yerleşmiş, hairline çerçeveli, 8 radius, kendi BÜYÜK HARF başlığı.
- Liste dili: hairline ayırıcı, sağda çerçeveli tür rozeti, soluk zaman/meta, kart yok.
- Tip ölçeği: 20 / 15 / 13 / 12 / 11; 600 / 500 / 400.
- Radius 4 / 6 / 8; gölge yok; gradient yok; hairline düşük kontrast.
- Workspace başlığı içeriğin parçası, üstten nefesli; altında **durum**, açıklama değil.

Kural olarak yazılanlar (eleştiri turunda netleşti):

- Kenar çubuğunda **tek vurgu taşıyıcı** vardır: seçili kipin 2 px işareti.
  Kimlik bloğu yalnız ad + tek renkli (currentColor) çizgi işaret; renkli ikon
  yok (§21), slogan yok.
- Bar ve şerit kontrolleri `IconButton` + `icons.tsx` SVG'dir (erişilebilir ad
  zorunlu). Unicode glif yalnız anlam taşıdığı ve test-kilitli olduğu yerde
  kalır (ciddiyet ●▲○, Status ✕/✓, döndürme düğmesi etiketleri); süs olarak
  asla.
- 11 px BÜYÜK HARF etiketler, meta ve zaman `--text-secondary` ile yazılır;
  `--text-muted` yalnız devre dışı/yer tutucu metin içindir (AA dışında
  kalabilen tek sınıf). Muted 11 px etiket açık temada 2.9:1'e düşüyordu.
- Yükseklikler `min-height` ve metin ölçeğine bağlıdır: `--row-h`,
  `--control-h`, `--toolbar-h` `calc(N px * var(--text-scale))`. %200'de
  satır 64 px olur; kırpılmaz.
- Odak rengi sistem mavisidir (`--focus`), vurgu değil: Apple odak disiplini,
  vurgu dolgulu birincil düğme üzerinde de görünür kalır. §16'daki "focus"
  kalemi için bilinçli sapma.

Bilerek **farklılaşanlar** (aynı aile, farklı ürün):

| Konu | NöbetçiTakvim | Yüksekbaş Belge |
|---|---|---|
| Merkez | zaman (bugün / yarın / yaklaşan) | belge (açık belge / son kullanılanlar / sonuç) |
| Vurgu | kehribar | mürekkep yeşili (mevcut `--accent`) |
| Birincil buton | nötr ters kontrast | vurgu dolgulu, aynı ölçü ve ağırlık (şartname §16–17) |
| Sağ panel | hep açık | yalnız gösterecek gerçek içerik varken; yoksa çizilmez |
| Arama | var (⌘K) | yok — belge uygulamasında arama yüzeyi icat edilmez |
| Alt bölüm | tema anahtarı + Ayarlar + Hakkında + daralt | yalnız Ayarlar (⌘,); tema Ayarlar'da zaten var |
| Rozet renkleri | kehribar/soluk | tek renk, çerçeveli; ciddiyet renk + işaret + metinle |

## 4. Pencere başlığı kararı

NöbetçiTakvim'de native başlık çubuğu **gizli/örtüşük**: trafik ışıkları kenar
çubuğunun üstünde durur, kenar çubuğu pencerenin tepesine kadar çıkar ve tek üst
bar yardımcı bardır. Yüksekbaş Belge şu an `titleBarStyle: "Visible"` kullanıyor;
bu, başlık çubuğunun hemen altında ikinci bir bant doğuruyor — reddedilen görünümün
kök nedeni.

Karar: `titleBarStyle: "Overlay"` + `hiddenTitle: true`. Kenar çubuğunun kimlik
bloğu ve yardımcı bar `data-tauri-drag-region` taşır; pencere hâlâ native
sürüklenir, hizalanır, tam ekrana girer. Uygulama adı, pencere başlığı artık
görünmediği için kenar çubuğuna döner (önceki "adı tekrarlama" kararı bu koşulla
geçersizleşir; bkz. `docs/DESIGN.md`).

Bu bir pencere görünümü ayarıdır; yetenek/gizlilik politikasına, CSP'ye ve
komut anlamlarına dokunmaz. `core:window:allow-start-dragging` izni zaten
vardı ve artık tüketiliyor; budanmamalı.

Karar, referans modelin doğrudan sonucu olduğu için burada verildi; ama sahibi
tersini isterse geri dönüş yolu bellidir: `Visible` + `hiddenTitle: false`,
kimlik bloğu kalkar, bar tek bant olarak kalır, `--titlebar-h: 0`. Tam ekranda
trafik ışıkları çekilir; kabuk `data-fullscreen` ile üst bandı 12 px'e indirir.
Açılışta webview boyanana kadar pencere `backgroundColor` (açık chrome) ~100 ms
görünür; koyu temada kısa bir açık kare. Kabul edildi ve not düşüldü —
tema-başına arka plan için yetenek genişletilmez.

Varsayılan pencere 1180×800'den **1280×800**'e çıkar: ilk açılış, kabul
edilen tam kompozisyonla (panel 300) olmalı. `minHeight` 620 → 600: şartname
900×600 kabul boyutunu istiyor; ulaşılamayan boyut test edilemez.

## 5. Token tercümesi

| Token | Eski | Yeni | Neden |
|---|---|---|---|
| `--sidebar-w` | 204 | 232 | referans oranı; 13 pt etiket + 16 ikon + kimlik bloğu rahat sığar |
| `--toolbar-h` | 44 | 48 | referans barı; 28 pt buton dikeyde nefes alır |
| `--inspector-w` | 288 | 300 | referans oranı; 64 pt satırda üç sütun |
| `--inspector-inset` | — | 12 | panel yüzeye gömülü değil, içeriden yerleşik |
| `--row-h` | — | 32 | kenar çubuğu satırı |
| `--text-title` | 19 | 20 | workspace başlığı |
| `--text-heading` | 15 | 15 | bölüm / boş durum başlığı |
| `--text-body` | 13 | 13 | |
| `--text-meta` | 11.5 | 12 | liste meta |
| `--text-label` | — | 11 | BÜYÜK HARF grup ve panel etiketi |
| `--control-h` | — | 28 | buton / alan yüksekliği |
| Seçili durum | vurgu metni + vurgu yıkaması | tonal yüzey + 2 px vurgu sol işaret + 500 | referans; vurgu alanı boyamaz |
| Sağ panel | tam yükseklik, sol hairline | içeriden yerleşik panel, dört kenar hairline, 8 radius | referans |
| Gölge | sheet'te %14 | aynı | referansta gölge yok; sheet istisna |
| İkincil düğme | yükseltilmiş dolgu + %16 çerçeve | tonal dolgu (`--surface-hover`) + hairline | referans "tonal + hairline"; panel üzerinde de okunur |
| Tür rozeti | BÜYÜK HARF soluk metin | 4 px radius, hairline, ikincil metin, 11 px | 999 px pill 4/6/8 kuralının dışında |
| Panel genişliği | 288 | 300, her genişlikte; 1280 altında varsayılan kapalı, bardaki "Ayrıntılar" ile açılır | 280'e daralma yok; workspace daralmaz |

Yüzey ataması (koyu / açık):

| Bölge | Token | Koyu | Açık |
|---|---|---|---|
| kenar çubuğu | `--surface-sidebar` | #1f1f21 | #e6e5e1 |
| bar (chrome) | `--surface-app` | #1c1c1e | #ebeae7 |
| workspace | `--surface-workspace` | #171719 (chrome'dan derin) | #f3f2ef (chrome'dan açık) |
| sağ panel | `--surface-raised` | #232326 | #fbfbfa |
| belge | `--surface-document` | #ffffff | #ffffff |
| gömük (alan, alıntı) | `--surface-sunken` | #131315 | #e2e1dd |

## 6. Kabul soruları (her ekran için)

- **Apple:** Bu ekran macOS'ta birinci taraf seviyesinde bir profesyonel araç
  içinde yabancı görünür mü?
- **Japon:** Bu ekrandan hangi öğe işlev kaybı olmadan kalkar? Boşluk ilişki mi
  kuruyor, yalnız boş mu?
- **Aile:** NöbetçiTakvim'in yanına konduğunda aynı ekipten çıktığı hissediliyor
  mu — ama skin değiştirilmiş hâli gibi değil?
