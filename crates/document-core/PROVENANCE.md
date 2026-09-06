# Kaynak kaydı

Bu crate, kullanıcının sahibi olduğu **Tavzih** deposundan commit
`7df3f5dd2775d30a6aec48f2cc84547583c0b396` durumuyla, `crates/tavzih-core`
yolundan alındı. 2026-09-07 itibarıyla 17 dosyanın tamamı byte-identical.

## Neden Tavzih source of truth

1. DüzenEk'in kendi vendored kopyasındaki `ORIGIN.md` Tavzih'i kaynak ilan
   ediyor ve *"The Tavzih application was not modified"* diyor. Yön zaten tek yönlü.
2. Doğruluk kanıtı burada: `tests/corpus.rs` 57 test + modül içi 68 birim testi.
   DüzenEk'in kopyasında bunların hiçbiri yoktu.
3. `docs/format-research/EXP-001…013` model kararlarının gerekçesi Tavzih'te.
4. Tavzih çekirdeği çift yönlü kullanıyor (reader **ve** writer); DüzenEk yalnız
   beş API noktasını: `docx::reader::probe`, `udf::reader::read_udf`,
   `docx::writer::write_docx`, `model`, `warnings`.
5. Vendored kopyada DüzenEk'e özgü hiçbir uyarlama yoktu — yalnız `convert.rs`
   ve testler çıkarılmıştı.

## Bu fazda değişmeyenler

Parser/writer davranışı, model, API yüzeyi, uyarı sözleşmesi, güvenlik limitleri.
Provenance alanı **eklenmedi**. Semantik refactor yapılmadı. Tek değişiklik
`Cargo.toml`'un bağımsızlaştırılmasıdır; çözülen bağımlılık sürümleri aynıdır.

## Lisans

Tavzih manifest'i MIT beyan ediyor. O checkout'ta bağımsız bir LICENSE dosyası
veya telif bildirimi yoktu; **dış dağıtımdan önce sahiplik ve yeniden dağıtım
izni belgelenmelidir.** Bu, birleşik uygulamanın yayın önkoşuludur.
