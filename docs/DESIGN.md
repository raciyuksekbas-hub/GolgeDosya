# GölgeDosya — tasarım dili

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

## Görsel yeniden kompozisyon (2026-09-10)

Tasarım turu sonundaki kabuk sahibi tarafından reddedildi: başlık çubuğunun
altında ikinci bir bant, yönsüz boş çalışma alanı, ortada küçük bir CTA. Yeni
referans NöbetçiTakvim ürün ailesi. Ölçüm ve tercüme
`docs/design/01-referans-analizi.md`, yerleşim `docs/design/02-yerlesim-haritasi.md`.

Aşağıdaki kayıtlı kararlar **bilinçli olarak geçersizleşti**:

- *"Uygulama adı kenar çubuğunda tekrarlanmaz"* (4c12917 / 4c12689): pencere
  başlığı artık gizli (`titleBarStyle: Overlay`, `hiddenTitle`), dolayısıyla ad
  yalnız kenar çubuğunun kimlik bloğunda yaşar — tek renkli işaret + ad, slogan
  yok.
- *"Düzenle ve Karşılaştır migration dondurması"* (`PdfWorkspace.tsx`,
  `pdf.css`, `compare.css` başlık yorumları): yerleşim ve panel sunumu yeniden
  kompoze edilir. Dondurulmuş olan yalnız **motor ve davranış**tır: `duzenek_*`
  komut kümesi, `rotatePages/previewGeometry`, `compare.css`'in belge satırı /
  fragment / ray kuralları (`:243-334` eski numaralandırma), satır eşitleme DOM
  sözleşmesi. Bu davranışlar için testler korunur; yerleşim testleri aynı
  commit'te yeni kompozisyonu tarif edecek biçimde yenilenir.
- Token sayıları: 19/15/13/11.5 → 20/15/13/12 + 11 etiket; 204/288/44 →
  232/300/48; yükseklikler metin ölçeğine bağlı.
- Inspector: kabuk artık gerçekten çiziyor (`src/shell/chrome.tsx`,
  `InspectorPanel`), içeriden yerleşik panel olarak. Home'da hiçbir kipte panel
  yok. `Inspector` ilkeli kaldırıldı.
- Odak rengi sistem mavisi kalır (şartname §16'daki "focus" kaleminden bilinçli
  sapma): vurgu dolgulu birincil düğme üzerinde de görünür.
- `compare.css`'in bağımsız uygulamadan kalan 159 ölü/kapsamsız kuralı silindi
  (kabuk, kenar çubuğu, topbar, hakkında, güncelleme, toast, açılış animasyonu,
  küresel `button`/`::selection`/kaydırma çubuğu/odak/hareket kuralları). Kabuğun
  `.sidebar`'ına `gap: 2px` sızdırıyor ve temanın "OS azalt dese de göster"
  tercihini eziyordu.
- Pencere: 1280×800 varsayılan (kabul edilen tam kompozisyon ilk açılış olsun),
  minHeight 600 (şartnamedeki 900×600 ulaşılabilir olsun). Örtüşük başlık
  çubuğu bir pencere görünümü ayarıdır; yetenek listesi değişmedi
  (`allow-start-dragging` artık tüketiliyor). Tam ekranda üst bant 12 px.
  Açılışta ~100 ms açık chrome karesi kabul edildi; tema-başına arka plan için
  yetenek genişletilmez.

## Marka: GölgeDosya (2026-09-10)

**Görsel kimlik geçicidir.** "Kat" işareti, pirinç palet ve marka varlıkları,
dış tasarım seti gelene kadar ürünün adsız/işaretsiz kalmaması için üretildi;
final tasarım kabulü değildir. Ad, kimlik (`tr.yuksekbas.golgedosya`), veri
devralma ve varlık üretim hattı kalıcıdır — değişecek olan yalnız
`brand/` içindeki çizimlerdir.

Ürünün adı `GölgeDosya`. Kimlik `apps/belge-shell/brand/` altında tek kaynakta
tutulur (`BRAND.md`, `appicon.svg`, `build-icons.sh`); `src-tauri/icons/`
tamamen oradan türetilir ve elle düzenlenmez.

İşaret "Kat": sağ alt köşesi kaldırılmış bir sayfa; altında pirinç bir katman.
Vurgu rengi mürekkep yeşilinden **eskitilmiş pirince** döndü — açık temada
`#8a6a3e`, koyu temada `#c9a472`. Yüzeyler, tipografi, ölçüler ve yerleşim
değişmedi.

Geçersizleşen kararlar:

- *"Kenar çubuğunda tek vurgu taşıyıcı seçili kip işaretidir"*: kimlik işareti
  de markanın pirincini taşır. Kural artık şöyle okunur — seçili kip işareti
  tek **etkileşimli** vurgudur; kimlik bir kontrol değildir.
- *"Açılış ekranı yok"* kararı **korundu**. Kısa süre bir `Splash` denendi;
  ölçüm gösterdi ki pencerenin yakalanabilir ilk karesi zaten çizilmiş ana
  ekran oluyor, yani açılış ekranı en fazla tek karelik bir parlama olurdu.
  Kaldırıldı. Sürüm Ayarlar → Hakkında'da.
- Yapılandırma kimliği `tr.yuksekbas.belge` → `tr.yuksekbas.golgedosya`.
  Eski dizin bir migration kaynağıdır: yeni ad altında henüz `settings.json`
  yoksa eskisi devralınır, varsa dokunulmaz. Eski dizin **silinmez**.
