import { invoke } from "@tauri-apps/api/core";
import type { BatchResult, ConversionResult, InspectOutcome, OutputFolder } from "./types";

export const inspectFiles = (paths: string[]) =>
  invoke<InspectOutcome[]>("tavzih_inspect_files", { paths });
export const convertFile = (path: string) =>
  invoke<ConversionResult>("tavzih_convert_file", { path });
export const convertBatch = (paths: string[], stamp: string) =>
  invoke<BatchResult>("tavzih_convert_batch", { paths, stamp });
export const termsAccepted = () => invoke<boolean>("tavzih_terms_accepted");
export const acceptTerms = () => invoke<void>("tavzih_accept_terms");
export const outputFolder = () => invoke<OutputFolder>("tavzih_output_folder");
export const setOutputFolder = (path: string | null) =>
  invoke<OutputFolder>("tavzih_set_output_folder", { path });
/**
 * Çıktıyı klasöründe gösterir. `outputs`: az önce üretilen dosyalar; boşsa
 * çıktı klasörünün kendisi açılır (bkz. `revealTargets`).
 */
export const revealOutputFolder = (outputs: string[]) =>
  invoke<void>("tavzih_reveal_output_folder", { outputs });

/**
 * Yerel `YYYY-MM-DD_HHMM` damgası.
 *
 * Motorda saat dilimi veritabanı yoktur; UTC bir dosya adı kullanıcıya yanlış
 * saati gösterirdi. Bu yüzden damga pencerede üretilip motora verilir.
 */
export function localStamp(now = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return (
    `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}` +
    `_${p(now.getHours())}${p(now.getMinutes())}`
  );
}
