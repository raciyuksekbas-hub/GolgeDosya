/**
 * DüzenEk'in PDF çalışma alanının kullandığı tipler.
 *
 * Bağımsız uygulamanın `src/types.ts` dosyasından alındı; yalnız bu görünümün
 * ihtiyaç duyduğu alt küme. Alan adları Rust tarafındaki `ScanBatchResult` ve
 * `SourceFile` ile birebir aynıdır — serde adlandırması snake_case, o yüzden
 * burada da snake_case.
 */

export type SourceFormat =
  | "pdf"
  | "doc"
  | "docx"
  | "jpg"
  | "jpeg"
  | "png"
  | "tif"
  | "tiff"
  | "heic"
  | "udf"
  | "unknown";

export type SignedPolicy = "use_original_as_is" | "create_derived_copy";

export interface SourceFile {
  id: string;
  path: string;
  file_name: string;
  format: SourceFormat;
  size_bytes: number;
  sha256_before: string;
  mtime: number;
  page_count: number;
  is_signed: boolean;
  signature_note?: string;
  signed_policy: SignedPolicy;
  is_approved_for_conversion: boolean;
  is_repaired?: boolean;
  repair_note?: string;
}

export interface ScanFileError {
  path: string;
  reason: string;
}

export interface ScanBatchResult {
  sources: SourceFile[];
  errors: ScanFileError[];
}
