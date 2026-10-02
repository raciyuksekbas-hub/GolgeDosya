/**
 * Kaydetme yeri — Düzenle, Ekler ve Denetle'nin ortak dili.
 *
 * Saha: "Kayıt klasörleri arasında birlik yok. Farklı araçlarda farklı
 * klasörlere kaydediliyor sanki." Pencereler başlangıç klasörü almıyordu;
 * işletim sistemi en son hatırladığı klasörü açıyordu.
 *
 * Tek kural: kaydetme ve klasör seçme penceresi KAYNAK belgenin klasöründe
 * açılır (`startFolder`); kaynak geçici bir yerdeyse işletim sisteminin
 * varsayılanı kalır. Karar arka uçtadır (`output_place.rs`).
 *
 * Dönüştür bu kuralın dışındadır: kendi çıktı klasörüne yazar (Ayarlar).
 */
import { invoke } from "@tauri-apps/api/core";
import { logFailure } from "./failure";

/**
 * Önerinin en fazla bekleneceği süre. Kaynak erişilemeyen bir ağ
 * paylaşımındaysa klasörü yoklamak uzun sürer; kullanıcı "Kaydet"e basıp
 * hiçbir şey olmadığını sanmasın, pencere varsayılan yerde açılsın.
 */
export const START_FOLDER_WAIT_MS = 1500;

/**
 * Kaydetme penceresinin açılacağı klasör: kaynağın klasörü ya da `null`
 * (işletim sisteminin varsayılanı). Hiçbir zaman reddetmez; öneri
 * alınamıyorsa ya da gecikirse pencere yine açılmalıdır.
 */
export async function startFolder(
  source: string | undefined,
  wait = START_FOLDER_WAIT_MS,
): Promise<string | null> {
  if (!source) return null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<null>((resolve) => {
    timer = setTimeout(() => resolve(null), wait);
  });
  try {
    const asked = invoke<string | null>("output_start_folder", { source }).then((f) => f ?? null);
    return await Promise.race([asked, late]);
  } catch (e) {
    logFailure("output start folder", e);
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/** Klasör + ad, klasörün kendi ayracıyla (Windows `\`, diğerleri `/`). */
export function inFolder(folder: string, name: string): string {
  const sep = folder.includes("\\") && !folder.includes("/") ? "\\" : "/";
  return folder.endsWith(sep) ? `${folder}${name}` : `${folder}${sep}${name}`;
}

/**
 * Pencerenin `defaultPath`'i: önerilen klasör varsa `<klasör>/<ad>`, yoksa
 * yalnız ad (işletim sistemi klasörü kendi seçer).
 */
export function dialogPath(folder: string | null, name: string): string {
  return folder ? inFolder(folder, name) : name;
}
