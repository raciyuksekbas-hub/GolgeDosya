/**
 * Denetle — düzeltilmiş kopyanın kaydetme penceresi.
 *
 * Pencere öneri almıyordu: ad yoktu, klasör yoktu, filtre yoktu. İşletim
 * sistemi en son hatırladığı klasörü açıyordu ve kullanıcı adı elle
 * yazıyordu; "dava.pdf" yazarsa DOCX baytları `.pdf` uzantısıyla
 * kaydediliyordu.
 *
 * Artık pencere belgenin klasöründe (bkz. `shared-ui/savePlace`), motorun
 * kendi önerdiği adla (`ikincigoz_core::writeback::output_file_name`:
 * "<ad> - Düzeltilmiş.<uzantı>") ve belgenin türüne süzülmüş açılır.
 * Kullanıcının yazdığı ad yine olduğu gibi motora gider (öneri yalnız
 * öneridir); ad çakışırsa motor " (2)" türetir ve sonuç gerçek adı söyler.
 */
import type { SaveDialogOptions } from "@tauri-apps/plugin-dialog";
import { dialogPath } from "../../shared-ui/savePlace";
import { extensionOf, fileNameOf } from "../../shell/modes";

/** Denetle'nin açtığı türler ve pencerede görünen adları. */
const FORMAT_NAME: Record<string, string> = {
  docx: "Word belgesi",
  udf: "UYAP belgesi",
};

/** Motorun önerdiği ad: "<ad> - Düzeltilmiş.<uzantı>". Kaynak adı asla. */
export function correctedName(source: string): string {
  const name = fileNameOf(source);
  const dot = name.lastIndexOf(".");
  return dot > 0
    ? `${name.slice(0, dot)} - Düzeltilmiş${name.slice(dot)}`
    : `${name} - Düzeltilmiş`;
}

/** Kaydetme penceresinin seçenekleri. `folder`: `startFolder`'ın sonucu. */
export function correctedSaveOptions(source: string, folder: string | null): SaveDialogOptions {
  const ext = extensionOf(source);
  return {
    title: "Düzeltilmiş kopyayı kaydet",
    defaultPath: dialogPath(folder, correctedName(source)),
    filters: ext ? [{ name: FORMAT_NAME[ext] ?? ext.toUpperCase(), extensions: [ext] }] : undefined,
  };
}
