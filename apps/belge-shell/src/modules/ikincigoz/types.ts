// `ikincigoz-core` sözleşmesinin yansıması. Standalone uygulamadaki alan adları
// ve severity değerleri birebir korundu; sözleşme değişirse derleme kırılır.

export type Severity = "error" | "warning" | "review";

/**
 * Motorun GERÇEKTEN gönderdiği biçim: snake_case.
 *
 * Bu arayüz eskiden camelCase ilan ediyordu; hiçbir alan çalışma zamanında o
 * adla yoktu, yani tip yalan söylüyordu ve ona güvenen her okuma `undefined`
 * alıyordu (bir kez gerçek bir bulgu-anahtarı çakışmasına yol açtı). Rust
 * tarafında `SourceLocation` üzerinde `rename_all` YOKTUR — sözleşme budur.
 */
export interface SourceLocation {
  block_index: number;
  run_index: number | null;
  char_start: number | null;
  char_end: number | null;
  container_path: string | null;
}

export interface Fix {
  block_id: string;
  char_start: number;
  char_end: number;
  /** Aralığı şu anda tam olarak dolduran metin; doğrulama içindir. */
  original: string;
  replacement: string;
  description: string;
}

export interface Finding {
  rule_id: string;
  title: string;
  severity: Severity;
  confidence: number;
  /** Tam olarak ne görüldüğünü ve ne beklendiğini söyleyen tek cümle. */
  message: string;
  /** Motorun bunu neden sorun saydığı. "Neden böyle dedi?" sorusunu yanıtlar. */
  explanation: string;
  block_id: string;
  location: SourceLocation;
  context: string | null;
  fix: Fix | null;
}

export interface DocumentSummary {
  fileName: string;
  format: string;
  blockCount: number;
  wordCount: number;
  charCount: number;
}

export interface BlockView {
  id: string;
  kind: string;
  text: string;
  isHeading: boolean;
  level: number | null;
}

export interface TruncatedRuleView {
  ruleId: string;
  shown: number;
  total: number;
}

export interface AnalysisResult {
  document: DocumentSummary;
  blocks: BlockView[];
  findings: Finding[];
  errorCount: number;
  warningCount: number;
  reviewCount: number;
  truncatedRules: TruncatedRuleView[];
  elapsedMs: number;
  profileName: string | null;
}

export interface WriteResult {
  fileName: string;
  directory: string;
  applied: number;
}
