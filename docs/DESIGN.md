# Yüksekbaş Belge — tasarım dili

Apple'ın masaüstü disiplini, Japon tasarımının ölçülülüğü. Taklit değil:
Apple'dan **hiyerarşi ve native davranış**, Japon tasarımından **boşluğun bilgi
taşıması ve süsün yokluğu**.

Bu ürün, hukukçuların ciddi belgeler üzerinde saatlerce çalışacağı bir araçtır.
Dikkati kendine değil belgeye vermelidir.

## İlkeler

| | |
|---|---|
| **Ma** | Boşluk bilgi mimarisinin parçasıdır; ne sıkışık ne savruk |
| **Kanso** | Bilgi, eylem veya yön üretmeyen öğe kaldırılır |
| **Shibui** | Sessiz premium; incelik kullandıkça fark edilir |
| **Seijaku** | Yoğun belge işinde sakinlik veren görsel ritim |

Wabi-sabi **değil**: ürün rustik veya organik değil, modern ve hassastır.

## Phase A — mevcut arayüz envanteri (2026-09-08)

Yeniden tasarımdan önce bulunan durum:

| Ekran | Dosya | Sorun |
|---|---|---|
| Kabuk | `Layout`, `Sidebar` | Ayrı "sayfa başlığı" bloğu dikey alan yiyor; toolbar yok |
| Ana yüzey | `DocumentSurface` | Son kullanılanlar tür ve zaman göstermiyor |
| Dönüştür | `ConvertWorkspace` | Bölüm başlığı satır içi stille elle kuruluyor |
| Denetle | `ReviewWorkspace` | Aynı satır içi başlık tekrar; `.notice` her şey için kutu |
| Karşılaştır | `compare.css` | **Kendi tam design system'i**: `--brand-teal`, `--brand-coral`, Manrope fontu |
| Düzenle | `pdf.css` | DüzenEk değişken adlarını yeniden bağlıyor |
| Ayarlar | `Settings` | Dört seçim kutusu, bölümsüz modal |

**Kök bulgu.** Üç ayrı görsel dil yan yana yaşıyordu: kabuk token'ları,
Karşılaştır'ın kendi markası, Düzenle'nin yeniden bağlanmış adları. Ürünün "dört
uygulamanın yan yana dizilmiş hâli" gibi hissettirmesinin sebebi buydu.

İkinci bulgu: **belge bağlamı her kip değişiminde siliniyordu** (`App.tsx`,
`setDocuments([])`). Kullanıcı aynı belgeyi her kipte yeniden seçmek zorundaydı.

## Phase B — bilgi mimarisi

### Dört kip, tek ortam

Düzenle · Dönüştür · Karşılaştır · Denetle — bunlar eski ürün adlarının yan yana
dizilmiş hâli değil, **tek belge çalışma ortamının dört kipidir**. Eski adlar
(Tavzih, DüzenEk, Değişikİş, İkinciGöz) teknik katmanda `FeatureState.key`
olarak kalır; kullanıcı arayüzünde görünmez.

### Belge bağlamı kipler arasında korunur

`shell/modes.ts` tek kaynaktır: hangi kip hangi türü açar, kaç belge ister.
`carryContext` üç sonuç üretir:

| Sonuç | Ne zaman | Davranış |
|---|---|---|
| `keep` | Yeni kip belgeyi açabiliyor | Belge taşınır, yeniden seçim yok |
| `needsMore` | Karşılaştır'a tek belgeyle gelindi | İkinci belge istenir, ilki korunur |
| `mismatch` | Tür uyuşmuyor | **Sessizce düşmez**; sade bir açıklama gösterilir |

Bir belge isteyen kipe iki belgeyle gelinirse ikincisi atılmaz — kullanıcı
Karşılaştır'a döndüğünde yine oradadır.

### Yerleşim

```
┌──────────┬────────────────────────────────────┐
│ Sidebar  │ Toolbar: başlık + bağlamsal eylem  │
│  kipler  ├────────────────────────────────────┤
│          │                                    │
│  açık    │   Workspace          │ Inspector   │
│  belge   │                      │ (gerekirse) │
└──────────┴────────────────────────────────────┘
```

Inspector yalnız gösterilecek içerik varken çizilir. Boş üçüncü kolon açılmaz.

## Phase C — token'lar

Tek sistem; üç ayrı palet kaldırıldı.

**Yüzeyler.** Açık temada saf beyaz yok: chrome hafif sıcak nötr, belge yüzeyi
ondan daha açık. Sepia/parşömen değil. Koyu temada saf siyah yok: çok koyu sumi,
yüzeyler arasında yalnız hafif ton farkı; belge yüzeyi beyaz kalır ve chrome'dan
net ayrılır.

**Vurgu.** Tek renk: mürekkep yeşili `#2f5d50`. Mavi "teknoloji" klişesinden
uzak. Yalnız seçili navigasyon, birincil eylem, aktif durum ve odak.

**Semantik.** `danger` · `warning` · `review` · `success`. Neon değil, paletten
bir adım güçlü. **Hiçbir anlam yalnız renkle verilmez** — işaret ve metin etiketi
eşlik eder.

**Tipografi.** macOS sistem fontu; yeni font paketlenmez. 19 / 15 / 13 / 11.5 px.
Uzun metin `62ch` ile sınırlı.

**Ölçü.** 4'ün katları. Radius küçük (4/6/8) — masaüstü aleti, yuvarlak kart değil.

**Hareket.** 120–180ms, `cubic-bezier(0.32, 0.08, 0.24, 1)`. Spring/bounce yok.
Hareket dikkat istemez, durumu anlatır.

## Phase D — ortak ilkeller

`shared-ui/primitives.tsx`: Button · IconButton · Toolbar (+Title/Group/Divider/
Spacer) · Section · Divider · EmptyState · Status · Inspector · Field.

`IconButton` erişilebilir adı **zorunlu** tutar: `label` hem tooltip hem
`aria-label` olur. Tooltip tek başına ekran okuyucuya yetmez.

`Status` her tonu bir işaretle eşler; renk tek taşıyıcı değildir.
