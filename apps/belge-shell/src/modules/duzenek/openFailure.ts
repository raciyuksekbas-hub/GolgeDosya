/**
 * Açma hatalarının kullanıcı karşılığı.
 *
 * Motorun hata metni geliştirici içindir: JavaScript'in `Error:` öneki,
 * `Doğrulama hatası (Validation Failed)` gibi sınıf etiketleri, io ayrıntıları
 * ve dosya yolları taşır. Bunların hiçbiri kullanıcı yüzeyine çıkmaz.
 *
 * Burada motorun BİLDİRDİĞİ kategori tanınır ve kısa, eyleme dönük tek cümleye
 * çevrilir. Motorun söylemediği bir sebep uydurulmaz; tanınmayan hata için
 * güvenli yedek cümle döner. Teknik metin kaybolmaz, `console.debug`'da kalır.
 *
 * Motor sözleşmesi değişmedi: eşleme yalnız sunum sınırında yapılır.
 */

export const OPEN_FAILURE_TITLE = "Belge açılamadı";
export const OPEN_FAILURE_FALLBACK = "Farklı bir dosya seçip yeniden deneyin.";

/**
 * Teknik önek ve sarmalayıcı etiketleri düşürür.
 *
 * Motorun kendi cümlesini koruması gereken yerlerde (kaydetme sonucu gibi)
 * kullanılır: bilgi kaybetmeden yalnız sınıf adlarını temizler.
 */
export function plainMessage(raw: unknown): string {
  const text = raw instanceof Error ? raw.message : String(raw);
  return text
    .replace(/^(?:[A-Za-z_$][\w$]*Error|Error):[ \t]*/gm, "")
    .replace(/^Doğrulama hatası \(Validation Failed\):[ \t]*/gm, "")
    .trim();
}

/**
 * Bilinen kategoriler. Sıra önemlidir: özgül olan önce gelir, çünkü eksik
 * dönüştürücü motorda bir doğrulama hatası olarak sarılır ve genel doğrulama
 * cümlesi o durumu yanlış anlatırdı.
 */
const CATEGORIES: { match: RegExp; detail: string }[] = [
  {
    match: /libreoffice|dönüşümü için .{0,40}bulunamadı/i,
    detail:
      "Bu belge türünü açmak için gereken dönüştürücü bu bilgisayarda bulunamadı. PDF dosyası seçerek devam edebilirsiniz.",
  },
  { match: /bir klasördür/i, detail: "Bu bir klasör. Tek bir belge seçin." },
  {
    match: /bulunamadı veya erişilemez|girdi dosyası bulunamadı/i,
    detail: "Dosya bulunamadı. Taşınmış, adı değişmiş veya silinmiş olabilir; yeniden seçin.",
  },
  {
    match: /desteklenmeyen dosya biçimi/i,
    detail: "Bu dosya biçimi burada açılamıyor. PDF, Word ya da görsel dosyası seçin.",
  },
  {
    match: /pdf dosyası|geçersiz pdf/i,
    detail: "Bu PDF okunamadı. Dosya bozulmuş olabilir; farklı bir kopya deneyin.",
  },
  {
    match: /udf/i,
    detail: "Bu UDF dosyası okunamadı. Dosya bozulmuş olabilir; farklı bir kopya deneyin.",
  },
  {
    match: /görsel/i,
    detail: "Bu görsel okunamadı. Dosya bozulmuş olabilir; farklı bir kopya deneyin.",
  },
  {
    match: /tarama sırasında değişti/i,
    detail: "Belge okunurken değişti. Dosyayı kapatıp yeniden seçin.",
  },
  {
    match: /doğrulama hatası|validation failed/i,
    detail: "Dosya doğrulanamadı. Farklı bir PDF seçin veya dosyayı yeniden oluşturup deneyin.",
  },
  {
    match: /okunamadı|erişilemedi|sağlama toplamı hesaplanamadı|dosya bilgileri/i,
    detail: "Dosya okunamadı. Erişim izni olan bir konumdan seçmeyi deneyin.",
  },
];

/**
 * Ham hatanın kullanıcıya söylenecek tek cümlesi. Teknik metin DÖNMEZ.
 *
 * Eşleme ham metin üzerinden yapılır: `Doğrulama hatası (Validation Failed)`
 * gibi sarmalayıcı etiketin kendisi de bir kategori işaretidir, önce silinirse
 * hata tanınamaz ve yedeğe düşerdi.
 */
export function describeOpenFailure(raw: unknown): string {
  const text = raw instanceof Error ? raw.message : String(raw);
  return CATEGORIES.find((c) => c.match.test(text))?.detail ?? OPEN_FAILURE_FALLBACK;
}
