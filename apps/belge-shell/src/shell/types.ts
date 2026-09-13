/** Rust `features::FeatureState` ile birebir eşleşir. */
export interface FeatureState {
  /** Kararlı anahtar; eski ürün adı. Log/flag/hata kayıtlarında bu kullanılır. */
  key: "duzenek" | "tavzih" | "degisikis" | "ikincigoz";
  /** Kullanıcının gördüğü Türkçe eylem adı. */
  label: string;
  route: string;
  /** Motor bu derlemeye linklendi mi? */
  compiled: boolean;
  /** Çalışma zamanında açık mı? */
  enabled: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  nameIsProvisional: boolean;
  configDir: string;
}

export interface RecentDocument {
  path: string;
  openedAt: number;
}

export interface Settings {
  theme: "system" | "light" | "dark";
  textScale: number;
  highContrast: "system" | "on" | "off";
  reduceMotion: "system" | "on" | "off";
  /** Eski anahtar; reduceMotion "system" iken karar buna düşer. */
  respectReducedMotion: boolean;
  acceptedTerms: number | null;
  outputDir: string | null;
  rendererPath: string | null;
  disabledRules: string[];
  includeReview: boolean;
  sourceReadOnly: boolean;
  linearResults: boolean;
  recentDocuments: RecentDocument[];
  migratedFrom: string[];
}

export type SourceStatus =
  | { migrated: { fields: string[] } }
  | "nothingToDo"
  | "notFound"
  | { unreadable: { detail: string } }
  | { needsManualStep: { detail: string } };

export interface SourceReport {
  app: string;
  identifier: string;
  path: string;
  status: SourceStatus;
}

export interface MigrationReport {
  changed: boolean;
  sources: SourceReport[];
}

/** Tercihler penceresinin sekmeleri. Kenar çubuğu doğrudan birini açar. */
export type PrefTab = "gorunum" | "erisim" | "hakkinda" | "telif" | "geri";
