/**
 * Hataların kullanıcı karşılığı — açma, kaydetme ve dışa aktarma.
 *
 * Motorun hata metni geliştirici içindir: JavaScript'in `Error:` öneki,
 * `Doğrulama hatası (Validation Failed)` gibi sınıf etiketleri, io ayrıntıları
 * ve dosya yolları taşır. Bunların hiçbiri kullanıcı yüzeyine çıkmaz.
 *
 * Burada motorun BİLDİRDİĞİ kategori tanınır ve kısa, eyleme dönük tek cümleye
 * çevrilir. Motorun söylemediği bir sebep uydurulmaz; tanınmayan hata için
 * güvenli yedek cümle döner. Teknik metin kaybolmaz, `console.debug`'da kalır.
 *
 * Aynı ilke iki yolda da geçerlidir: belge açılırken de kopya yazılırken de
 * kullanıcı motorun cümlesini değil, kategorisinin tek cümlesini görür.
 *
 * Motor sözleşmesi değişmedi: eşleme yalnız sunum sınırında yapılır.
 */

export const OPEN_FAILURE_TITLE = "Belge açılamadı";
export const OPEN_FAILURE_FALLBACK = "Farklı bir dosya seçip yeniden deneyin.";
export const SAVE_FAILURE_FALLBACK = "Kopya oluşturulamadı. Yeniden deneyin.";

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
 * Kayıt için metni zararsızlaştırır.
 *
 * Konsol da bir yüzeydir: paketlenmiş üründe açılabilir ve içeriği dışarı
 * taşınabilir. Dosya yolu ve belge adı müvekkil bilgisidir — hata ayıklama
 * için gereken kategori bilgisi onlarsız da okunur.
 */
const DOC_EXT = "pdf|docx?|udf|jpe?g|png|tiff?|heic|json|xml";

export function redact(text: string): string {
  return (
    text
      // Yollar önce: içlerinde belge adı da var.
      .replace(/(?:~|\.{1,2})?\/[^\s"'`,;)\]]+/g, "‹yol›")
      // Motor belge adını tırnak içinde verir; boşluklu adlar da bütün gider.
      .replace(new RegExp(`'[^']*\\.(?:${DOC_EXT})'`, "gi"), "'‹belge›'")
      .replace(new RegExp(`"[^"]*\\.(?:${DOC_EXT})"`, "gi"), '"‹belge›"')
      .replace(new RegExp(`[^\\s/\\\\"'\`]+\\.(?:${DOC_EXT})\\b`, "gi"), "‹belge›")
  );
}

/** Üretim derlemesinde konsola giden tek satır. Ham motor metni DEĞİLDİR. */
export function failureLogLine(scope: string, raw: unknown): string {
  return `[belge] ${scope}: ${redact(plainMessage(raw)).slice(0, 200)}`;
}

/** Geliştirme derlemesi mi? Vite dışında (test koşucusu, statik çizim) hayır. */
function isDev(): boolean {
  try {
    return Boolean((import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV);
  } catch {
    return false;
  }
}

/**
 * Hata kaydı — tek kapı.
 *
 * Geliştirmede ham hata nesnesi olduğu gibi durur; orada yığın izi ve yol
 * gerçekten gerekir. Üretimde yalnız redakte edilmiş tek satır yazılır:
 * kategoriyi görürsünüz, müvekkilin dosya adını görmezsiniz.
 */
export function logFailure(scope: string, raw: unknown): void {
  if (isDev()) {
    console.debug(`[belge] ${scope}`, raw);
    return;
  }
  console.debug(failureLogLine(scope, raw));
}

/**
 * Metin hâlâ geliştirici gibi mi konuşuyor?
 *
 * Bazı motorlar kullanıcı cümlesini kendisi üretir (İkinciGöz'ün `message_tr`'si,
 * Tavzih'in `AppError.message`'ı). O cümleleri kategoriye indirmek bilgi
 * kaybettirir. Bu yollarda metin olduğu gibi taşınır — ama yalnız temizse.
 */
const TECHNICAL =
  /(^|\s)\/[\w.\-/]+|os error|\bpanic\b|\bunwrap\b|\.rs\b|[a-z_]{2,}::[a-z_]{2,}|\bError\b|Failed\b|\bcommand\b|\bundefined\b|Validation/i;

/**
 * Motorun kendi kullanıcı cümlesini taşıyan yollar için güvenli geçiş.
 *
 * Sınıf öneki ve sarmalayıcı etiket düşer; geriye teknik görünen bir şey
 * kalırsa (dosya yolu, io kodu, sınıf adı) kullanıcıya o değil, verilen yedek
 * cümle gider. Ham metin çağıranın `console.debug`'unda kalır.
 */
export function safeMessage(raw: unknown, fallback: string): string {
  const text = plainMessage(raw);
  return text.length === 0 || TECHNICAL.test(text) ? fallback : text;
}

/**
 * Kaydetme ve dışa aktarma kategorileri.
 *
 * Bu yoldaki hatalar güvenlik denetimlerini de taşır — kaynak bütünlüğü, sayfa
 * sayımı, boyut sınırı. Bu bilgi kullanıcıya ulaşmalı, ama sınıf adlarıyla ve
 * dosya yollarıyla değil. En özgül kategori önce gelir.
 */
const SAVE_CATEGORIES: { match: RegExp; detail: string }[] = [
  {
    match: /bütünlük|integrity|tarama sırasında değişti/i,
    detail: "Kaynak belge işlem sırasında değişti; kopya oluşturulmadı. Belgeyi yeniden açıp deneyin.",
  },
  {
    match: /sayfa hesaplama|page accounting/i,
    detail: "Çıktının sayfa sayısı beklenenle uyuşmadı; kopya kaydedilmedi.",
  },
  {
    match: /boyut\w* sınırı|size limit/i,
    detail: "Çıktı dosyası boyut sınırını aştı; kopya kaydedilmedi.",
  },
  {
    match: /onayı verilmedi|unapproved/i,
    detail: "İmzalı belge için onay verilmedi. Onay kutusunu işaretleyip yeniden deneyin.",
  },
  {
    match: /yeniden doğrulanamadı/i,
    detail: "Kaydedilen kopya yeniden açılıp doğrulanamadı. Yeniden deneyin.",
  },
  {
    match: /bozuk veya geçersiz|geçersiz pdf|geçersiz udf|geçersiz görsel/i,
    detail: "Belge okunamadı; kopya oluşturulamadı. Farklı bir kopya deneyin.",
  },
  {
    match: /permission|izin|read-?only|salt okunur/i,
    detail: "Bu klasöre yazılamadı. Yazma izni olan bir klasör seçip yeniden deneyin.",
  },
  {
    match: /no space|disk full|yeterli (disk )?alan/i,
    detail: "Diskte yeterli yer yok. Yer açıp yeniden deneyin.",
  },
  {
    match: /doğrulama hatası|validation failed/i,
    detail: "Çıktı doğrulanamadı; kopya kaydedilmedi. Yeniden deneyin.",
  },
  {
    match: /okunamadı|erişilemedi|yazılamadı|os error|io error|bulunamadı/i,
    detail: "Dosya yazılamadı. Klasörü kontrol edip yeniden deneyin.",
  },
];

/** Kaydetme/dışa aktarma hatasının kullanıcı cümlesi. Teknik metin DÖNMEZ. */
export function describeSaveFailure(raw: unknown): string {
  const text = raw instanceof Error ? raw.message : String(raw);
  return SAVE_CATEGORIES.find((c) => c.match.test(text))?.detail ?? SAVE_FAILURE_FALLBACK;
}

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
