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
  /** Karşılama ekranının başlığı — kipin GÖREVİ, "Belge açın" değil. */
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
    emptyTitle: "PDF üzerinde çalışın",
    extensions: ["pdf", "docx", "doc", "udf", "jpg", "jpeg", "png", "tiff", "heic"],
    hint: "Sayfaları düzenlemek, döndürmek, sıralamak veya dışa aktarmak için bir PDF açın.",
    openLabel: "PDF Aç",
    // Liste PDF dışı türleri de taşır (Düzenle onları da açar), bu yüzden
    // başlık "Son PDF'ler" olamaz: etiketin listeyle uyuşması, sloganın
    // kulağa hoş gelmesinden önemli.
    recentTitle: "Son Belgeler",
    pickerLabel: "Belge veya görsel",
    needs: 1,
  },
  tavzih: {
    label: "Dönüştür",
    emptyTitle: "Belge dönüştürün",
    extensions: ["docx", "udf"],
    hint: "Word ve UYAP biçimleri arasında: DOCX ⇄ UDF.",
    openLabel: "Belge Aç",
    recentTitle: "Son Belgeler",
    pickerLabel: "Word veya UYAP belgesi",
    needs: 1,
  },
  degisikis: {
    label: "Karşılaştır",
    emptyTitle: "İki belgeyi karşılaştırın",
    extensions: ["pdf", "docx", "doc", "udf"],
    hint: "İlk belgeyi seçin, ardından karşılaştıracağınız ikinci belgeyi ekleyin.",
    openLabel: "İlk Belgeyi Seç",
    recentTitle: "Son Belgeler",
    pickerLabel: "Karşılaştırılacak belgeler",
    needs: 2,
    slots: ["Belge A", "Belge B"],
  },
  ikincigoz: {
    label: "Denetle",
    emptyTitle: "Belgeyi denetleyin",
    extensions: ["docx", "udf"],
    hint: "Göndermeden önce son okuma: DOCX veya UDF dosyanızı açın.",
    openLabel: "Belge Aç",
    recentTitle: "Son Belgeler",
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
