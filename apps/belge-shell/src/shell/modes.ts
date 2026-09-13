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
  /** Kipin ne yaptığı — tek cümle, başlık altında. */
  purpose: string;
  /** Bu kipin açabildiği uzantılar. */
  extensions: string[];
  /** Boş durumun açıklaması — bu kipin gerçekten açtığı türler (§8). */
  hint: string;
  /** Dosya seçicide görünecek tür adı. */
  pickerLabel: string;
  /** Kaç belge gerekir: bir kip iki belge isteyebilir (Karşılaştır). */
  needs: 1 | 2;
}

export const MODES: Record<FeatureState["key"], Mode> = {
  duzenek: {
    label: "Düzenle",
    purpose: "Sayfaları düzenleyin, yeni bir kopya oluşturun",
    extensions: ["pdf", "docx", "doc", "udf", "jpg", "jpeg", "png", "tiff", "heic"],
    hint: "PDF, DOCX, UDF veya görsel dosyanızı açın ya da buraya sürükleyin.",
    pickerLabel: "Belge veya görsel",
    needs: 1,
  },
  tavzih: {
    label: "Dönüştür",
    purpose: "Word ve UYAP biçimleri arasında",
    extensions: ["docx", "udf"],
    hint: "DOCX veya UDF dosyanızı açın ya da buraya sürükleyin.",
    pickerLabel: "Word veya UYAP belgesi",
    needs: 1,
  },
  degisikis: {
    label: "Karşılaştır",
    purpose: "İki belge arasındaki değişiklikler",
    extensions: ["pdf", "docx", "doc", "udf"],
    hint: "Karşılaştırmak için iki PDF, DOCX veya UDF dosyası açın ya da buraya sürükleyin.",
    pickerLabel: "Karşılaştırılacak belgeler",
    needs: 2,
  },
  ikincigoz: {
    label: "Denetle",
    purpose: "Göndermeden önce son okuma",
    extensions: ["docx", "udf"],
    hint: "DOCX veya UDF dosyanızı açın ya da buraya sürükleyin.",
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
