import { useRef } from "react";
import type { Settings } from "./types";
import { useFocusTrap } from "../shared-ui/useFocusTrap";
import { Button, Field } from "../shared-ui/primitives";

interface Props {
  settings: Settings;
  onChange: (next: Settings) => void;
  onClose: () => void;
}

/**
 * Ayarlar — macOS sheet.
 *
 * Yalnız kullanıcının anlamlı bulacağı ve GERÇEKTEN var olan tercihler.
 * Yapılandırma dizini, taşınan ayarlar, feature bayrakları, bundle kimliği ve
 * depolama anahtarları burada YOKTUR; bunlar geliştirme bilgisidir.
 *
 * Bölümler mevcut ayarlara göre kuruldu; olmayan özellik için hayalî ayar
 * eklenmedi. "Dönüştürme" bölümü yok çünkü çıktı klasörü Dönüştür kipinin
 * kendi yüzeyinde, işin yapıldığı yerde duruyor.
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
        <div className="sheet-head">
          <h2 id="ayar-baslik">Ayarlar</h2>
        </div>

        <div className="sheet-body">
          <section className="settings-group" aria-labelledby="ayar-gorunum">
            <h3 id="ayar-gorunum">Görünüm</h3>
            <Field label="Tema" htmlFor="ayar-tema">
              <select
                id="ayar-tema"
                value={settings.theme}
                onChange={(e) => set("theme", e.target.value as Settings["theme"])}
              >
                <option value="system">Sistemle aynı</option>
                <option value="light">Açık</option>
                <option value="dark">Koyu</option>
              </select>
            </Field>
            <Field label="Metin boyutu" htmlFor="ayar-olcek">
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
            </Field>
          </section>

          <section className="settings-group" aria-labelledby="ayar-erisim">
            <h3 id="ayar-erisim">Erişilebilirlik</h3>
            <Field
              label="Yüksek kontrast"
              hint="Çizgileri ve ikincil metni güçlendirir"
              htmlFor="ayar-kontrast"
            >
              <select
                id="ayar-kontrast"
                value={settings.highContrast}
                onChange={(e) => set("highContrast", e.target.value as Settings["highContrast"])}
              >
                <option value="system">Sistemle aynı</option>
                <option value="on">Açık</option>
                <option value="off">Kapalı</option>
              </select>
            </Field>
            <Field
              label="Hareketi azalt"
              hint="Geçişleri kaldırır"
              htmlFor="ayar-hareket"
            >
              <select
                id="ayar-hareket"
                value={settings.reduceMotion}
                onChange={(e) => set("reduceMotion", e.target.value as Settings["reduceMotion"])}
              >
                <option value="system">Sistemle aynı</option>
                <option value="on">Açık</option>
                <option value="off">Kapalı</option>
              </select>
            </Field>
          </section>

          <section className="settings-group" aria-labelledby="ayar-hakkinda">
            <h3 id="ayar-hakkinda">Hakkında</h3>
            <p className="settings-about">
              Yüksekbaş Belge — belge çalışma ortamı.
              <br />
              © 2026 Raci Çetin Yüksekbaş
            </p>
          </section>
        </div>

        <div className="sheet-actions">
          <Button onClick={onClose}>Bitti</Button>
        </div>
      </div>
    </div>
  );
}
