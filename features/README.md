# Modül crate'leri

Her modülün Rust motoru buraya gelir. Kabuk motorları doğrudan çağırmaz;
`features/<ad>` crate'i kendi Tauri komutlarını ve kendi capability dosyasını
getirir.

| Modül | UI adı | Kaynak | Faz | Rust motoru? |
|---|---|---|---|---|
| `tavzih` | Dönüştür | Tavzih | 3 | evet — `document-core` + `convert.rs` |
| `ikincigoz` | Denetle | İkinciGöz | 4 | evet — `ikincigoz-core` **olduğu gibi** |
| `degisikis` | Karşılaştır | Değişikİş | 5 | hayır — diff motoru TypeScript'te kalır |
| `duzenek` | Düzenle | DüzenEk | 6 | evet — `pdf-core` + `office-bridge` |

## İsimlendirme

Kullanıcı arayüzü **eylem adlarını** gösterir (Düzenle / Dönüştür / Karşılaştır /
Denetle). Kod, log, feature flag ve hata kayıtları **eski ürün adlarını** korur.
Bir kayıtta `duzenek` görmek, hangi kod tabanından geldiğini tartışmasız yapar —
migration boyunca bu ayrım kasıtlıdır.

## İkinciGöz — özel not

İkinciGöz'ün parser'ı Phase 4'te **birleştirilmez**. Kendi `ikincigoz-core`'u ile
taşınır. `writeback.rs` konteyneri yeniden serileştirmez, byte düzeyinde yamalar
ve bunu `container_path` provenance'ına dayandırır — Tavzih'in fidelity modelinde
bu alan yok. Parser konsolidasyonu Phase 7'nin araştırma konusudur ve başarısız
olması kabul edilebilir bir sonuçtur.
