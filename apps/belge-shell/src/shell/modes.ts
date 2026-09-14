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

import type { FeatureState } from "./types";

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
  /** Son kullanılanlar bölümünün başlığı — kipin diliyle. */
  recentTitle: string;
  /** Dosya seçicide görünecek tür adı. */
  pickerLabel: string;
  /** Kaç belge gerekir: bir kip iki belge isteyebilir (Karşılaştır). */
  needs: 1 | 2;
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
    emptyTitle: "Sayfalar ve dışa aktarma",
    extensions: ["pdf", "docx", "doc", "udf", "jpg", "jpeg", "png", "tiff", "heic"],
    // İkinci cümle bir nezaket değil zorunluluk: liste DOCX ve UDF satırları
    // gösteriyor, motor da onları gerçekten açıyor. "Yalnız PDF" demek
    // verilmeyen bir söz olurdu.
    hint:
      "Bir PDF açın; sayfaları sıralayın, döndürün ya da dışa aktarın. " +
      "Word ve UYAP belgeleri açılırken PDF'ye dönüştürülür.",
    openLabel: "PDF Aç",
    // Liste PDF dışı türleri de taşır (Düzenle onları da açar), bu yüzden
    // başlık "Son PDF'ler" olamaz: etiketin listeyle uyuşması, sloganın
    // kulağa hoş gelmesinden önemli.
    recentTitle: "Son belgeler",
    pickerLabel: "Belge veya görsel",
    needs: 1,
  },
  tavzih: {
    label: "Dönüştür",
    emptyTitle: "Word ve UYAP arasında",
    extensions: ["docx", "udf"],
    hint: "DOCX ve UDF biçimleri arasında dönüştürün; kaynak belgeniz olduğu gibi kalır.",
    openLabel: "Belge Aç",
    recentTitle: "Son belgeler",
    pickerLabel: "Word veya UYAP belgesi",
    needs: 1,
  },
  degisikis: {
    label: "Karşılaştır",
    emptyTitle: "İki sürüm arasındaki farklar",
    extensions: ["pdf", "docx", "doc", "udf"],
    hint: "İki belge seçin; eklenen, silinen ve değişen bölümler yan yana gösterilir.",
    openLabel: "İlk Belgeyi Seç",
    recentTitle: "Son belgeler",
    pickerLabel: "Karşılaştırılacak belgeler",
    needs: 2,
    slots: ["Belge A", "Belge B"],
    slotInvites: ["İlk belgeyi seç", "İkinci belgeyi seç"],
  },
  ikincigoz: {
    label: "Denetle",
    emptyTitle: "Göndermeden önce son okuma",
    extensions: ["docx", "udf"],
    hint: "Bir DOCX veya UDF açın; yazım, noktalama ve tutarlılık bulgularını inceleyin.",
    openLabel: "Belge Aç",
    recentTitle: "Son belgeler",
    pickerLabel: "Word veya UYAP belgesi",
    needs: 1,
  },
};

export function extensionOf(path: string): string {
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function fileNameOf(path: string): string {
  return path.split("/").pop() || path;
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
