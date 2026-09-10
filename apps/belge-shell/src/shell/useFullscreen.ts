import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Pencere tam ekranda mı?
 *
 * Örtüşük başlık çubuğunda kenar çubuğunun üst bandı trafik ışıklarına
 * ayrılmıştır; tam ekranda ışıklar çekilir ve bant ölü alan olur. Yalnız
 * okuma: `core:window:default` içindeki is-fullscreen ve onResized.
 * Tauri yoksa (tarayıcı önizlemesi, sunucu tarafı render) false kalır.
 */
export function useFullscreen(): boolean {
  const [fullscreen, setFullscreen] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined") return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let win: ReturnType<typeof getCurrentWindow>;
    try {
      win = getCurrentWindow();
    } catch {
      return;
    }
    const read = () =>
      win
        .isFullscreen()
        .then((v) => {
          if (!cancelled) setFullscreen(v);
        })
        .catch(() => {
          /* Tauri yok ya da izin yok: bant olduğu gibi kalır. */
        });
    void read();
    win
      .onResized(() => void read())
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return fullscreen;
}
