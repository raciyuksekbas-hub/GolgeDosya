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
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Settings } from "../shell/types";

/**
 * Yerel pencere çerçevesinin teması.
 *
 * Windows'ta başlık çubuğunu işletim sistemi çizer ve temayı web içeriğinden
 * öğrenmez: açık bir Windows'ta "Koyu" seçilince koyu uygulamanın üstünde açık
 * bir başlık çubuğu kalıyordu. "Sistem" seçiliyken çerçeve sistemi izler.
 * Tauri dışında (testler, statik çizim) sessizce hiçbir şey yapmaz.
 */
export async function syncWindowTheme(theme: Settings["theme"]): Promise<void> {
  try {
    await getCurrentWindow().setTheme(theme === "dark" ? "dark" : theme === "light" ? "light" : null);
  } catch {
    // Pencere yok (tarayıcı, test) ya da izin yok: içerik teması yine uygulanır.
  }
}

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
    // İkinciGöz'ün çözüm kuralı birebir korundu: `reduceMotion` "system"
    // dışındaysa o kazanır; "system" ise karar eski `respectReducedMotion`
    // anahtarına düşer. `false` + "system", "sistem azaltma dese bile
    // animasyonları göster" demektir ve başka hiçbir alanla ifade edilemez.
    const motion =
      s.reduceMotion !== "system"
        ? s.reduceMotion === "on"
        : s.respectReducedMotion
          ? motionQuery.matches
          : false;
    root.dataset.motion = motion ? "reduced" : "full";
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
