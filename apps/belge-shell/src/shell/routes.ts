import type { FeatureState } from "./types";

/**
 * Rota çözümü.
 *
 * Kabuk tek pencerede çalışır ve gerçek bir router'a ihtiyaç duymaz; dört
 * sabit rota var. Router kütüphanesi eklemek, taşınacak dört arayüzün
 * hiçbirinin bugün ihtiyaç duymadığı bir bağımlılık olurdu.
 */
export const DEFAULT_ROUTE = "/";

/** Etkin olmayan bir rotaya gidilemez: kapatılan modül erişilemez olmalı. */
export function resolveRoute(requested: string, features: FeatureState[]): string {
  const match = features.find((f) => f.route === requested);
  if (match && match.enabled) return requested;
  return DEFAULT_ROUTE;
}

export function featureForRoute(route: string, features: FeatureState[]): FeatureState | undefined {
  return features.find((f) => f.route === route && f.enabled);
}
