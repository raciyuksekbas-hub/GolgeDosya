/**
 * Kaydetme yeri — Düzenle, Ekler ve Denetle'nin ortak dili.
 *
 * Saha: "Kayıt klasörleri arasında birlik yok. Farklı araçlarda farklı
 * klasörlere kaydediliyor sanki." Pencereler başlangıç klasörü almıyordu;
 * işletim sistemi en son hatırladığı klasörü açıyordu. Sonuç ekranları da
 * yeri farklı söylüyordu: Düzenle yalnız tam yolu, Denetle yalnız dosya adını
 * yazıyordu; "Klasörde Göster" yalnız Dönüştür'de vardı.
 *
 * Tek kural:
 *  * Kaydetme ve klasör seçme penceresi KAYNAK belgenin klasöründe açılır
 *    (`startFolder`); kaynak geçici bir yerdeyse işletim sisteminin
 *    varsayılanı kalır. Karar arka uçtadır (`output_place.rs`).
 *  * Kayıttan sonra klasörün adı görünür, tam yol ipucundadır ve tek eylem
 *    "Klasörde Göster"dir (`SavedPlace`).
 *
 * Dönüştür bu kuralın dışındadır: kendi çıktı klasörüne yazar (Ayarlar).
 * Karşılaştır da dışındadır: raporu pencere açmadan İndirilenler'e yazar
 * (`degisikis.rs` report_directory). Ayarlar'daki ipucu bunu adıyla söyler.
 */
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./primitives";
import { logFailure, safeMessage } from "./failure";
import { fileNameOf } from "../shell/modes";

/** Her araçta aynı ad. Platforma özgü dosya yöneticisi adı geçmez. */
export const SHOW_IN_FOLDER = "Klasörde Göster";

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

/** Yolun klasör kısmı (her iki ayraçla). Klasör yoksa boş. */
export function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut > 0 ? path.slice(0, cut) : cut === 0 ? path.slice(0, 1) : "";
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

/** Az önce kaydedilen dosyayı (ya da paket klasörünü) gösterir. */
export const revealSaved = (path: string) => invoke<void>("output_reveal", { path });

/**
 * "Klasörde Göster"in kendisi. Başarıda boş, başarısızlıkta kullanıcı
 * cümlesi döner: açılamayan klasör sessiz kalmaz.
 */
export async function showInFolder(
  path: string,
  onReveal: (path: string) => Promise<void> = revealSaved,
): Promise<string> {
  try {
    await onReveal(path);
    return "";
  } catch (e) {
    logFailure("output reveal", e);
    return safeMessage(e, "Klasör açılamadı. Dosya taşınmış veya silinmiş olabilir.");
  }
}

/**
 * Kaydın yeri: "Klasör  Dava Dosyaları  [Klasörde Göster]".
 *
 * `path` kaydedilen DOSYAdır; `isFolder` ise kaydedilen şeyin kendisi bir
 * paket klasörüdür (görsel paketi) ve gösterilen ad onun adıdır. Klasör
 * otomatik açılmaz (WCAG 3.2.5); yalnız istenince.
 */
export function SavedPlace({ path, isFolder = false, onReveal = revealSaved }: {
  path: string;
  isFolder?: boolean;
  onReveal?: (path: string) => Promise<void>;
}) {
  const [error, setError] = useState("");
  const dir = isFolder ? path : folderOf(path);
  const reveal = async () => {
    setError("");
    setError(await showInFolder(path, onReveal));
  };
  return (
    <>
      <p className="saved-place">
        <span className="saved-place-label">Klasör</span>
        <span className="saved-place-name" title={dir}>{fileNameOf(dir)}</span>
        <Button className="btn-sm" variant="quiet" onClick={reveal}>
          {SHOW_IN_FOLDER}
        </Button>
      </p>
      {error ? <p className="saved-place-error" role="alert">{error}</p> : null}
    </>
  );
}
