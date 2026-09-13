# Mod karakteri ve ürün yüzeyleri — wireframe

Önceki kompozisyonun kusuru: dört kip aynı boş-durum şablonunu paylaşıyordu,
ekranın çoğu ölü alandı ve ürünün kendisi (hakkında, telif, geri bildirim)
hiç yoktu. Bu belge kod yazılmadan önce yeni kompozisyonu sabitler.

Kural aynı: hiyerarşi çizgiyle değil, yüzey tonu · boşluk · hizalama ile.
Yeni kural: **her kip aynı sistemde, farklı görev modeliyle.**

---

## 0. Kenar çubuğu

```
GölgeDosya

  Düzenle
  Dönüştür
  Karşılaştır
  Denetle

  ────────────────

  Ayarlar
  Hakkında
  Geri bildirim
```

Kipler üstte, ürün yüzeyleri altta; aralarında tek ince ayırıcı. Ayarlar,
Hakkında ve Geri bildirim artık gizli değil — üçü de tercihler penceresini
kendi sekmesinde açar.

---

## 1. Belge yüzeyi (dört kipin karşılama ekranı)

Boş durum ekranın matematiksel merkezinde değil; **680 px'lik çalışma
sütununda, üstten ~%18'de ve sola hizalı**. Altında gerçek içerik: son
kullanılanlar. Yapay kart yok.

```
│ sidebar │ toolbar: Düzenle                       [PDF Aç]  │
│         │                                                   │
│         │  ┌── 680 px ───────────────────────────┐          │
│         │  │ PDF üzerinde çalışın          17/600│          │
│         │  │ Sayfaları düzenlemek, döndürmek,    │ 12.5     │
│         │  │ sıralamak veya dışa aktarmak için   │          │
│         │  │ bir PDF açın.                       │          │
│         │  │                                     │          │
│         │  │ Son Kullanılanlar            13.5/500│         │
│         │  │ ▣ dava-dilekcesi.docx         46 px │          │
│         │  │   DOCX · 12 dakika önce             │          │
│         │  └─────────────────────────────────────┘          │
```

Kipe göre değişen tek şey metin ve birincil eylem — kompozisyon aynı:

| Kip | Başlık | Açıklama | Eylem |
|---|---|---|---|
| Düzenle | PDF üzerinde çalışın | Sayfaları düzenlemek, döndürmek, sıralamak veya dışa aktarmak için bir PDF açın. | `PDF Aç` |
| Dönüştür | Belge dönüştürün | Word ve UYAP biçimleri arasında: DOCX ⇄ UDF. | `Belge Aç` |
| Karşılaştır | İki belgeyi karşılaştırın | İlk belgeyi açın, ardından karşılaştıracağınız ikinci belgeyi seçin. | `İlk Belgeyi Aç` |
| Denetle | Belgeyi denetleyin | Göndermeden önce son okuma: DOCX veya UDF dosyanızı açın. | `Belge Aç` |

Hiç kayıt yoksa liste bloğu hiç çizilmez; sürükleme ipucu açıklamanın altında
tek satır kalır.

---

## 2. Toolbar

Boş bant yok. Belge yokken **solda kipin adı**, sağda kipin birincil eylemi.
Belge açıkken solda belgenin adı, sağda o kipin eylemleri.

```
Düzenle                                            [ PDF Aç ]
ek-3 bilirkişi raporu.pdf      [Klasör seç] [Yeni PDF kaydet] [Kapat]
```

---

## 3. Düzenle · Dönüştür · Karşılaştır · Denetle (belge açıkken)

Önceki turda sabitlenen kompozisyonlar korunur (bkz. 04). Kipin karakteri
belge açıkken zaten kendini gösteriyor: Düzenle'de sayfa şeridi + tuval,
Dönüştür'de kaynak→hedef, Karşılaştır'da iki pano + ray, Denetle'de bulgu
listesi + özet.

---

## 4. Tercihler penceresi

Tek modal içindeki üç blok kalktı. Solda sekme rayı olan, geniş ama hafif bir
sheet:

```
┌──── 660 px ──────────────────────────────────────────┐
│  Görünüm        │  Görünüm                    17/600 │
│  Erişilebilirlik│                                    │
│  Hakkında       │  Tema              [Koyu      ⌄]   │
│  Telif          │  Metin boyutu      [Normal    ⌄]   │
│  Geri bildirim  │                                    │
│   160 px        │                            [Bitti] │
└──────────────────────────────────────────────────────┘
```

- Sekme rayı 160 px, satırlar kenar çubuğuyla aynı dilde (31 px, tonal seçim).
- İçerik soldan hizalı, bölümler boşlukla ayrılır, kutu ve çizgi yok.
- `Bitti` sağ altta, ikincil.
- **`Genel` sekmesi yok:** bu üründe oraya ait gerçek bir tercih bulunmuyor;
  çıktı klasörü işin yapıldığı yerde, Dönüştür yüzeyinde duruyor. Boş bir
  sekme açmak yerine hiç açılmadı.

### Hakkında

```
▣  GölgeDosya
   Sürüm 0.0.1

   Belge çalışma ortamı. Dört LegalTech aracının — Düzenle,
   Dönüştür, Karşılaştır, Denetle — tek pencerede birleşmiş hâli.

   Geliştirici   Raci Çetin Yüksekbaş
   Kimlik        tr.yuksekbas.golgedosya
```

Aşırı marka sunumu yok: uygulama işareti 32 px, ad, sürüm, bir cümle, iki
künye satırı.

### Telif

```
Copyright © 2026 Raci Çetin Yüksekbaş
Tüm hakları saklıdır.

GölgeDosya tescilli (proprietary) bir yazılımdır. Adı, görsel
kimliği, kullanıcı arayüzü ve özgün bileşenleri korunmaktadır;
izinsiz çoğaltılamaz, değiştirilemez, yeniden yayımlanamaz.

Üçüncü taraf bildirimleri              13.5/500
Uygulama açık kaynak bileşenler kullanır; lisans bildirimleri
kaldırılmamıştır.

  MIT                    react · lopdf · zip · quick-xml · tiff …
  Apache-2.0 / MIT       tauri · serde · image · rayon …
  BSD-2 / BSD-3          mammoth · sprintf-js …
  Apache-2.0             pdfjs-dist

Tam liste: THIRD_PARTY_NOTICES.md
```

### Geri bildirim

```
Geri bildirim

Bu sürümde uygulama içinden gönderim yoktur.

GölgeDosya ağ bağlantısı kurmaz: belgeleriniz, adları ve
içerikleri hiçbir zaman dışarı çıkmaz. Geri bildiriminizi
geliştiriciye kendi kanalınızdan iletebilirsiniz.
```

Hayalî bir gönderme düğmesi, sahte bir uç nokta veya var olmayan bir bağlantı
üretilmedi; uygulamanın ağ izni yok ve yüzey bunu açıkça söylüyor.
