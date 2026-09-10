# Yerleşim haritası — kabuk ve beş ekran

Phase B. Koddan önce yerleşim. Her çizim `docs/design/01-referans-analizi.md`
tercümesine dayanır. Ölçüler token adıyla verilir; sayı gerekiyorsa o belgedeki
değerdir. Bağımsız eleştiri turunun (49 bulgu) kararları işlenmiştir.

## 0. Kalıcı kabuk modeli

```
┌──────────────┬────────────────────────────────────────────────────────────┐
│ ● ● ●        │ yardımcı bar (--toolbar-h)                                  │
│ ▢ Yüksekbaş  │ [bağlam: belge adı · tür]        [kip eylemleri] [Kapat]   │
│   Belge      ├──────────────────────────────────┬─────────────────────────┤
│              │                                  │ ┌─────────────────────┐ │
│ ▸ Düzenle    │  workspace                       │ │ PANEL BAŞLIĞI       │ │
│   Dönüştür   │                                  │ │                     │ │
│   Karşılaştır│                                  │ │  (yalnız içerik     │ │
│   Denetle    │                                  │ │   varken çizilir)   │ │
│ ─────────────│                                  │ │                     │ │
│ AÇIK BELGE   │                                  │ │                     │ │
│ dilekçe.docx │                                  │ └─────────────────────┘ │
│              │                                  │                         │
│ ─────────────│                                  │                         │
│ ⚙ Ayarlar    │                                  │                         │
└──────────────┴──────────────────────────────────┴─────────────────────────┘
  --sidebar-w    minmax(0, 1fr)                     --inspector-w + inset
```

Kurallar:

- Native başlık çubuğu örtüşük (`Overlay`, başlık gizli). Trafik ışıkları kenar
  çubuğunun üst `--titlebar-h` bandında durur; kenar çubuğu, kimlik bloğu ve
  yardımcı bar `data-tauri-drag-region` taşır. Tam ekranda üst bant 12 px'e iner.
- Yardımcı bar **yalnız** içerik sütununun üstündedir; kenar çubuğu tepeye kadar
  çıkar. Bar sayfa başlığı taşımaz: solda belge bağlamı (kabuk çizer: ad + tür
  rozeti; belge yokken **boş**), sağda kipin eylemleri, en sağda kabuğun "Kapat"ı.
  Home'da bar boştur — §6 "boşsa sakin bırak".
- Sağ panel içeriden yerleşik yükseltilmiş yüzeydir (`--inspector-inset`); yalnız
  o kipin gerçek içeriği varken çizilir. ≥1280 açık gelir; altında varsayılan
  kapalı ve **her genişlikte** bardaki "Ayrıntılar" ikon düğmesiyle açılır
  (aria-pressed). Kapalıyken içerik bağlı kalır (durum kaybolmaz).
- Workspace başlığı yoktur Home'da; başlık tier'ı (`--text-title`) yalnız gerçek
  içerik başlığı olan yerde kullanılır.
- Kenar çubuğu: kimlik bloğu (tek renkli işaret + ad) → dört kip → (varsa)
  hairline + AÇIK BELGE → boşluk → hairline + Ayarlar. Seçili kip: tonal yüzey +
  2 px vurgu sol işaret + 500. Kenar çubuğunda başka vurgu taşıyıcı yok.
- Yoğunluk: kenar çubuğu ve bar kompakt; workspace ferah; panel orta.

### Sunum adaptörleri (motor değişmez)

Kipler kendi durumunu tutmaya devam eder. Kabuk iki adaptör sunar
(`src/shell/chrome.tsx`):

```
<ToolbarActions>…</ToolbarActions>              → yardımcı barın sağ ucuna
<InspectorPanel title scope?>…</InspectorPanel> → sağ panele; varlığı paneli açar
<InspectorSection title>…</InspectorSection>    → panel içinde ikinci bölüm
```

Mekanizma: React context + `createPortal`. **Sağlayıcı yokken** (sunucu tarafı
render, sözleşme testleri) içerik olduğu yerde satır içi çizilir; böylece
`renderToStaticMarkup(<PdfWorkspace/>)` araç listesini hâlâ içerir ve hiçbir
test DOM aramaz. `scope` (örn. `pdf-root`, `compare-root`) panel sarmalayıcısına
sınıf olarak eklenir: modülün kapsamlı CSS'i portal dışında da çalışır.

Sıra: `.toolbar-actions` (kip) **önce**, `.toolbar-shell` (Ayrıntılar, Kapat)
sonra — birincil eylem "Kapat"ın solunda durur. DOM sırası görsel sıradır.

Sınıf adı rezervi: kabuk, `compare.css`'in kapsamsız ilk sınıflarını kullanmaz
(`.sidebar-brand`, `.sidebar-group`, `.sidebar-foot`, `.topbar-*`, `.nav-*`,
`.toast`, `.filters`, `.paper`, `.pane`, `.button`, `.change-list`,
`.empty-lines`, `.file-types`, `.upload-zone`, `.added/.removed/.modified`).
Phase D bu ölü bağımsız-uygulama kabuğu kurallarını `compare.css`'ten siler
(sunum; motor bloğu :243-334 dokunulmaz).

## 1. Home (belge açık değil)

```
┌ yardımcı bar ───────────────────────────────────────────────┐
│                                                              │  boş — sakin
├──────────────────────────────────────────────────────────────┤
│                                                              │
│                                                              │
│                    ▢                                         │  28 px belge işareti
│                 Belge açın                                   │  h1, --text-heading / 600
│   PDF, DOCX, UDF veya görsel dosyanızı açın                  │  ≤44ch, ikincil (§8 dili)
│            ya da buraya sürükleyin.                          │
│                 [ Belge Aç ]                                 │  TEK birincil, ⌘O
│                                                              │
│                                                              │
│ ──────────────────────────────────────────────────────────── │  hairline
│ SON KULLANILANLAR                                            │  --text-label, ikincil renk
│   dava-dilekcesi.docx              ⟨DOCX⟩       12 dk önce   │  satır 40 px, sağda tür rozeti
│   ek-rapor.pdf                     ⟨PDF⟩        dün          │
│   sozlesme.udf                     ⟨UDF⟩        3 gün önce   │
│                                            Listeyi temizle   │  sessiz
└──────────────────────────────────────────────────────────────┘
```

- Mod başlığı yok; boş durumun "Belge açın" satırı ekranın h1'idir (A8: tek h1).
- Boş durum bloğu kalan alanın **%38**'inde (grid 38fr / auto / 62fr); son
  kullanılanlar aşağı bölgede, en fazla **5** satır (900×600'de sütun kaymaz).
  Son kullanılan yoksa liste hiç çizilmez. Liste yanında vurgu çizgisi yok.
- Kip türe göre metin (`modes.ts` tek kaynak): Düzenle → "PDF, DOCX, UDF veya
  görsel dosyanızı açın ya da buraya sürükleyin."; Dönüştür → "DOCX veya UDF
  dosyanızı açın ya da buraya sürükleyin."; Karşılaştır → "Karşılaştırmak için
  iki PDF, DOCX veya UDF dosyası açın ya da buraya sürükleyin."; Denetle →
  "DOCX veya UDF dosyanızı açın ya da buraya sürükleyin."
- Zaman: Rust `openedAt` **saniye** gönderir; sunum sınırında ms'ye çevrilir
  (bugün "1 Oca" gösteren gerçek kusur). Basamaklar: az önce / N dk önce /
  N sa önce / dün / N gün önce (≤6) / d MMM.
- Home'da **hiçbir kipte panel yok** (§10 "içerik yoksa hiç çizilmesin").
  Dönüştür'ün ÇIKTI paneli yalnız belge açıkken, ConvertWorkspace'in kendi
  durumundan çizilir — ilk kullanım kabulü atlanmaz, kabuk modül komutu çağırmaz.
- Belge bağlamı uyuşmazlığı (`mismatch` / `needsMore`) boş durumun **üstünde**
  tek satır `Status`.

### Kabul çerçevesi (§28)

Dark, 1280×800 (yeni varsayılan pencere), Düzenle Home, sentetik son
kullanılanlar fixture'ı (3 satır), panel **yok** — dürüst durum bu. Panel
ilişkisi ikinci karede gösterilir: Dönüştür, sentetik DOCX açık, ÇIKTI paneli
görünür. İkisi de paketlenmiş pencereden `screencapture -l <id>` ile alınır;
PNG'nin tek renk olmadığı piksel kontrolüyle doğrulanır (bkz. §8).

## 2. Düzenle (PDF açık)

```
┌ yardımcı bar ───────────────────────────────────────────────────────────────┐
│ ▢ sozlesme.pdf ⟨PDF⟩         [Klasör seçerek kaydet] [Yeni PDF kaydet] [▣] [Kapat]
├──────────────┬──────────────────────────────────────┬────────────────────────┤
│ sayfa şeridi │ [Seç][Sırala][Sil][Döndür]  ↶ ↷  Sığdır ▾│ ┌ BELGE ─────────────┐ │
│ ┌────┐       │ ✓ PDF kaydedildi … (Status, role=status)│ │ ▸ Birleştir        │ │
│ │ 1  │ ☑     │        ┌──────────────────┐          │ │   Sıkıştır          │ │
│ └────┘       │        │                  │          │ │   PDF → PNG/JPG     │ │
│ ┌────┐       │        │    PDF sayfası   │          │ │ ▸ Diğer (katlı)     │ │
│ │ 2  │ ☐     │        │                  │          │ │   Kırp · Filigran   │ │
│ └────┘       │        └──────────────────┘          │ │   Sayfa no · Görsel │ │
│ ┌────┐       │  sozlesme.pdf · s. 2 / 12 · İşaretli │ ├─────────────────────┤ │
│ │ 3  │ ☑     │                                      │ │ SEÇİLİ ARAÇ         │ │
│ └────┘       │                                      │ │ Seçili sayfalar     │ │
│ …            │                                      │ │ açıklama (1 cümle)  │ │
│              │                                      │ │ [araca özgü alan]   │ │
│              │                                      │ │ Belge ekle…  (merge/│ │
│              │                                      │ │  images) + kaynak ↑ │ │
│              │                                      │ │ 12 sayfa · 3 işaretli│ │
│              │                                      │ │ [Tümünü] [Temizle]  │ │
│              │                                      │ │ ☐ imza onayı (varsa)│ │
│              │                                      │ └─────────────────────┘ │
└──────────────┴──────────────────────────────────────┴────────────────────────┘
```

- Öncelik: PDF > sayfalar > araçlar > ayarlar. Belge alanı ortada ve en geniş.
- **Sayfalar** grubu (select/reorder/delete/rotate) günlük iştir → belge alanının
  üstündeki kompakt şeritte `aria-pressed` segment olarak durur; aynı `setKind`.
  Döndür kipinde ↶/↷ `IconButton` (etiketleri test-kilitli metinler), yakınlaştırma
  seçici. **Belge** ve katlı **Diğer** grupları panelde; `<details>` ve
  `open={group.keys.includes(kind)}` aynen. Üç grup adı ve on bir etiket hâlâ
  SSR çıktısında (sağlayıcısız satır içi çizim).
- Seçili aracın ayarı, `Belge ekle…` (mevcut `choose()`; Birleştir ve Görseller →
  PDF için tek yol) ve kaynak listesi (↑ sırala) panelin SEÇİLİ ARAÇ bölümünde.
- Kaydet eylemleri **yardımcı barda**; sonuç metni (`operation-status`,
  role=status) **workspace'te**, şeridin altında — panel kapalıyken de görünür.
- Sayfa şeridi (`thumbnail-list`) workspace'in sol kenarında ~132 px sütun.
- "PDF Araçları" başlığı ve açıklama paragrafı kalkar. `pdf-root` sınıfı ve
  motor çağrıları (`duzenek_*`) aynen; panel `scope="pdf-root"` ile çizilir.
- **Test yenilemesi (aynı commit'te):** `pdfWorkspace.test.tsx` "yerleşim
  sözleşmesi" bloğu yeni kompozisyonu tarif eder (grid sütunları, DOM sırası,
  sticky); B4 invoke kümesi, B6 geometri, A5 aria-label'lar, on bir etiket ve
  `<details>` kilitleri korunur. `pdf.css` token'lara bağlanır (radius ≤ 8,
  `--space-*`, `--control-h`, `.card` yok). `docs/DESIGN.md` migration
  dondurmasının kalktığını yazar.

## 3. Dönüştür

```
┌ yardımcı bar ─────────────────────────────────────────────────────────────┐
│ ▢ dilekce.docx ⟨DOCX⟩                                [Dönüştür] [▣] [Kapat]│
├───────────────────────────────────────────┬────────────────────────────────┤
│                                           │ ┌ ÇIKTI ─────────────────────┐ │
│   KAYNAK                                  │ │ /Users/…/Belgeler/Dönüşüm  │ │  tam yol, seçilebilir
│   dilekce.docx                            │ │ [Finder'da Göster]         │ │
│   DOCX · 184 KB                           │ │ [Değiştir…] Varsayılana dön│ │  sessiz, yalnız varsayılan değilse
│                                           │ └────────────────────────────┘ │
│              ↓                            │                                │
│                                           │                                │
│   HEDEF                                   │                                │
│   UYAP UDF                                │                                │
│                                           │                                │
│ ───────────────────────────────────────── │                                │
│ SONUÇ                                     │                                │
│ dilekce.docx → dilekce.udf                │                                │
│   Kaynak belge değiştirilmedi.            │                                │
│   Yaklaşık aktarıldı: tablo kenarlıkları  │                                │
└───────────────────────────────────────────┴────────────────────────────────┘
```

- Kaynak → Hedef sakin akış; form değil. Birden çok dosyada KAYNAK bir listedir;
  okunamayan dosya kendi satırında hata metniyle (mevcut davranış).
- "Dönüştür" birincil eylemi barda (`ToolbarActions`); onay uyarısı
  (`ConversionWarning`) ve ilk kullanım kabulü (`FirstUseAcceptance`) **aynen**.
- Çıktı klasörü panele taşınır (`outputFolder / setOutputFolder /
  revealOutputFolder` aynı). Sonuç tek yerde: SONUÇ, workspace'te.

## 4. Karşılaştır

```
┌ yardımcı bar ───────────────────────────────────────────────────────────────┐
│ ▢ taslak-v1.docx ↔ ▢ taslak-v2.docx                            [▣] [Kapat]  │
├──────────────────────┬──┬──────────────────────┬─────────────────────────────┤
│ TEMEL SÜRÜM          │⇄ │ DEĞİŞİK SÜRÜM        │ ┌ FARKLAR ────────────────┐ │
│                      │ 1│                      │ │ 24 değişiklik           │ │
│  paragraf…           │  │  paragraf…           │ │ + 8 ekleme      %33     │ │
│  ▒▒ silinen ▒▒       │ 2│  ▒▒ eklenen ▒▒       │ │ − 6 silme       %25     │ │
│                      │  │                      │ │ ~ 10 değişiklik %42     │ │
│                      │ 3│                      │ ├─────────────────────────┤ │
│                      │  │                      │ │ SEÇİLİ FARK             │ │
│                      │  │                      │ │ 2 · Değiştirilen · m.3  │ │
│                      │  │                      │ │ temel: …  değişik: …    │ │
│                      │  │                      │ ├─────────────────────────┤ │
│                      │  │                      │ │ [Tümü][Eklenen][Silinen]│ │
│                      │  │                      │ │ liste                   │ │
└──────────────────────┴──┴──────────────────────┴─────────────────────────────┘
```

- `compare-toolbar` kalkar: adlar barın bağlamı (kabuk), özet ve filtre panelde.
- Her kontrolün tek sahibi: değiştir (swap) yalnız ray başında; filtre yalnız
  panelde; kapat/gizle yalnız bardaki "Ayrıntılar".
- `ChangeInspector` **başsız** olur (kendi `<aside>`'ı, başlığı ve kapat düğmesi
  kalkar); şarta bağlı panel `InspectorPanel scope="compare-root"` içinde çizilir.
  Donut kalkar; §13 metin düzeni (sayı + yüzde) — `comparisonFlow.test.tsx`
  ve `swapSemantics.test.tsx` aynı commit'te yeni sözleşmeye güncellenir.
  Panelin iç kuralları (`compare.css :336-424`) sunumdur; 11/12/13 token'lara ve
  `--text-scale`'e bağlanır (6.5–9.5 px metin yok).
- Fark rayı (`ChangeRail`), satır eşitleme ve diff işaretleme **aynen**.

## 5. Denetle

```
┌ yardımcı bar ─────────────────────────────────────────────────────────────┐
│ ▢ dilekce.docx ⟨DOCX⟩                        [Kopyaya uygula…] [▣] [Kapat] │
├────────────────────────────────────────────┬───────────────────────────────┤
│ ✓ kopya.docx · 2 düzeltme uygulandı (Status)│ ┌ DENETİM ÖZETİ ───────────┐ │
│  BULGULAR                                  │ │ Kesin hata          4    │ │
│  ● KESİN HATA  Taraf adı tutarsız   12. p. │ │ Uyarı               7    │ │
│    Aynı taraf iki biçimde yazılmış.        │ │ İncele              3    │ │
│    ┃ …paragrafın tamamı, [işaretli] …     │ │ 48 paragraf · 2.140 kelime│ │
│    Neden: …                                │ │ Standart profil          │ │
│    ☐ Önerilen düzeltme: A → B              │ │ (kesilen kural notu)     │ │
│  ▲ UYARI       Madde numarası atlanmış 31. │ ├──────────────────────────┤ │
│  ○ İNCELE      …                           │ │ DÜZELTMELER              │ │
│                                            │ │ 2 / 5 seçildi            │ │
│                                            │ │ [Tümünü seç] [Kaldır]    │ │
│                                            │ │ Kopyaya yazılır; kaynak  │ │
│                                            │ │ değiştirilmez.           │ │
│                                            │ └──────────────────────────┘ │
└────────────────────────────────────────────┴───────────────────────────────┘
```

- Satır içi akordeon **korunur** (kayıtlı karar): bulgu, kanıtının yanında.
  Alıntı ±40 karakter yerine **paragrafın tamamını** gösterir (`blocks[]` zaten
  sonuçta); belge ortamda mevcut olur.
- Tek etiket kümesi: `SEVERITY` (Kesin hata / Uyarı / İncele) hem listede hem
  panelde. Kesilen kural notu panelde; yazma sonucu ve hata workspace'te `Status`.
- "Kopyaya uygula…" birincil eylem barda; seçim yardımcıları panelde.

## 6. Ayarlar (sheet)

```
┌──────────────────────────────────────────┐
│ Ayarlar                                  │
│ GÖRÜNÜM                                  │
│ Tema                       [Sistemle ▾]  │
│ Metin boyutu               [Normal   ▾]  │
│ ERİŞİLEBİLİRLİK                          │
│ Yüksek kontrast            [Sistemle ▾]  │
│   Çizgileri ve ikincil metni güçlendirir │
│ Hareketi azalt             [Sistemle ▾]  │
│   Geçişleri kaldırır                     │
│ HAKKINDA                                 │
│ Yüksekbaş Belge 0.0.1 — belge çalışma    │
│ ortamı. © 2026                           │
│                                  [Bitti] │
└──────────────────────────────────────────┘
```

- Yalnız mevcut dört ayar; iki ipucu da kalır; sürüm `appInfo.version`'dan.
- Sheet karakteri aynı (üstten iner, odak tuzağı, Escape, odak dönüşü).

## 7. Pencere boyutları

| Genişlik × yükseklik | Kenar çubuğu | Panel | Workspace |
|---|---|---|---|
| 1440×900 | 232 | açık (300 + 12) | kalan |
| 1280×800 (varsayılan) | 232 | açık | kalan |
| 960×620 | 232 | varsayılan kapalı; "Ayrıntılar" ile açılır | ≥ 716 |
| 900×600 (minimum) | 232 | varsayılan kapalı; "Ayrıntılar" ile açılır | ≥ 656 |

Metin ölçeği %150 ve %200'de aynı dört boyut tekrar alınır. Hiçbir boyutta
yatay kaydırma yok; uzun dosya adı barda kısaltılır; eylemler sarmaz.

## 8. Görsel kabul yöntemi

Statik harness yeterli değildir. Ekran görüntüsü **gerçek pencereden** alınır:

1. Pencere kimliği `CGWindowListCopyWindowInfo` ile bulunur (`qa/shots/capture.sh`).
2. `screencapture -x -o -l <id>` — pencere geçerli Space'te ve boyanmış olmalı;
   arka plandaki Space'te webview boş çıkar (bu turda görüldü).
3. PNG'nin tek renk olmadığı piksel kontrolüyle doğrulanır; boş kare sunulmaz.
4. Ekran kaydı izni yoksa açıkça yazılır; PASS uydurulmaz.

Geliştirme döngüsü `tauri dev` (gerçek WKWebView + HMR) ile yürür; tarayıcı
önizlemesi yalnız DOM/erişilebilirlik ağacı için kullanılır.
