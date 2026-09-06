import { useRef } from "react";
import type { Settings } from "./types";
import { useFocusTrap } from "../shared-ui/useFocusTrap";

interface Props {
  settings: Settings;
  onChange: (next: Settings) => void;
  onClose: () => void;
}

/**
 * Ayarlar sayfası.
 *
 * Yalnız kullanıcının anlamlı bulacağı tercihler. Yapılandırma dizini, taşınan
 * ayarlar, feature bayrakları ve sürüm bilgisi burada YOKTUR; bunlar geliştirme
 * bilgisidir ve `console.debug`'a yazılır.
 */
export function SettingsSheet({ settings, onChange, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, true, onClose);

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    onChange({ ...settings, [key]: value });

  return (
    <div
      className="sheet-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="sheet" ref={ref} role="dialog" aria-modal="true" aria-labelledby="ayar-baslik">
        <h2 id="ayar-baslik">Ayarlar</h2>

        <div className="sheet-row">
          <label htmlFor="ayar-tema">Görünüm</label>
          <select
            id="ayar-tema"
            value={settings.theme}
            onChange={(e) => set("theme", e.target.value as Settings["theme"])}
          >
            <option value="system">Sistemle aynı</option>
            <option value="light">Açık</option>
            <option value="dark">Koyu</option>
          </select>
        </div>

        <div className="sheet-row">
          <label htmlFor="ayar-olcek">Metin boyutu</label>
          <select
            id="ayar-olcek"
            value={String(settings.textScale)}
            onChange={(e) => set("textScale", Number(e.target.value))}
          >
            <option value="100">Normal</option>
            <option value="125">Büyük</option>
            <option value="150">Daha büyük</option>
            <option value="175">Çok büyük</option>
            <option value="200">En büyük</option>
          </select>
        </div>

        <div className="sheet-row">
          <label htmlFor="ayar-kontrast">Yüksek kontrast</label>
          <select
            id="ayar-kontrast"
            value={settings.highContrast}
            onChange={(e) => set("highContrast", e.target.value as Settings["highContrast"])}
          >
            <option value="system">Sistemle aynı</option>
            <option value="on">Açık</option>
            <option value="off">Kapalı</option>
          </select>
        </div>

        <div className="sheet-row">
          <label htmlFor="ayar-hareket">Hareketi azalt</label>
          <select
            id="ayar-hareket"
            value={settings.reduceMotion}
            onChange={(e) => set("reduceMotion", e.target.value as Settings["reduceMotion"])}
          >
            <option value="system">Sistemle aynı</option>
            <option value="on">Açık</option>
            <option value="off">Kapalı</option>
          </select>
        </div>

        <div className="sheet-actions">
          <button type="button" className="btn" onClick={onClose}>
            Bitti
          </button>
        </div>
      </div>
    </div>
  );
}
