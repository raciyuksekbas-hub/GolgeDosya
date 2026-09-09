import { useEffect } from "react";

/**
 * macOS kısayolları.
 *
 * Yalnız sistemin zaten öğrettiği iki tanesi bağlanır: ⌘O ve ⌘,. Bunlar her
 * macOS uygulamasında aynı anlama gelir, dolayısıyla öğrenilmeleri gerekmez.
 * Onlarca kısayoldan oluşan bir katalog üretilmedi — keşfedilemeyen kısayol
 * profesyonel kullanıcıya da yardım etmez.
 *
 * Metin girişi sırasında devre dışı: bir alana yazarken ⌘O basmak belge
 * seçicisini açmamalı. Sheet açıkken de devre dışı — odak tuzağının içinden
 * arka plandaki eylemi tetiklemek odak modelini bozar.
 */
export interface Shortcut {
  /** Tuş, `event.key` değeriyle karşılaştırılır (küçük harfe çevrilir). */
  key: string;
  run: () => void;
  /** Kısayolun şu an geçerli olup olmadığı. */
  enabled?: boolean;
}

function inTextEntry(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable === true
  );
}

export function useShortcuts(shortcuts: Shortcut[], active = true) {
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      // Yalnız ⌘ (macOS) / Ctrl. Shift veya Alt eklenmiş kombinasyonlar bize ait değil.
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
      if (inTextEntry(e.target)) return;
      const hit = shortcuts.find(
        (s) => s.enabled !== false && s.key.toLowerCase() === e.key.toLowerCase(),
      );
      if (!hit) return;
      e.preventDefault();
      hit.run();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [shortcuts, active]);
}
