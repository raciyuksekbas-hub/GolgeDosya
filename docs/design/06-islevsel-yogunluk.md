# İşlevsel yoğunluk — wireframe

İki uç da reddedildi: önce AI slop (kart, hardal CTA, dev boşluk), sonra
çıplak wireframe (başlık + düğme + %80 boşluk). Bu belge üçüncü noktayı
kod yazılmadan önce sabitler.

Tek ölçü (§24): **her ekranda üç sorunun üçü de görünür olmalı.**

1. Kullanıcı burada ne yapıyor?
2. Şu anda hangi belge/bağlam açık?
3. Bir sonraki anlamlı eylem ne?

Biri görünmüyorsa ekran fazla seyrektir. Dördüncüsü eklenirse ekran
kalabalıktır.

Ölçüler 1280×800 pencere içindir.

---

## A. Shell

```
0        220                                            1280
├─────────┼────────────────────────────────────────────────┤
│ sidebar │ titlebar (trafik ışıkları)              40 px  │
│  220    ├────────────────────────────────────────────────┤
│         │ toolbar  belge · durum ············· eylem 46px│
│         ├──────────────────────────────────┬─────────────┤
│         │ workspace                        │ inspector   │
│         │                                  │  300 px     │
│         │                                  │ gerektiğinde│
└─────────┴──────────────────────────────────┴─────────────┘
```

Üç kolon her zaman açık değildir:

| Yüzey | Çizilme koşulu |
|---|---|
| sayfa şeridi | gerçekten sayfa varsa |
| araç paneli | kullanılabilir belge oturumu varsa |
| inspector | gösterilecek gerçek içerik varsa |
| toolbar eylemi | o an yapılabilecek bir iş varsa |

Hiyerarşi çizgiyle değil **yüzey tonu · boşluk · hizalama** ile. Tek yapısal
çizgi kenar çubuğunun sağ kenarı, tek ayırıcı kenar çubuğu footer'ı.

### Palet

| Yüzey | Koyu | Açık |
|---|---|---|
| pencere / toolbar | `#1C1C1E` | `#EFEEEC` |
| kenar çubuğu | `#202022` | `#E9E8E5` |
| workspace | `#18181A` | `#F6F5F3` |
| yükseltilmiş | `#242426` | `#FBFAF9` |
| çizgi | `rgb(255 255 255 / 8%)` | `rgb(0 0 0 / 8%)` |
| metin | `.92` / `.58` | `#1C1C1E` / `#6B6B70` |

**Hardal/pirinç vurgu kalktı.** Seçili satırın 2 px işareti artık nötr
(`--text-primary` %65). Renk yalnız semantikte (hata/uyarı/incele) ve odak
halkasında. Marka işaretinin kendi rengi vardır — o marka varlığıdır, UI
vurgusu değil.

### Tipografi (§19)

```
18 / 600   major context   — ekranın konusu
14 / 500   section         — bölüm başlığı
13         body            — gövde, satır, kontrol
11.5       metadata        — künye, sayı, ikincil
```

All-caps yalnız inspector başlığında. Letter-spacing gösterisi yok.

### Toolbar (§7)

Toolbar boş bant değildir; **işi vardır**: solda bağlam, ortada o bağlamın
ölçüsü, sağda eylem.

```
Düzenle                                              [ PDF Aç ]
ek-3 bilirkişi raporu.pdf   8 sayfa · 2 işaretli     Yeni PDF kaydet
dava-dilekcesi.docx         Word (.docx) → UYAP (.udf)
v1.docx ↔ v2.docx           2 / 18 fark
dava-dilekcesi.docx         5 kesin hata · 2 uyarı · 1 incele
```

Durum metni kipin kendi yüzeyinden gelir (`ToolbarStatus` portalı), sabit
genişlikli rakamlarla, ikincil tonda. Anlamsız ikon doldurulmaz.

---

## B. Home / karşılama

Karşılama ekranın matematiksel merkezinde değil: **680 px'lik çalışma
sütununda, üst bantta, sola hizalı.** Altında ölü alan değil gerçek içerik
durur.

```
│ sidebar │ toolbar: Düzenle                       [ PDF Aç ] │
│         │                                                    │
│         │  PDF üzerinde çalışın                       18/600 │
│         │  Sayfaları düzenlemek, döndürmek, sıralamak         │
│         │  veya dışa aktarmak için bir PDF açın.       13    │
│         │                                                    │
│         │  [ PDF Aç ]   veya belgeyi buraya sürükleyin        │
│         │                                                    │
│         │  Son PDF'ler                                14/500 │
│         │  ▣ ek-3 bilirkişi raporu.pdf                 46 px │
│         │    PDF · dün                                 11.5  │
│         │  ▣ kesif-tutanagi.pdf                              │
│         │    PDF · 3 gün önce                                │
```

- Liste **kipe göre süzülür**: Düzenle PDF'leri, Denetle DOCX/UDF'leri
  gösterir. Bu kipin açamayacağı bir satır çizilmez — tıklanınca reddedilen
  satır bilgi değil tuzaktır.
- Başlık kipin diline uyar: `Son PDF'ler` · `Son Belgeler`.
- Kayıt yoksa liste bloğu hiç çizilmez; yerine **desteklenen türler** tek
  künye satırı olarak durur (§12): `PDF · DOCX · UDF · JPG · PNG`.
- Satır 46 px, iki satır, 16 px ikon, tam yol yok, rozet yok, çizgi yok.
- `Listeyi temizle` üçüncül (§8).

---

## C. Düzenle — gerçek PDF çalışma ortamı

Belge yokken: B'deki karşılama + `Son PDF'ler`.

Belge açıkken:

```
│ sidebar │ toolbar: ek-3 raporu.pdf  8 sayfa · 2 işaretli  [Yeni PDF kaydet]│
│         ├──────┬────────────────────────────────┬────────────┤
│         │ 124  │ [Seç][Sırala][Sil][Döndür]  ⌄  │ Belge      │
│         │ şerit│                                │ Birleştir  │
│         │ □ 1  │                                │ Sıkıştır   │
│         │ ■ 2  │      P D F   S A Y F A         │ Görsele    │
│         │ □ 3  │      (kahraman)                │            │
│         │      │                                │ ▸ Diğer    │
│         │      │ künye (11.5)                   │ seçili araç│
```

Öncelik: **belge > sayfalar > araçlar.** Sayfa araçları sayfaların
yanındaki segment şeridinde; belge çapındaki işler panelde; nadir işler
(`filigran`, `sayfa no`, `kırp`, `görseller → PDF`) katlı `Diğer` altında.

Toolbar durumu: `{n} sayfa` ve seçim varsa `· {k} işaretli`.

---

## D. Dönüştür — tek akış, tek eylem

Kart yığını yok; hizalama ve boşlukla kurulan tek akış.

```
│ sidebar │ toolbar: dilekce.udf   UYAP (.udf) → Word (.docx) │
│         │                                                    │
│         │   Kaynak                    Hedef           11.5   │
│         │   dilekce.udf        →      Word Belgesi    18/600 │
│         │   UYAP UDF                  .docx           13     │
│         │                                                    │
│         │   Çıktı konumu                              11.5   │
│         │   Dönüştürülen Belgeler        Değiştir…           │
│         │                                                    │
│         │   [ Dönüştür ]                                     │
```

İki durak, aralarında ok; iki sütun aynı taban çizgisinde hizalı. Birincil
eylem **akışın içinde** durur — iş orada yapılır (§10). Toolbar o yüzden
yön bilgisini taşır, düğmeyi tekrarlamaz (§22: ekranda tek baskın eylem).

Tamamlanınca aynı yüzey devam eder:

```
   dilekce.udf  →  dilekce.docx            ✓ Tamamlandı

   Tablo hücre kenarlıkları yaklaşık aktarıldı (s. 2)   11.5
   Kaynak belge değiştirilmedi.

   [ Finder'da Göster ]      Yeniden dönüştür
```

---

## E. Karşılaştır — iki belge modeli boş durumda da görünür

Bu kipin boş durumu diğer üçünün metin varyantı değildir: zihinsel model
ekranda durur.

```
│ sidebar │ toolbar: Karşılaştır                               │
│         │                                                    │
│         │  İki belgeyi karşılaştırın                  18/600 │
│         │  İlk belgeyi seçin, ardından karşılaştıracağınız    │
│         │  ikinci belgeyi ekleyin.                           │
│         │                                                    │
│         │  ┌── Belge A ────┐      ┌── Belge B ────┐          │
│         │  │ + İlk belgeyi │  ↔   │ + İkinci      │          │
│         │  │   seçin       │      │   belgeyi seç │          │
│         │  └───────────────┘      └───────────────┘          │
│         │                                                    │
│         │  Son Belgeler …                                    │
```

İlk belge seçilince A dolar (ad + tür), B birincil olur:

```
│         │  ┌── Belge A ────┐      ┌── Belge B ────┐          │
│         │  │ sozlesme-v1   │  ↔   │ + İkinci      │          │
│         │  │ DOCX          │      │   belgeyi seç │          │
```

Yuvalar kart değil: tonal yüzey, 1 px yok, radius 6, 96 px yükseklik.

Belge açıkken (değişmedi): iki pano + ray + fark listesi. **Ray konum,
liste içerik** gösterir; ikisi aynı seçici gibi görünmez. Fark sayacı
toolbar'da (`2 / 18 fark`), panelde tekrar edilmez.

---

## F. Denetle — belge bağlamı hiç ölmez

Belge açık, bulgu var:

```
│ sidebar │ toolbar: dilekce.docx  5 kesin hata · 2 uyarı · 1 incele │
│         ├─────────────────────────────────┬──────────────┤
│         │ ┌── 720 px ──────────────────┐  │ Denetim özeti│
│         │ │ ● Kesin hata  Çift noktalama│  │ Kesin hata 5│
│         │ │   12. paragraf        11.5 │  │ Uyarı      2│
│         │ │ (açık bulgu: mesaj, alıntı,│  │ İncele     1│
│         │ │  gerekçe, önerilen düzeltme)│  │ 26 p · 178 k│
```

Bulgu yok:

```
│ sidebar │ toolbar: dilekce.docx        Bulgu yok           │
│         │                                                   │
│         │              Bulgu bulunmadı              18/600  │
│         │              Bu belge tanımlı kuralların          │
│         │              hiçbirine takılmadı.                 │
│         │                                                   │
│         │              13 paragraf · 84 kelime incelendi    │
```

Ekranın geri kalanı ölmez: toolbar belgenin adını ve `Bulgu yok` durumunu
taşımaya devam eder, kenar çubuğu kipi gösterir. Üçüncü kolon açılmaz —
üç kez "0" yazan bir sütun bilgi üretmez.

---

## G. Ayarlar — gerçek macOS tercihler penceresi

```
┌──── 660 px ──────────────────────────────────────────┐
│  Genel          │  Genel                       18/600 │
│  Görünüm        │                                     │
│  Erişilebilirlik│  Çıktı klasörü      Dönüştürülen…   │
│  Hakkında       │  Dönüştürülen …     [Değiştir…]     │
│  Telif          │                                     │
│  Geri bildirim  │  Son kullanılanlar  [Listeyi temizle]│
│                 │  3 belge hatırlanıyor               │
│     160 px      │                            [Bitti]  │
└──────────────────────────────────────────────────────┘
```

- **Tek grid**: etiket/açıklama solda, kontrol sağda `200 px` sabit
  kolonda. Kontroller satırdan satıra kaymaz (§13).
- Yükseklik sabit (500 px) — sekme değişince pencere zıplamaz.
- `Genel` gerçek tercihler taşır: çıktı klasörü (mevcut, çalışan komut) ve
  son kullanılanlar. Uydurma tercih eklenmedi; komut yoksa satır çizilmez.
- Bölümler boşlukla ayrılır; kutu, çerçeve, satır çizgisi yok.

---

## H. Hakkında

```
▣  GölgeDosya
   Sürüm 0.0.1

   Belge çalışma ortamı. Dört aracın — Düzenle, Dönüştür,
   Karşılaştır, Denetle — tek pencerede birleşmiş hâli.

   Geliştirici        Raci Çetin Yüksekbaş
   Telif              © 2026 Raci Çetin Yüksekbaş
```

İşaret 32 px. Pazarlama başlığı, slogan, rozet yok. Güncelleme kanalı
olmadığı için güncelleme satırı da yok — var olmayan bir şeyin yüzeyi
çizilmez. Bundle kimliği gösterilmez (geliştirme bilgisidir).

---

## I. Telif

```
Copyright © 2026 Raci Çetin Yüksekbaş
All Rights Reserved

GölgeDosya tescilli (proprietary) bir yazılımdır. Adı, görsel
kimliği, kullanıcı arayüzü ve özgün bileşenleri korunmaktadır.

Üçüncü taraf bildirimleri                            14/500
Uygulama açık kaynak bileşenler kullanır; lisans bildirimleri
kaldırılmamıştır.

  MIT                react · lopdf · zip · quick-xml · tiff …
  Apache-2.0 / MIT   tauri · serde · image · rayon …
  Apache-2.0         pdfjs-dist
  BSD-2 / BSD-3      mammoth · sprintf-js …

Tam liste ürünle birlikte dağıtılır.
```

Uzun lisans metinleri ekrana yığılmaz; aileler özetlenir, bildirimler
**kaldırılmaz**.

---

## J. Geri bildirim

Kapasite gerçeği: uygulamanın **ağ izni yoktur**, bildirilmiş bir iletişim
uç noktası yoktur, `opener` izni yalnız Dönüştür'ün çıktı klasörünü
Finder'da göstermesi içindir. Bu yüzden bir gönderme uç noktası
**uydurulmadı** (§16).

Var olan kapasiteyle çalışan yüzey:

```
Geri bildirim

Sorun, öneri veya genel görüşünüzü yazın. Yazdığınız metin
uygulamadan kendiliğinden çıkmaz; belge içeriğiniz hiçbir
zaman gönderilmez.

┌──────────────────────────────────────────┐
│                                          │  textarea
│                                          │  7 satır
└──────────────────────────────────────────┘

[ Metni Kopyala ]   Bu sürümde uygulama içinden gönderim yoktur.
```

`Metni Kopyala` gerçekten çalışır (webview panosu; ağ yok, izin
gerektirmez) ve metni kullanıcının kendi kanalına taşır. Ölü düğme, sahte
uç nokta, hayalî "gönderildi" onayı yok.

---

## K. Light / Dark

Her ikisi de birinci sınıf.

- **Light** saf beyaz değil: chrome `#EFEEEC`, workspace `#F6F5F3`,
  yükseltilmiş `#FBFAF9`. İçerik yüzeyi chrome'dan **daha açıktır**.
- **Dark** saf siyah değil: sumi `#18181A`–`#242426` aralığı.
- Aynı token adları, aynı ritim, aynı ölçü. Tema yalnız ton değiştirir;
  hiçbir ekranın kompozisyonu temaya göre değişmez.
- Semantik renkler her iki temada da kontrast eşiğini geçer ve **hiçbir
  anlam yalnız renkle verilmez**: ciddiyet şekil + etiket taşır.

---

## Yasak listesi (§28) — bu turda uygulanan denetim

`center everything` · `card grid` · `pill overload` · `huge whitespace` ·
`gold accent` · `gradient` · `glass` · `glow` · `fake premium` ·
`AI dashboard` · `landing page` · `hero empty state` ·
`rounded-everything` · `all-caps spam`

Sadeleştirme testi (§29) her öğeye iki soru sorar: *gerekli mi* ve
*kaldırırsam ekran görevini anlatmaya devam eder mi.* İkincisine hayır
diyen öğe kalır — bu turda kalan öğeler: toolbar durum metni, son
kullanılanlar listesi, desteklenen türler satırı, Karşılaştır yuvaları,
Dönüştür çıktı konumu, Denetle künyesi.

---

## Bilerek yapılan üç sapma

Şartnameden üç noktada ayrıldım; üçü de "etiket gerçeği söylesin" kuralına
dayanıyor.

1. **`Son PDF'ler` → `Son Belgeler` (§9).** Düzenle yalnız PDF açmıyor —
   DOCX, UDF ve görselleri de açıyor ve liste onları da taşıyor. Başlığı
   "Son PDF'ler" bırakmak, altında `DOCX` yazan satırlar dururken etiketi
   yalancı yapardı. Filtre zaten "bu kipin açabildiği" kümesini veriyor.

2. **`Dönüştür` düğmesi barda değil akışın içinde (§7 ↔ §10).** §7 barda,
   §10 akışta gösteriyordu. İkisini birden çizmek §22'nin "ekranda tek
   baskın eylem" kuralını bozuyordu. İş akışta yapılıyor; düğme de orada.
   Bar yönü taşıyor: `Word (.docx) → UYAP (.udf)`.

3. **`[ Geri Bildirim Gönder ]` → `[ Metni Kopyala ]` (§16).** Ürünün ağ
   izni yok, bildirilmiş bir uç nokta yok. §16'nın kendi kuralı —
   "yeni backend icat etme, var olan capability neyse onu kullan" —
   gönderme düğmesini imkânsız kılıyor. Ölü bir düğme yerine gerçekten
   çalışan bir eylem kondu; yüzey durumu açıkça yazıyor.

---

## Olgunlaştırma turu — değişenler

Sistem yeniden kurulmadı; üç zayıf alan ürün seviyesine çekildi.

**Düzenle.** Sayfa şeridi bir gezgine döndü: üç yüzey artık tonla ayrılıyor —
gezgin (`--surface-app`) · önizleme kuyusu (`--surface-sunken`) · kâğıt
(`--surface-document`, iki temada da aynı). Görüntülenen sayfa tonal yüzey +
2 px işaret, çıktıya dahil edilen sayfa kâğıdın kendi üzerindeki ince
çerçeve. Onay kutuları sistemin mavisini değil ürünün nötr tonunu kullanıyor.
Bekleme, dev kutunun ortasındaki tek kelime değil; küçük döner + kısa etiket.
Panel başlığı `Araçlar`, seçili araç bloğu kısaldı, `Sil` seçiliyken yıkıcı
karakterini metin ve ince işaretle taşıyor — kırmızı dolgu yok.

**Dönüştür.** Akış 680 px'lik sütunda, üstten 72 px içeride. Ok artık
adların satırında (biçim satırının yanına düşüyordu). **Tamamlandığında akış
kaybolmuyor**: aynı iki durak duruyor, hedef bir biçim adı yerine üretilen
dosya oluyor; altında ✓ satırı, amber tek satırlık uyarı, çıktının yeri ve
tek birincil eylem.

**Karşılaştır ve Denetle.** Yalnız mikro-hiyerarşi: panel başlıkları cümle
düzeninde (`Farklar`, `Denetim özeti`), sayılar sağda ve küçük noktalarla,
seçili fark kart değil solda vurgulu hafif yüzey, `Eski`/`Yeni` etiketleri,
kompakt filtre segmenti, 46 px fark satırı. Açık bulgu üç okunur bloğa
ayrıldı — `Sorun` · `Belgede` · `Önerilen düzeltme` — ve arka planı
hafifledi; vurgu solda ciddiyet renginde ince bir işaret.

**Ortak.** Boşluk ölçeğine 20 px eklendi (4 · 8 · 12 · 16 · 20 · 24 · 32).
Arayüzde büyük harf zorlaması kalmadı; kullanılmayan `Pill` ilkeli kaldırıldı.
Düzenle'nin karşılama metni artık Word/UYAP/görsel dosyalarının PDF'ye
dönüştürüldüğünü söylüyor — liste bu türleri gösterdiğine göre vaat de
dürüst olmalı.
