# PDF sıkıştırma — saha araştırması ve noktasal düzeltme (2026-09-21)

Giriş noktası bir saha belirtisiydi: 42 sayfalık, 9603,8 KB'lık bir PDF'e
Sıkıştır "Bu belge zaten yeterince optimize. Anlamlı bir küçülme sağlanamadı.
Çıktı kaydedilmedi." diyordu (en iyi aday 9399,8 KB, %2,12). Soru: dosya mı
gerçekten optimize, yoksa motor mu fırsat kaçırıyor?

Gerçek dosya yalnız yerel kanıt olarak kullanıldı; repoya girmedi, içeriği
loglanmadı. Aşağıdaki bütün rakamlar yapısal ölçümdür.

## Ölçüm özeti

| | |
|---|---|
| Boyut dağılımı | görseller %96,2 · içerik akışları %3,3 · sözlük/xref %0,5 · gömülü font ~0 |
| Görseller | 42 (sayfa başına bir tarama), ~150 DPI, hepsi JPEG, hepsi 4:2:0 |
| Yapı | tek revizyon, yinelenen görsel yok, orphan yok |
| Köken | dosyanın kendisi bir GölgeDosya **Ekler çıktısı**; Ekler hattı onu `target_size_bytes` altına zaten sıkıştırmıştı |
| Bağımsız oracle | qpdf yapısal (kayıpsız) %0,57 · görsel-farkında q72/4:2:0 + yapısal **%2,99** |

Sonuç: dosya gerçekten sınıra yakın; motorun kaçırdığı pay ölçülen 0,87
puan. Kullanıcıyı asıl rahatsız eden, bulunan 204 KB'ın tek ölçütlü %3
eşiğinde sessizce atılması ve bunun "zaten optimize" diye sunulmasıydı.
Aynı motor sahadaki uygun dosyalarda %28–%81 küçültüyor.

## Bu turda düzeltilen

**D2 — görsel süzgeç zinciri.** `decode_image` yalnız tek başına `DCTDecode`'u
ya da tamamı `FlateDecode` olan zinciri tanıyordu; `[/FlateDecode /DCTDecode]`
taşıyan 14 görsel (dosyanın %42'si) motora görünmezdi. Zincir artık ISO
32000-1 §7.4 sırasıyla çözülür: her taşıma katmanı kendi `/DecodeParms`'ı ile,
sondaki görsel kodeki kalan baytlarla. Yapısı bozuk zincir belgeyi düşürmez,
yalnız o görsel atlanır. Gerçek dosyada çözülebilen görsel 27 → 41; o 14
görsel de sınırda olduğu için bayt sonucu değişmedi (oracle da ~1 KB
öngörmüştü). Testler: `crates/pdf-core/tests/image_filter_chain.rs`.

**Eşik politikası.** Eski tek ölçüt (%3) yerine eskisinin üst kümesi: en az %3,
**ya da** en az %2 ve en az 100 KB, **ya da** en az 500 KB. Kullanıcının önerdiği
"%2 ve 100 KB ya da 500 KB" tek başına eski kuraldan DAHA KATIDIR (1 MB'ın %5'i,
51 KB, eskiden kaydediliyordu); bu yüzden %3 kolu korundu. Gerçek corpus'ta
(7 PDF) tek değişen sonuç saha dosyasıdır: NoBenefit → yayınlandı.
Testler: `crates/ekler-core/tests/compression_policy.rs`.

**Kullanıcı cümlesi.** "Bu belge zaten yeterince optimize." kaldırıldı. Yerine
gerçek sonuç: "204 KB (%2,1) küçültülebildi. Bu değer anlamlı küçülme eşiğinin
altında kaldığı için çıktı oluşturulmadı." ve alt bilgi "Kaynak · Olası çıktı".
"N görsel … renk uzayı veya maskesi nedeniyle yeniden kodlanamadı" cümlesi de
kaldırıldı: sonuç, görselin NEDEN yeniden kodlanmadığını taşımaz; sahadaki
bir Ekler çıktısında görsellerin üçü de çözülebilirdi, yalnız küçülmüyordu.

**"Daha önce işlenmiş" bağlamı.** Kaynak GölgeDosya'nın kendi marka çizimini
(canonical ya da birleşme sonrası eski çizim, bayt bayt) taşıyorsa ve kazanç
yoksa: "Bu belge GölgeDosya tarafından daha önce işlenmiş. Ek küçülme sınırlı
olabilir." Tanıma 56 yabancı PDF'te sıfır yanlış pozitif verdi. Sıkıştırmadan
ÖNCE gösterilmez: sahadaki başka bir Ekler çıktısı yine de %38 küçüldü.

## Ertelenen (backlog)

### PDF JPEG ENCODER QUALITY/SUBSAMPLING (D1)

Yeniden kodlama, `image` crate'inin `JpegEncoder`'ı ile yapılıyor; bu kodlayıcı
RGB'yi **4:4:4** yazar ve alt örnekleme için genel API sunmaz.

| | Kaynak (tarayıcı) | Motor, q65 |
|---|---|---|
| Renk alt örneklemesi | 4:2:0 | 4:4:4 |
| Luma kuantizasyon toplamı | 2952 | 2583 |
| Aynı görselin boyutu | 276.661 B | 309.344 B |

Motorun "en agresif" ön ayarı taramanın kendisinden daha İNCE kuantize ediyor;
bu yüzden 22 büyük tarama görseli yeniden kodlanınca BÜYÜYOR ve eleniyor.
Aynı pikseller 4:2:0 + optimize Huffman ile 277.064 B. Bu dosyada bedeli
~35 KB; asıl etkisi, zaten sıkıştırılmış taramaların hiç küçültülemeyişi.
Düzeltme bir kodlayıcı değişimi gerektirir (yeni crate / FFI); bilinçli olarak
bu turun dışında bırakıldı.

### Object stream çıktısı (D3)

`lopdf` klasik yapıda yazar; qpdf'in `object_stream_mode=generate` ile aldığı
kayıpsız %0,57 masada kalıyor.

### Yinelenen kaynak birleştirme (D4)

Aynı görsel/font/form'un tekrar gömülmesi birleştirilmiyor. Saha dosyasında
yinelenen yoktu; bedeli ölçülemedi.

## Değişmeyen, bilinen davranış

GölgeDosya ilk kez markaladığı sayfanın altına bir marka bandı ekler ve
görünür kutuyu genişletir (`MediaBox` alt kenarı aşağı iner, eşleşen
`CropBox`). Kaynak zaten markalıysa eklenmez; saha dosyasında kutular birebir
kaldı. Bu tur markalamaya dokunmadı.
