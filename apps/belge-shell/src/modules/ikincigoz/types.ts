// `ikincigoz-core` sözleşmesinin yansıması. Standalone uygulamadaki alan adları
// ve severity değerleri birebir korundu; sözleşme değişirse derleme kırılır.

export type Severity = "error" | "warning" | "review";

export interface SourceLocation {
  blockIndex: number;
  runIndex: number | null;
  charStart: number | null;
  charEnd: number | null;
  containerPath: string | null;
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
