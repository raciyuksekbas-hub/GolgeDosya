# Arayüz denetimi — GölgeDosya

Bu belge, arayüzü yeniden mühendislikten geçirmeden önce yapılan envanterdir.
Her görünür parça için tek soru soruldu: **bu parça hangi kullanıcı problemini
çözüyor ve kaldırırsam kullanıcı işini hâlâ yapabilir mi?** Cevabı tatmin
etmeyen her parça aşağıda gerekçesiyle birlikte işaretlendi.

Sınıflar: `KEEP` · `SIMPLIFY` · `MOVE` · `HIDE` (yalnız gerektiğinde görünür) ·
`REMOVE`.

---

## 0. Görev modeli (Phase B)

Her kipin tek cümlesi. Bir ekrandaki her parça bu cümleye hizmet etmek
zorundadır; etmiyorsa parça değil, gürültüdür.

| Kip | Kullanıcının tek cümlesi |
|---|---|
| **Düzenle** | Kullanıcı PDF'nin sayfaları üzerinde değişiklik yapıp yeni bir kopya almak istiyor. |
| **Dönüştür** | Kullanıcı belgenin biçimini değiştirmek istiyor. |
| **Karşılaştır** | Kullanıcı iki belge arasında neyin değiştiğini görmek istiyor. |
| **Denetle** | Kullanıcı belgede dikkat etmesi gereken bir şey olup olmadığını görmek istiyor. |

Home'un cümlesi kipe ait değil, ortama aittir: **kullanıcı çalışacağı belgeyi
açmak istiyor.**

---

## 1. Hiçbir şeyin çizmediği kod

Envanterin ilk bulgusu görsel değil: arayüzün bir bölümü hiç çizilmiyor.

| Parça | Durum | Karar |
|---|---|---|
| `Toolbar`, `ToolbarTitle`, `ToolbarSpacer`, `ToolbarGroup`, `ToolbarDivider` | hiçbir ekranda kullanılmıyor | `REMOVE` |
| `Divider`, `WorkspaceHead` | hiçbir ekranda kullanılmıyor | `REMOVE` |
| `EmptyState` (ilkel) | kullanılmıyor — dört ekran kendi boş durumunu elle yazıyor | `SIMPLIFY` (§2) |
| `.toolbar-title`, `.toolbar-subtitle`, `.toolbar-group`, `.toolbar-divider`, `.divider`, `.workspace-head*` | ölü CSS | `REMOVE` |
| `.btn-danger`, `.doc-meta`, `.progress`, `.progress-bar`, `.prose`, `.row-end`, `.surface-wide` | ölü CSS | `REMOVE` |
| `.is-drop-target`, `.is-ready`, `.status-toggle`, `.status-actions`, `.status-meta`, `.update-notice`, `.pane-revised` | bağımsız uygulamadan kalan ölü CSS | `REMOVE` |
| `Icon.tsx` içindeki `copy`, `chevron-left/right`, `lock`, `info`, `x`, `check`, `sync`, `filter` | 14 simgenin 9'u çizilmiyor | `REMOVE` |

Ayrıca iki simge dili var: kabuk 16 ızgarada 1.3 çizgi, Karşılaştır 24
ızgarada 1.6 — küçültülünce ~1.0'a iniyor ve aynı pencerede iki farklı çizgi
kalınlığı görünüyor. `SIMPLIFY`: çizgi kalınlığı ölçekten bağımsız hâle
getirilip tek değere bağlanır.

---

## 2. Aynı işi yapan dört ayrı boş durum

| Yer | Sınıf | Yapı |
|---|---|---|
| Home — hiç kayıt yok | `.dropzone > .empty` | ızgara 38fr/auto/62fr, ikon + başlık + ipucu + eylem |
| Düzenle — belge yok | `.preview-empty` | flex ortalı, başlık + ipucu |
| Düzenle — açılamadı | `.preview-failed` | flex ortalı, başlık + cümle + eylem |
| Denetle — bulgu yok | `.review-clear` | ızgara 38fr/auto/auto/62fr, başlık + cümle + künye |

Dört ayrı CSS bloğu, üç ayrı dikey hizalama kuralı, iki ayrı optik konum.
Aynı bilgi mimarisi: **ne oldu → tek cümle → (varsa) tek eylem.**

`SIMPLIFY`: tek `EmptyState` ilkeli, tek CSS bloğu, tek optik konum
(kalan alanın %38'i). Dört ekran da aynı yerde, aynı ritimde durur.

---

## 3. Karşılaştır — aynı sayı üç kez, gezinme iki kez

| Parça | Gözlem | Karar |
|---|---|---|
| Ray: `FARKLAR 22` bloğu | Toplam sayı rayın altındaki `1/22` konum göstergesinde zaten var | `REMOVE` |
| Ray: `1/22` konum | Seçili farkın kaçıncısı olduğunu söyleyen tek yer | `KEEP` |
| Panel: `22 değişiklik` | Özetin başlığı; panel kapalıyken ray taşır | `KEEP` |
| Panel: `%73 / %0 / %27` | Sayılar zaten yanında; yüzde türetilmiş ve karar değiştirmiyor | `REMOVE` |
| Panel: `SEÇİLİ FARK` + "Rayda ya da listede bir fark seçin." | Boş yer tutucu; ilk fark açılışta seçilirse hiç oluşmaz | `SIMPLIFY` |
| Panel: `↑ ↓ 22 fark arasında gezin` | **Böyle bir kısayol yok.** Hiçbir yerde ok tuşu bağlanmamış | `REMOVE` |
| Panel: fark listesi | Rayla aynı işi yapar ama her farkın özetini taşır; ray konumu, liste içeriği verir | `KEEP` |
| Filtre sekmeleri | Dört filtre, listeyi daraltır | `KEEP` |

---

## 4. Dönüştür — ekranın %90'ı boş, panel bir dosya yolu taşıyor

| Parça | Gözlem | Karar |
|---|---|---|
| KAYNAK → HEDEF akışı | Kipin tüm işi bu; ama sol üstte sıkışık, altında 900 px ölü alan | `SIMPLIFY` (optik konum) |
| `ÇIKTI` paneli | Üçüncü kolonun tamamı bir klasör yolu + iki düğme için açılıyor | `MOVE` |
| Tam yol, monospace, dokuz satır sarmalı | Kullanıcı klasörü tanır, yolu okumaz | `SIMPLIFY` (klasör adı; tam yol ipucunda) |
| `Finder'da Göster` | Dönüştürmeden **önce** anlamsız, sonra birincil | `HIDE` |
| `Değiştir…` / `Varsayılana dön` | Gerçek tercih | `KEEP` (akışın altında tek satır) |

---

## 5. Düzenle — sayfa araçları iki ayrı yerde

| Parça | Gözlem | Karar |
|---|---|---|
| Belgenin üstündeki şerit: `Seç · Sırala · Sil · Döndür` | Sayfa işi belgenin yanında yapılır; doğru ev | `KEEP` |
| Panel: `SAYFALAR` grubu — aynı dört araç, uzun etiketle | Aynı dört komut, iki ayrı yerde | `REMOVE` |
| Panel: `BELGE` grubu (Birleştir · Sıkıştır · PDF→PNG) | Sayfa seçimi gerektirmeyen, belgenin bütününe ait işler | `KEEP` |
| Panel: katlı `DİĞER` | Ayar isteyen nadir işler | `KEEP` |
| Panel: `SEÇİLİ ARAÇ` | Seçilen aracın ayarı ve kaynak listesi | `KEEP` |
| Sayfa şeridi / araç paneli / kaydetme eylemleri | Belge durumuna bağlı (önceki tur) | `KEEP` |

---

## 6. Denetle

| Parça | Gözlem | Karar |
|---|---|---|
| Bulgu satırı: şekil + metin etiketi + başlık + konum | Ciddiyet asla yalnız renkle verilmiyor; sözleşme testi var | `KEEP` |
| Satır içi akordeon (mesaj, paragraf alıntısı, gerekçe, düzeltme) | Kanıt bulgunun yanında | `KEEP` |
| `DENETİM ÖZETİ` paneli | Yalnız bulgu varken (önceki tur) | `KEEP` |
| 0 bulgu durumu | Ortak boş duruma taşınır (§2) | `SIMPLIFY` |

---

## 7. Home ve kabuk

| Parça | Gözlem | Karar |
|---|---|---|
| Kenar çubuğu: kimlik + dört kip + Ayarlar | Dört satır, tonal seçim, vurgu boyaması yok | `KEEP` |
| Yardımcı bar: belge çipi + kip eylemi + panel düğmesi | Sayfa başlığı taşımıyor; boşken bant olmaktan çıkıyor | `KEEP` |
| Son kullanılanlar | Yüzeyin asıl içeriği | `KEEP` |
| `veya belgeyi buraya sürükleyin` + `Listeyi temizle` | Yüzeyin alt çerçevesi | `KEEP` |
| Ayarlar sheet'i | Görünüm · Erişilebilirlik · Hakkında | `KEEP` |

---

## 8. Kalan borç (bu turda kapatılmıyor)

- **Düzenle DOCX/UDF sözü.** Kip bu türleri açabildiğini söylüyor ama dönüşüm
  harici bir bileşene bağlı; yoksa belge açılamıyor. Durum artık dürüstçe
  anlatılıyor (`Belge açılamadı` + kurtarma eylemi) ama vaadin kendisi motor
  tarafında bir karardır, sunum katmanında düzeltilemez.
- **Kaydetme hatalarının metni.** Motorun kendi cümlesi taşınıyor; yalnız sınıf
  öneki temizleniyor. Kategori eşlemesi açma yolunda var, kaydetme yolunda yok.
