import type { FeatureState } from "./types";

/**
 * Rota çözümü.
 *
 * Kabuk tek pencerede çalışır ve dört sabit rotası vardır; router kütüphanesi
 * eklemek taşınacak dört arayüzün hiçbirinin bugün ihtiyaç duymadığı bir
 * bağımlılık olurdu.
 */

/** Etkin olmayan bir rotaya gidilemez: kapatılan modül erişilemez olmalı. */
export function resolveRoute(requested: string, features: FeatureState[]): string {
  const match = features.find((f) => f.route === requested);
  return match && match.enabled ? requested : "";
}

/** Açılışta gösterilecek bölüm: kenar çubuğu sırasındaki ilk kullanılabilir olan. */
export function firstAvailableRoute(features: FeatureState[]): string {
  return features.find((f) => f.enabled)?.route ?? "";
}

export function featureForRoute(
  route: string,
  features: FeatureState[],
): FeatureState | undefined {
  return features.find((f) => f.route === route && f.enabled);
}
