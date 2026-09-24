/**
 * Kaybettiren bir eylemden önce tek soru (§54).
 *
 * Yalnız GERÇEKTEN bir şey kaybolacaksa açılır; karar çağıranındır. Odak
 * güvenli seçenekte ("Vazgeç") başlar: Enter'a alışkanlıkla basan kullanıcı
 * çalışmasını silmez. Escape de vazgeçer. Görünüm Dönüştür'ün onay
 * penceresiyle aynı `sheet` kalıbıdır; yeni bir görsel dil yoktur.
 */
import { useEffect, useRef, type ReactNode } from "react";
import { useFocusTrap } from "./useFocusTrap";

export function ConfirmSheet({
  title,
  children,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const sheet = useRef<HTMLDivElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  useFocusTrap(sheet, true, onCancel);
  useEffect(() => {
    cancel.current?.focus();
  }, []);

  return (
    <div className="sheet-backdrop">
      <div
        className="sheet"
        ref={sheet}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="onay-baslik"
        aria-describedby="onay-metin"
      >
        <h2 id="onay-baslik">{title}</h2>
        <p id="onay-metin" style={{ color: "var(--text-secondary)" }}>
          {children}
        </p>
        <div className="sheet-actions">
          <button type="button" className="btn" ref={cancel} onClick={onCancel}>
            Vazgeç
          </button>
          <button type="button" className="btn btn-primary" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
