# DüzenEk birleşmesi — hangi yetenek taşındı, hangisi taşınmadı

**Tarih:** 2026-09-20 · **Bağlam:** gerçek hukuk belgeleriyle saha turu

Bu not bir soruyu kesin cevaplar: *DüzenEk gerçekten taşındı mı?*

## Kısa cevap

Önceki turlarda "DüzenEk taşındı" iddiası **yalnız PDF araçları üzerinden**
kuruluyordu. Bu eksik bir iddiaydı. DüzenEk'i asıl DüzenEk yapan
**dilekçe ekleri yönetimi** taşınmamıştı.

Bu turda taşındı. Özellik matrisi artık ikisini **ayrı** gösterir.

## Bulunan gerçek durum

Motor zaten bu depodaydı. `crates/ekler-core` (*ekler* = annexes) tam alan
modelini taşıyor ve 21 Tauri komutu kayıtlıydı; React tarafı bunların yalnız
**4'ünü** çağırıyordu. Eksik olan Rust değil, arayüzdü.

Ayrıca: ek oluşturma, atama ve otomatik dağıtım bağımsız uygulamada da **Rust
değildi** — TypeScript durum dönüşümleriydi. Rust yalnız bitmiş `Project`'i
plan/dışa aktarma anında alır. Bu yüzden bu turda yeni Rust yazılmadı;
semantik taşındı.

## Taşınan alan modeli

Korunan ayrım:

```
Mantıksal Ek  ≠  Kaynak Dosya  ≠  Fiziksel PDF
```

"Ek-3 — Banka Dekontları" birden fazla kaynak belge içerebilir; bir kaynağın
sayfaları nihai pakette belirli bir mantıksal eke aittir.

| Kavram | Tip | Yer |
|---|---|---|
| Mantıksal ek | `LogicalExhibit` | `crates/ekler-core/src/model.rs` |
| Ek ↔ kaynak bağı | `ExhibitSourceRef` | aynı |
| Kaynak dosya | `SourceFile` | aynı |
| Fiziksel çıktı | `PhysicalOutput` | aynı |
| Toplayıcı kök | `Project` | aynı |
| Dışa aktarma planı | `ExportPlan` | `plan.rs` |

Arayüz tarafındaki saf dönüşümler: `apps/belge-shell/src/modules/duzenek/ekler/annexState.ts`.

## Taşınan kullanıcı akışları

- Yüklenen Belgeler: ad, sayfa sayısı, atanma durumu
- Dilekçe Ekleri: oluştur, adlandır, sırala, sil
- Atama: sürükle-bırak **ve** seçim kutusu (klavye/ekran okuyucu yolu)
- Otomatik Dağıt: yalnız atanmamışları dağıtır, elle yapılan atamalara dokunmaz
- Ekler Listesini Kopyala
- Ekleri Hazırla ve Kaydet → `duzenek_prepare_uyap`

## Bilinçli olarak TAŞINMAYANLAR

Bunlar eksiklik değil, karardır:

- **Proje kaydetme/yükleme** (`duzenek_save_project_to_file`,
  `duzenek_load_project_from_file`, `duzenek_relink_source`). Komutlar
  derlenmiş durumda ama UI'ye bağlı değil. GölgeDosya'nın bugünkü modeli
  "oturum" değil "iş"; kalıcı proje dosyası ayrı bir ürün kararıdır.
- **Plan önizleme ve bölme incelemesi** (`duzenek_prepare_export_plan`,
  `duzenek_execute_export_plan`, `duzenek_split_into_new_exhibit`). Nihai iş
  tek adımda `duzenek_prepare_uyap` üzerinden yürüyor; boyut sınırı aşıldığında
  motor bölmeyi kendisi yapar. Kullanıcıya bölme kararını gösteren ara ekran
  taşınmadı.
- **Sayfa aralığı ile atama** (`ExhibitSourceRef.page_range`). Model
  destekliyor; arayüz bir kaynağın tamamını atar. Bir kaynağın sayfalarını iki
  ayrı eke bölmek bu sürümde yapılamaz.
- **Office/UDF dönüşümü** Düzenle içinden (`duzenek_convert_office_pdf`).
  Bilinçli: dönüşüm Dönüştür kipinin işidir (bkz. `shell/modes.ts`).

## Özellik matrisi

`apps/belge-shell/src-tauri/src/features.rs` artık beş satır taşır ve
`ekler` kendi satırıdır. Aynı cargo bayrağıyla (`feature_duzenek`) gelir —
motor aynıdır — ama **ayrı bir yetenektir**. Kip sayısını sabit yazan testler
matristen okuyacak biçimde değiştirildi; Rust matrisi ile kabuk bir daha ayrı
düşemez.
