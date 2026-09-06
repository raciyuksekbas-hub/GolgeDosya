/**
 * Tema ve erişilebilirlik tercihlerinin belgeye uygulanması.
 *
 * Dört uygulamada dört ayrı çözüm vardı. Burada tek bir yer var, ama
 * *soyutlama değil*: yalnız `documentElement` üzerine data-* öznitelikleri
 * yazar. Görsel kararlar CSS'te kalır.
 *
 * Tercihler `settings.json`'da tutulur — localStorage'da DEĞİL. DüzenEk'in
 * temayı localStorage'da tutması, bundle kimliği değişince sessizce
 * kaybolmasına yol açıyordu; birleşik uygulama bu hatayı tekrarlamaz.
 */
import type { Settings } from "../shell/types";

type Tri = "system" | "on" | "off";

function resolveTri(value: Tri, query: string): boolean {
  if (value === "on") return true;
  if (value === "off") return false;
  return window.matchMedia(query).matches;
}

export function applyPreferences(s: Settings): () => void {
  const root = document.documentElement;
  const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
  const contrastQuery = window.matchMedia("(prefers-contrast: more)");
  const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");

  const apply = () => {
    const dark = s.theme === "dark" || (s.theme === "system" && darkQuery.matches);
    root.dataset.theme = dark ? "dark" : "light";
    root.dataset.contrast = resolveTri(s.highContrast, "(prefers-contrast: more)") ? "more" : "normal";
    root.dataset.motion = resolveTri(s.reduceMotion, "(prefers-reduced-motion: reduce)") ? "reduced" : "full";
    // 100–200 arası; dışındaki değer kullanıcıyı arayüzden kilitleyebilir.
    const scale = Math.min(200, Math.max(100, s.textScale || 100));
    root.style.setProperty("--text-scale", String(scale / 100));
  };

  apply();
  darkQuery.addEventListener("change", apply);
  contrastQuery.addEventListener("change", apply);
  motionQuery.addEventListener("change", apply);
  return () => {
    darkQuery.removeEventListener("change", apply);
    contrastQuery.removeEventListener("change", apply);
    motionQuery.removeEventListener("change", apply);
  };
}
