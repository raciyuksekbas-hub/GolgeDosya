/**
 * Dilekçe ekleri alan modeli.
 *
 * Alanlar Rust tarafındaki `ekler_core::model` ile BİREBİR aynıdır (serde
 * snake_case), çünkü `Project` doğrudan komuta geçer. Uydurma alan yoktur.
 *
 * Modelin özü üç kavramın AYRI olmasıdır:
 *   Mantıksal Ek  ≠  Kaynak Dosya  ≠  Fiziksel PDF
 * "Ek-3 — Banka Dekontları" birden fazla kaynak belge içerebilir; bir kaynağın
 * sayfaları nihai pakette belirli bir mantıksal eke aittir.
 */
export interface ExhibitSourceRef {
  source_id: string;
  /** Boşsa kaynağın tüm sayfaları. */
  page_range?: [number, number] | null;
}

export interface LogicalExhibit {
  id: string;
  order: number;
  name: string;
  sources: ExhibitSourceRef[];
}

/** `ekler_core::SourceFile` — burada yalnız arayüzün okuduğu alanlar. */
export interface AnnexSource {
  id: string;
  path: string;
  file_name: string;
  page_count: number;
  is_signed: boolean;
}

/**
 * `pdf_core::StampConfig` — motorda düz bir struct alanıdır (Option DEĞİL,
 * serde default YOK). `null` göndermek komut gövdesine girmeden reddedilir.
 */
export interface StampConfig {
  enabled: boolean;
  position: "top_right" | "top_left" | "bottom_right" | "bottom_left";
  font_size: number;
  margin_pt: number;
  show_badge: boolean;
}

export interface Project {
  version: string;
  name: string;
  created_at: string;
  updated_at: string;
  sources: AnnexSource[];
  exhibits: LogicalExhibit[];
  target_size_bytes: number;
  stamp_config: StampConfig;
  optimization?: string;
}
