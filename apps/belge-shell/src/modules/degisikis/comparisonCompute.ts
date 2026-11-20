import { compareDocuments } from "./core/compare";
import type { ComparisonResult, DocumentBlock } from "./core/types";
import { buildComparisonViewModel, type ComparisonViewModel } from "./viewModels/comparisonViewModel";

/**
 * Karşılaştırmanın kendisi: worker'da da eşzamanlı yedekte de koşan tek kod.
 *
 * Ayrı dosyadadır, çünkü worker yalnız bunu içe aktarmalı: worker'ı kuran
 * `comparisonJob.ts` worker'ın içine girerse paketleyici worker'ı kendi
 * içinde yeniden paketlemeye çalışır.
 */

export interface ComparisonOutcome {
  comparison: ComparisonResult;
  model: ComparisonViewModel;
}

export interface ComparisonRequest {
  base: DocumentBlock[];
  revised: DocumentBlock[];
}

export type ComparisonReply =
  | { ok: true; outcome: ComparisonOutcome }
  | { ok: false; message: string };

/** Worker'ın ve eşzamanlı yedeğin koştuğu tek iş. */
export function computeComparison(base: DocumentBlock[], revised: DocumentBlock[]): ComparisonOutcome {
  const comparison = compareDocuments(base, revised);
  return { comparison, model: buildComparisonViewModel(comparison) };
}

/** Worker içinde iş: hata da yanıttır, worker sessizce ölmez. */
export function answerComparison(request: ComparisonRequest): ComparisonReply {
  try {
    return { ok: true, outcome: computeComparison(request.base, request.revised) };
  } catch (error) {
    return { ok: false, message: error instanceof Error ? error.message : String(error) };
  }
}
