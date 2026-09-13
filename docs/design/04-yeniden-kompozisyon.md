# Yeniden kompozisyon — metinsel wireframe

Mevcut görünüm reddedildi. Bu belge, kod yazılmadan önce her ekranın yeni
kompozisyonunu ve oranlarını sabitler. Ölçüler 1280×800 pencere içindir.

Kural: **border ile değil, yüzey tonu · boşluk · hizalama ile hiyerarşi.**

---

## 0. Kabuk

```
0        220                                           1280
├─────────┼───────────────────────────────────────────────┤
│ sidebar │ titlebar (trafik ışıkları)             28 px  │
│  220    ├───────────────────────────────────────────────┤
│         │ toolbar — yalnız anlamlı bağlam        46 px  │
│         ├───────────────────────────────────┬───────────┤
│         │ workspace                         │ inspector │
│         │                                   │  300 px   │
│         │                                   │ gerekirse │
└─────────┴───────────────────────────────────┴───────────┘
```

| Yüzey | Ton (koyu) | Ton (açık) |
|---|---|---|
| pencere / toolbar | `#1C1C1E` | `#EFEEEC` |
| kenar çubuğu | `#202022` | `#E9E8E5` |
| workspace | `#18181A` | `#F6F5F3` |
| yükseltilmiş (panel, sheet) | `#242426` | `#FBFAF9` |
| çizgi | `rgb(255 255 255 / 8%)` | `rgb(0 0 0 / 8%)` |
| metin | `.92` / ikincil `.58` | `#1C1C1E` / `#6B6B70` |

Kenar çubuğu ile workspace arasında **tek** düşük kontrastlı dikey çizgi.
Başka hiçbir yerde yatay çizgi yok: toolbar altı, footer üstü, ayarlar
bölümleri, kenar çubuğu altı — hepsi tonal fark ve boşlukla ayrılır.

**Kenar çubuğu.** Satır 31 px, ikon 16 px, metin 13 px. Seçili satır: çok
hafif tonal yüzey + solda 2 px sessiz işaret. Yuvarlak dev kutu yok.
Ayarlar altta, divider yok.

**Toolbar.** 46 px. Belge varsa solda adı, sağda o ekranın tek eylemi.
Belge yoksa toolbar boş bir bant değildir — zemini workspace ile aynıdır,
yalnız trafik ışıklarının yüksekliğini korur.

**Birincil eylem.** Koyu temada off-white (`#ECEAE5` / metin `#1C1C1E`),
açık temada mürekkep. Vurgu rengi geniş yüzey boyamaz.

**Tipografi.** Bağlam başlığı 17/600 · bölüm başlığı 13.5/500 · gövde 13 ·
künye 11.5. Büyük harf etiket yalnız panel başlığında.

**Radius.** 4 (küçük), 6 (kontrol), 8 (panel). Pill yok.

---

## 1. Home

```
│ sidebar │ toolbar:  (boş)                                    │
│         │                                                    │
│         │   ┌── 680 px ─────────────────────────┐            │
│         │   │ Son Kullanılanlar          13.5/500│           │
│         │   │                                    │           │
│         │   │ ▣  dava-dilekcesi.docx        46px │           │
│         │   │    DOCX · 12 dakika önce           │           │
│         │   │ ▣  ek-3 bilirkişi raporu.pdf       │           │
│         │   │    PDF · dün                       │           │
│         │   └────────────────────────────────────┘           │
```

- Liste yüzeyin tamamına yayılmaz: **680 px** okuma genişliği, solda
  workspace dolgusundan başlar.
- Satır 46 px, iki satır: ad (13) ve altında tür · zaman (11.5, ikincil).
- Solda 16 px belge simgesi.
- Hover: tonal, radius 6. Ayırıcı çizgi yok.
- Liste yukarıdan başlar. Altında ölü alan bırakmamak için **sürükle ipucu
  ve listeyi temizleme, listenin hemen altında** ikincil metin olarak durur;
  ekranın dibine çizgiyle çakılmaz.
- Kayıt yoksa: boş durum (§2).

---

## 2. Boş durum — tek yüzey

```
│         │            optik üst %38                           │
│         │         ┌── 360 px ──────────┐                     │
│         │         │ Belge açın    17/600│                    │
│         │         │ PDF dosyanızı açın  │                    │
│         │         │ veya buraya sürükle.│  12.5 ikincil      │
│         │         │                     │                    │
│         │         │ [ Belge Aç ]  küçük │                    │
│         │         └─────────────────────┘                    │
```

Dört ekranda aynı: Home (kayıt yok), Düzenle (belge yok / açılamadı),
Denetle (bulgu yok). İllüstrasyon yok, kutu yok, hardal düğme yok.

---

## 3. Düzenle

```
│ sidebar │ toolbar: ek-3 raporu.pdf              [Dışa Aktar] │
│         ├──────┬───────────────────────────────┬────────────┤
│         │ 124  │  ▸ sayfa araçları (segment)   │ 300        │
│         │ şerit│  ───────────────────────────  │ Belge işleri│
│         │      │                               │ (sessiz     │
│         │ □ 1  │        P D F   S A Y F A       │  satırlar)  │
│         │ ■ 2  │        (kahraman)             │            │
│         │ □ 3  │                               │ ▸ Diğer     │
│         │      │                               │            │
│         │      │  not (11.5, ikincil)          │ seçili araç │
```

- PDF ekranın kahramanı: şerit 124 px, panel 300 px, kalan her şey sayfa.
- Sayfa işlemleri (seç · sırala · sil · döndür) belgenin üstündeki segment
  şeridinde; yakınlaştırma aynı şeritte sağda.
- Belge çapındaki işler (birleştir · sıkıştır · dışa aktar) panelde sessiz
  satırlar; nadir işler `Diğer` disclosure'ı altında.
- Belge yoksa: şerit yok, panel yok, boş durum (§2).

---

## 4. Dönüştür

```
│ sidebar │ toolbar: dava-dilekcesi.docx          [Dönüştür]   │
│         │                                                    │
│         │          optik üst %38                             │
│         │        ┌── 420 px ────────────────┐                │
│         │        │ dava-dilekcesi.docx  17/600│               │
│         │        │ DOCX  →  UDF        13     │               │
│         │        │                            │               │
│         │        │ Çıktı: Dönüştürülen Belgeler│ 11.5 ikincil │
│         │        │ Değiştir…            tertiary│              │
│         │        └────────────────────────────┘               │
```

Üçüncü kolon yok. Tamamlandığında aynı yerde sonuç: ne üretildi, nerede,
tek birincil eylem `Finder'da Göster`.

---

## 5. Karşılaştır

```
│ sidebar │ toolbar: v1.docx ↔ v2.docx      Fark 2 / 18       │
│         ├──────────────┬──┬──────────────┬─────────────────┤
│         │ temel sürüm  │ray│ değişik sürüm│ inspector 300   │
│         │              │48│               │ seçili fark     │
│         │  belge metni │  │  belge metni  │ + kısa künye    │
│         │              │  │               │ + liste         │
```

- Ana alan farkın kendisi; UI çizgisiz.
- Ray **konumu**, liste **içeriği** gösterir — ikisi aynı seçici gibi
  görünmemeli: ray dar ve grafik, liste metinsel.
- Fark sayacı toolbar'da (`Fark 2 / 18`); panelde tekrar edilmez.

---

## 6. Denetle

```
│ sidebar │ toolbar: dava-dilekcesi.docx     [Kopyaya uygula] │
│         ├───────────────────────────────┬──────────────────┤
│         │  ┌── 720 px ───────────────┐  │ 300              │
│         │  │ ● Kesin hata  Çoklu boşluk│  │ 5 kesin hata    │
│         │  │   6. paragraf        11.5 │  │ 2 uyarı         │
│         │  │ ───────────────────────── │  │ 1 incele        │
│         │  │ (açık bulgu: mesaj,       │  │                 │
│         │  │  alıntı, gerekçe, düzeltme)│  │ 26 paragraf ·   │
│         │  └──────────────────────────┘  │ 178 kelime      │
```

- Bulgu satırı iki satırlı: ciddiyet + başlık, altında konum.
- Bulgu yoksa: iki satır (§2 ritmi), panel yok.

---

## 7. Ayarlar

```
                ┌──── 460 px ────────────────┐
                │ Ayarlar             17/600 │
                │                            │
                │ Görünüm             13.5/500│
                │ Tema            [Koyu   ⌄] │
                │ Metin boyutu    [Normal ⌄] │
                │                            │
                │ Erişilebilirlik            │
                │ Yüksek kontrast [Sistem ⌄] │
                │ Hareketi azalt  [Sistem ⌄] │
                │                            │
                │ Hakkında                   │
                │ GölgeDosya 0.0.1           │
                │                     [Bitti]│
                └────────────────────────────┘
```

Bölümler **boşlukla** ayrılır; kutu, çerçeve ve satır çizgisi yok. Yüzey
nötr yükseltilmiş ton, radius 8.
