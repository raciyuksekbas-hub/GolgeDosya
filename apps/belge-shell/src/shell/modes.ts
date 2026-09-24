/**
 * Çalışma kipleri.
 *
 * GölgeDosya dört ayrı uygulama değil, **tek belge çalışma ortamının dört
 * kipidir**. Eski ürün adları (Tavzih, DüzenEk, Değişikİş, İkinciGöz) teknik
 * katmanda `FeatureState.key` olarak yaşar; kullanıcı yalnız eylemi görür.
 *
 * Kabul edilen belge türleri burada tek yerde tanımlıdır: hem belge yüzeyi hem
 * de kipler arası geçiş aynı tabloyu okur. Ayrı tutulsalardı, bir kipe
 * geçildiğinde belgenin sessizce düşmesi kaçınılmaz olurdu.
 */

import type { FeatureState, RecentDocument } from "./types";

export interface Mode {
  /** Kullanıcının gördüğü eylem adı. */
  label: string;
  /**
   * Karşılama başlığı — kipin GÖREVİ.
   *
   * Emir kipi değil: "PDF üzerinde çalışın" bir çalışma notu gibi okunuyordu.
   * Başlık işin ADINI söyler, kullanıcıya ne yapacağını buyurmaz; ne
   * yapılacağı açıklamada, nasıl başlanacağı düğmededir.
   */
  emptyTitle: string;
  /** Bu kipin açabildiği uzantılar. */
  extensions: string[];
  /** Karşılama açıklaması: ne yapılacağı ve hangi türlerle. */
  hint: string;
  /** Karşılama ekranındaki birincil eylemin adı. */
  openLabel: string;
  /**
   * Son belgeler bölümünün başlığı.
   *
   * Aynı kavramın ürün içinde TEK adı vardır: tercihler penceresi de bu
   * listeyi "Son belgeler" diye anar. İki ad, iki kavram demektir.
   */
  recentTitle: string;
  /** Dosya seçicide görünecek tür adı. */
  pickerLabel: string;
  /**
   * Kaç belge gerekir: bir kip iki belge isteyebilir (Karşılaştır).
   *
   * `0` = kip kabuğun belge akışına girmez; kendi seçicisiyle çalışır
   * (Ekler: bir ek paketi birden çok kaynaktan kurulur).
   */
  needs: 0 | 1 | 2;
  /**
   * İki belge isteyen kipte yuvaların adı.
   *
   * Karşılaştır'ın boş durumu diğerlerinin metin varyantı değildir: iki
   * belgelik zihinsel model ekranda durur. Tek belgelik kiplerde yoktur.
   */
  slots?: [string, string];
  /** Yuvaların boş hâlindeki davet metni. Etiket ad, bu satır eylemdir. */
  slotInvites?: [string, string];
}

/**
 * Kipin kabul ettiği türlerin kullanıcıya gösterilecek yazımı.
 *
 * Kayıt yokken karşılama ekranı bu satırı taşır: "hiçbir şey yok" demek
 * yerine ekran ne kabul ettiğini söyler (§12). Uzantı listesi tek kaynaktan
 * türetilir; ikinci bir tablo tutulmaz.
 */
export function formatList(mode: Mode, limit = 5): string {
  return mode.extensions
    .slice(0, limit)
    .map((e) => e.toLocaleUpperCase("tr-TR"))
    .join(" · ");
}

export const MODES: Record<FeatureState["key"], Mode> = {
  duzenek: {
    label: "Düzenle",
    emptyTitle: "PDF çalışma alanı",
    // YALNIZ PDF. Tarayıcı DOCX/UDF/görseli de kabul eder, ama kabuğun PDF
    // çalışma alanı önizlemeyi ve her aracı HAM yol üzerinde çalıştırır:
    // bir DOCX verildiğinde `inspect_pdf` düşer, önizleme düşer, kaydetme
    // düşer. Bağımsız DüzenEk'in dönüşüm adımı (`duzenek_convert_office_pdf`)
    // kabukta bağlı değildir. Kip açamayacağı türü kabul etmez; görseller
    // "Görseller → PDF" aracının kendi seçicisinden, Word/UYAP belgeleri
    // Dönüştür kipinden girer. İşlevsel denetim: pdf_workspace_audit.rs.
    extensions: ["pdf"],
    hint: "Sayfaları sıralayın, döndürün, çıkarın ya da yeni bir kopyaya aktarın.",
    openLabel: "PDF Aç",
    recentTitle: "Son PDF'ler",
    pickerLabel: "PDF belgesi",
    needs: 1,
  },
  // Dilekçe EKLERİ. Kendi belge seçicisi vardır (bir ek paketi birden çok
  // kaynaktan kurulur), bu yüzden kabuğun tek-belge akışına girmez: kip
  // açıldığında doğrudan çalışma alanı gelir.
  ekler: {
    label: "Ekler",
    // Saha (madde 6): "Ekler'in ne yaptığını anlamam için çok fazla süre
    // geçti." Bu metin tanımlıydı ama Ekler kabuğun karşılama yüzeyini
    // kullanmadığı için HİÇ çizilmiyordu; ekranın amacı söylenmiyordu.
    // Başlık işin sonucunu adlandırır (tasarım kuralı: başlık emir değil),
    // ne yapılacağını açıklama söyler.
    emptyTitle: "Dilekçe ekleri, tek pakette",
    extensions: ["pdf", "docx", "doc", "jpg", "jpeg", "png", "tif", "tiff"],
    hint: "Belgeleri yükleyin, Ek-1, Ek-2… gruplarına ayırın ve düzenli bir çıktı klasörü hazırlayın.",
    openLabel: "Belge Ekle",
    recentTitle: "Son belgeler",
    pickerLabel: "Ek belgesi",
    needs: 0,
  },
  tavzih: {
    label: "Dönüştür",
    emptyTitle: "Word ve UYAP arasında",
    extensions: ["docx", "udf"],
    hint: "DOCX ve UDF biçimleri arasında dönüştürün. Kaynak belgeniz değişmez.",
    openLabel: "Belge Aç",
    recentTitle: "Son belgeler",
    pickerLabel: "Word veya UYAP belgesi",
    needs: 1,
  },
  degisikis: {
    label: "Karşılaştır",
    emptyTitle: "İki sürüm arasındaki farklar",
    extensions: ["pdf", "docx", "doc", "udf"],
    // Ürün dili her yerde aynı: "Temel sürüm" / "Değişik sürüm". Eskiden boş
    // durumda "Belge A / Belge B" deniyordu; paneller, rapor ve ray zaten
    // sürüm dilini kullanıyordu — kullanıcı aynı iki belgeye iki ayrı ad
    // altında bakıyordu.
    hint: "Temel sürüm ile değişik sürümü seçin; eklenen, silinen ve değişen bölümler yan yana gösterilir.",
    openLabel: "İlk Belgeyi Seç",
    recentTitle: "Son belgeler",
    pickerLabel: "Karşılaştırılacak belgeler",
    needs: 2,
    slots: ["Temel sürüm", "Değişik sürüm"],
    slotInvites: ["Temel sürümü seç", "Değişik sürümü seç"],
  },
  ikincigoz: {
    label: "Denetle",
    emptyTitle: "Göndermeden önce son okuma",
    extensions: ["docx", "udf"],
    // Saha (madde 37): "Yazım" ile başlıyordu; motor genel yazım denetimi
    // YAPMAZ (ortho.rs: bilinçli olarak yalnız birkaç bitişik/ayrı yazım
    // kuralı). Avukat "yazım"ı imla denetimi olarak okuyor ve "çok az hata
    // buluyor" sonucuna varıyordu. Metin, motorun gerçekten yaptığını söyler.
    // Ölçüm: docs/audit/2026-09-24-denetle-olcum.md
    hint: "Boşluk, noktalama, tekrar, numaralandırma ve tutarlılık sorunlarını tek listede görün.",
    openLabel: "Belge Aç",
    recentTitle: "Son belgeler",
    pickerLabel: "Word veya UYAP belgesi",
    needs: 1,
  },
};

export function extensionOf(path: string): string {
  const name = fileNameOf(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/**
 * Yolun son parçası — her iki ayraçla.
 *
 * Yalnız `/` ile bölünüyordu. Windows yolları `\` kullanır: son belgeler
 * listesi, bardaki belge adı ve hata mesajları kullanıcının adını taşıyan TAM
 * yolu gösteriyordu (`C:\Users\Çağrı Şahin\…`). Sondaki ayraç (klasör yolu)
 * boş parça üretmez.
 */
export function fileNameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() || path;
}

/**
 * Ekranda gösterilecek son belgeler. "Son belgeleri göster" kapalıysa boş:
 * liste bölümü ve "Tümünü Temizle" kendiliğinden kaybolur. Alanı taşımayan
 * eski bir arka uç (undefined) gösterir.
 */
export function visibleRecents(settings: { showRecents?: boolean; recentDocuments: RecentDocument[] }): RecentDocument[] {
  return settings.showRecents === false ? [] : settings.recentDocuments;
}

/** Bu kip, açık olan belgelerle çalışabilir mi? */
export function modeAccepts(key: FeatureState["key"], paths: string[]): boolean {
  if (paths.length === 0) return false;
  const mode = MODES[key];
  return paths.every((p) => mode.extensions.includes(extensionOf(p)));
}

/**
 * Kip değiştirildiğinde belge bağlamına ne olur?
 *
 * Ürünün en önemli davranışlarından biri: kullanıcı bir belgeyi bir kez açar ve
 * kipler arasında gezerken onu **yeniden seçmek zorunda kalmaz**. Bağlam ancak
 * yeni kip o türü gerçekten açamıyorsa düşer — ve o zaman da sessizce değil,
 * `mismatch` ile açıkça anlatılır.
 */
export type ContextOutcome =
  | { kind: "keep"; paths: string[] }
  | { kind: "needsMore"; paths: string[]; missing: number }
  | { kind: "mismatch"; paths: string[]; rejected: string[] };

export function carryContext(
  key: FeatureState["key"],
  paths: string[],
): ContextOutcome {
  const mode = MODES[key];
  if (paths.length === 0) return { kind: "keep", paths: [] };

  const rejected = paths.filter((p) => !mode.extensions.includes(extensionOf(p)));
  if (rejected.length > 0) return { kind: "mismatch", paths, rejected };

  if (paths.length < mode.needs) {
    return { kind: "needsMore", paths, missing: mode.needs - paths.length };
  }
  // Bir belge isteyen bir kipe iki belgeyle gelinirse ilki kullanılır; ikincisi
  // atılmaz, kullanıcı geri döndüğünde yine oradadır.
  return { kind: "keep", paths: paths.slice(0, Math.max(mode.needs, 1)) };
}
