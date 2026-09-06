// `crates/document-core/src/convert.rs` içindeki sürümlü sözleşmenin yansıması.
// Tek yerde tutulur ki sözleşme değişince arayüz değil derleme kırılsın.
// Bağımsız Tavzih'in `src/types.ts` dosyasından birebir alındı.

export type Direction = "docx_to_udf" | "udf_to_docx";
export type Status = "success" | "warning" | "failure";
export type Severity = "INFO" | "APPROXIMATION" | "LOSS";

export interface Warning {
  code: string;
  severity: Severity;
  title: string;
  location: string | null;
  detail: string | null;
  data_loss: boolean;
}

export interface ErrorInfo {
  code: string;
  message: string;
  detail: string;
  source_untouched: boolean;
}

export interface ConversionResult {
  contract_version: number;
  status: Status;
  source: string;
  source_name: string;
  output: string | null;
  output_name: string | null;
  direction: Direction;
  source_format: string;
  target_format: string;
  warnings: Warning[];
  error: ErrorInfo | null;
  source_sha256_before: string;
  source_sha256_after: string;
  source_unchanged: boolean;
  elapsed_ms: number;
}

export interface FileInfo {
  path: string;
  name: string;
  size_bytes: number;
  size_label: string;
  direction: Direction;
  source_format: string;
  target_format: string;
  planned_output_name: string;
  output_would_collide: boolean;
}

export interface AppError {
  code: string;
  message: string;
  detail: string;
}

export interface BatchResult {
  contract_version: number;
  status: Status;
  total: number;
  succeeded: number;
  warned: number;
  failed: number;
  items: ConversionResult[];
  output: string | null;
  output_name: string | null;
  is_zip: boolean;
  all_sources_unchanged: boolean;
  elapsed_ms: number;
}

export interface InspectOutcome {
  path: string;
  info: FileInfo | null;
  error: AppError | null;
}

export interface OutputFolder {
  path: string;
  default_path: string;
  is_default: boolean;
}
