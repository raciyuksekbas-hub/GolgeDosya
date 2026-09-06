# Ortak çekirdek crate'leri

Bu dizin Phase 3'e kadar boştur. Kaynaklar ve taşıma sırası:

| Crate | Kaynak | Faz | Not |
|---|---|---|---|
| `document-core` | Tavzih `crates/tavzih-core` | 3 | **Source of truth.** 13 dosya / 6.413 satır. Doğruluk kanıtı burada: 57 korpus + 68 birim testi. `convert.rs` bu crate'e girmez (bkz. MIGRATION.md açık soru 4). |
| `pdf-core` | DüzenEk `crates/ekler-core` PDF katmanı | 6 | `pdf/{mod,tolerant,validate}`, `toolbox`, `optimizer`, `raster`, `safe_io`, `stamp` — ~1.700 satır. DüzenEk domain'ine (`Project`, `LogicalExhibit`) dokunmayan kısım. |
| `office-bridge` | DüzenEk `ekler-core/office.rs` + `udf/mod.rs`, Değişikİş `legacy_doc.rs` | 6 | LibreOffice keşfi, `sandbox-exec`, `textutil`/Word COM. **document-core'a da pdf-core'a da gömülmez**: platform bağımlılıkları burada izole olur. |

## Neden Tavzih source of truth?

1. DüzenEk'in kendi `crates/tavzih-core/ORIGIN.md`'si Tavzih'i kaynak ilan
   ediyor ve *"The Tavzih application was not modified"* diyor.
2. 57 korpus testi ve `convert.rs`'in 12 testi yalnız Tavzih'te.
3. `docs/format-research/EXP-001…013` model kararlarının gerekçesi orada.
4. Tavzih çekirdeği çift yönlü kullanıyor; DüzenEk yalnız beş API noktasını:
   `docx::reader::probe`, `udf::reader::read_udf`, `docx::writer::write_docx`,
   `model`, `warnings`.
5. Vendored kopyada DüzenEk'e özgü **hiçbir uyarlama yok**.

2026-09-06 itibarıyla 13/13 dosya hâlâ byte-identical; tek fark `lib.rs`'in
modül listesi ve bağımsız `Cargo.toml`. Semantik drift **sıfır**.

## Değişmez

```
pdf-core  →  document-core     SERBEST
document-core  →  pdf-core     YASAK
```

`scripts/check-architecture.sh` her sürüm kapısında denetler.
