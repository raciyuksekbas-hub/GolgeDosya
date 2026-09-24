# Denetle — Türkçe korpus ölçümü (saha maddeleri 35, 36, 37)

Tarih: 2026-09-24 · Motor: `crates/ikincigoz-core` · Yöntem: bellekte kurulan
DOCX → gerçek `parser::parse` → `rules::analyze` (varsayılan seçenekler:
`min_confidence 0.55`, `include_review true`, `max_per_rule 50`). Model, ağ ya da
bulut yok. Korpus ve koşucu denetim sırasında depo dışında tutuldu; aşağıdaki
sayılar o koşucunun çıktısından, düzeltmelerden ÖNCE (HEAD 905a537) ve SONRA
(bu tur) alınmıştır.

## Beklenen / bulunan

| Kategori | Yerleştirilen | Bulunan (önce) | Bulunan (sonra) | Not |
|---|---:|---:|---:|---|
| Çift boşluk | 2 | 2 | 2 | Aralık doğru; arayüzde GÖRÜNMÜYORDU (madde 35, düzeltildi) |
| Uzun boşluk / sekme / bölünmez boşluk | 3 | 2 | 2 | Kısa ilk kelimeden sonraki sekme yapısal sayılır; baştaki/sondaki boşluk bilinçli muaf |
| Yinelenen noktalama | 5 | 4 | 4 | `, ,` kaçıyor |
| Büyük/küçük harf tutarlılığı | 3 | 0 | 0 | Kural yok |
| Terim tutarsızlığı (harf katlaması) | 2 grup | 2 | 2 | ÖNCE ikisi de DOĞRU yazımı işaretliyordu; SONRA yanlış yazımı işaretliyor, "sözleşme" için öneri var (madde 36) |
| Terim tutarsızlığı (birleşik/büyük harf: "İşveren / iş veren / İş Veren") | 1 grup | 0 | 0 | Kuralın modelinde değil (P2, ayrı ölçüm gerekir) |
| Yazım yanlışı (18 cümle) | 18 | 4 | 4 | Yalnız bitişik/ayrı yazım listesi (7 etkin kayıt); genel yazım denetimi YOK |
| Yinelenen kelime | 4 | 2 | 2 | `ve ve`, `bu bu`: 2 harfli kelimeler bilinçli muaf |
| Boş paragraf | 4 | 0 | 0 | Kural yok; `<w:p/>` blok bile olmuyor |
| Bozuk numaralandırma | 3 | 3 | 3 | Doğru yerde |

Toplam: yaklaşık 45 yerleştirilmiş kusur, 19 bulgu (17'si doğru yerde).
Temiz hukuk metni kontrolü (12 paragraf): 1 yanlış pozitif (`sınır` / `sinir`,
iki ayrı kelime). SONRA da 1: yalnız ı/i farkı olduğu için artık DÜZELTME
önerilmiyor; bulgu inceleme düzeyinde kalıyor.

Görünmeyen karakter (U+200B): bulunuyor; ÖNCE işaret 0 px genişliğindeydi,
SONRA `[U+200B]` olarak görünür ve ekran okuyucuya adıyla okunur.

## Beklenti farkı (madde 37)

Karşılama metni "Yazım, noktalama ve tutarlılık bulgularını…" diye
başlıyordu. Motor bilinçli olarak genel Türkçe yazım denetimi yapmaz
(`ortho.rs` başı; kendi açıklaması "genel bir yazım denetimi yapılmaz").
18 yaygın yazım yanlışından 4'ünü (hepsi bitişik/ayrı yazım) buluyor.
Avukat "yazım"ı imla denetimi olarak okuyor ve "çok az hata buluyor"
sonucuna varıyordu. Bu turda metin düzeltildi: "Boşluk, noktalama, tekrar,
numaralandırma ve tutarlılık sorunlarını tek listede görün." Bir test metni
davranışa bağlar (`tests/copy_promise.rs`): metin yeniden "yazım" derse motor
10 yaygın yazım yanlışının en az %80'ini bulmak zorundadır (bugün 0/10).

Yazım dışı ama motor kapsamındaki boşluklar da "az hata" hissine katkı
veriyor: büyük/küçük harf tutarlılığı ve boş paragraf kuralı yok, 2 harfli
kelime tekrarları muaf. Bunlar kesinlik öncelikli tasarım kararlarıdır;
değiştirilmedi.

## Yerel yazım denetimi seçenekleri (araştırma, uygulanmadı)

| Seçenek | Kapsam | Lisans / boyut | Durum |
|---|---|---|---|
| Küratörlü sözcük-dışı yanlış listesi (`yanlız→yalnız`, `herkez→herkes`, `malesef→maalesef`, `orjinal→orijinal`, `yalnış→yanlış`, `müracat→müracaat`, `tabiki→tabii ki`, `şöyleki→şöyle ki`, `pekçok→pek çok`, `sonuçda→sonuçta`) | Yalnız sözlükte karşılığı olmayan yazımlar, düzeltme önerili | Kod içi, ek bağımlılık yok | Önerilen ilk adım (P2). Listeyi kuran korpusla ölçülmemeli: ayrı, görülmemiş bir yanlış kümesiyle ölçülmeli. `bir çok` gibi iki kelimelik biçimler meşru okunuşları olduğu için alınmamalı |
| macOS `NSSpellChecker` (tr) | Genel | Sistem | Ölçüldü: 18'den 10'u, 16 temiz cümlede 3 yanlış pozitif (alan adı, hukuk terimleri). Sonuçlar platforma bağlı olur |
| Windows `ISpellChecker` (tr-TR) | Genel | Sistem (`windows` crate zaten bağımlılık) | tr-TR desteği ölçülmedi; Windows CI'da `IsSupported("tr-TR")` ile doğrulanmalı |
| Hunspell tr_TR + `spellbook` | Genel | MPL-2.0, sözlük ~38 MB | Kurulum boyutunu katlar, bildirim yükümlülüğü ekler; ölçülmedi |

Öneri: platform denetleyicileri ya da büyük sözlük ürün kararıdır (sonuçları
Windows ile macOS arasında farklılaştırır ve golden testleri belirsizleştirir).
Önce küratörlü liste, görülmemiş bir yanlış kümesiyle ölçülerek.

## Bu turda motorda değişen

* `TYPO_MULTIPLE_SPACES`: satır sonu (`<w:br/>`) içeren boşluk dizisi artık
  "çift boşluk" değil. Word'de orada satır sonu görünür; düzeltmesi yazılamıyor
  ve hep-ya-da-hiç yazımda seçilen bütün düzeltmeleri düşürüyordu.
* `CONSISTENCY_TERM_SPELLING`: başvuru biçimi Türkçe harf sayısıyla seçilir,
  her geçiş ayrı bulgu, ç/ğ/ö/ş/ü farkında öneri, ı/i farkında asla öneri yok,
  sıradan bağımsız.
