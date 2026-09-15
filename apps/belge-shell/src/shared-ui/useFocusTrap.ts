/**
 * Modal odak tuzağı.
 *
 * İkinciGöz ve Değişikİş'te bu hiç yoktu; DüzenEk ve MetinBul'da ayrı ayrı
 * yazılmıştı. Tek uygulama, dört modülün en iyisini alır.
 */
import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

export function useFocusTrap(ref: RefObject<HTMLElement | null>, active: boolean, onEscape?: () => void) {
  // `onEscape` çağıran tarafta satır içi bir ok fonksiyonu olarak veriliyor
  // (App.tsx: `onClose={() => setPrefsTab(null)}`), yani HER RENDER'DA yeni bir
  // kimlik. Efektin bağımlılığı olsaydı tuzak her render'da sökülüp yeniden
  // kuruluyor ve `items()[0].focus()` odağı İLK sekmeye fırlatıyordu: klavye
  // kullanıcısı arka arkaya iki ayar değiştiremiyordu. Kimliği değil, güncel
  // değeri taşı.
  const escape = useRef(onEscape);
  escape.current = onEscape;
  useEffect(() => {
    if (!active || !ref.current) return;
    const container = ref.current;
    const previous = document.activeElement as HTMLElement | null;

    const items = () =>
      Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
        (el) => el.offsetParent !== null || el === document.activeElement,
      );

    items()[0]?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && escape.current) {
        event.preventDefault();
        escape.current();
        return;
      }
      if (event.key !== "Tab") return;
      const list = items();
      if (list.length === 0) return;
      const first = list[0];
      const last = list[list.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    container.addEventListener("keydown", onKeyDown);
    return () => {
      container.removeEventListener("keydown", onKeyDown);
      // Odak, modalı açan öğeye geri döner. Aksi hâlde klavye kullanıcısı
      // sayfanın başına düşer.
      previous?.focus?.();
    };
  }, [ref, active]);
}
