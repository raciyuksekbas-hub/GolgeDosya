import { useEffect, useRef } from "react";

/**
 * İlk kullanım onayı ve iş başına uyarı.
 *
 * Metinler ürünün hukuki duruşudur ve **birebir** gösterilir; özetlenmez,
 * "devamını oku" arkasına saklanmaz. Bağımsız Tavzih'ten kelimesi kelimesine
 * alınmıştır ve taşıma sırasında değiştirilmemiştir.
 */

export function FirstUseAcceptance({ onAccept }: { onAccept: () => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    ref.current?.focus();
    // Escape, kullanıcının cevaplaması gereken bir kapıyı kapatmamalı.
    const block = (e: KeyboardEvent) => {
      if (e.key === "Escape") e.preventDefault();
    };
    window.addEventListener("keydown", block, true);
    return () => window.removeEventListener("keydown", block, true);
  }, []);

  return (
    <div className="sheet-backdrop">
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="onay-baslik">
        <h2 id="onay-baslik">Kullanım Koşulları</h2>
        <div className="selectable" style={{ color: "var(--text-secondary)" }}>
          <p>
            GölgeDosya, Word ve UDF belgeleri arasında biçimsel dönüşüm sağlar. Dönüşüm sonucunda
            kaynak belgenin görünümünün, düzeninin ve biçimsel özelliklerinin birebir
            korunacağı garanti edilmez. Yazı tipi, paragraf düzeni, sekmeler, tablolar, sayfa
            yapısı, üst/alt bilgiler ve benzeri unsurlarda farklılıklar veya aktarım hataları
            oluşabilir.
          </p>
          <p>
            Dönüştürülen belgenin doğruluğu, bütünlüğü ve kullanıma uygunluğu kullanıcı
            tarafından kontrol edilmelidir. Belge, özellikle resmî veya hukuki bir işlemde
            kullanılmadan önce mutlaka gözden geçirilmelidir. GölgeDosya, kullanıcı kontrolü
            yapılmaksızın kullanılan belgelerden doğabilecek sonuçlardan sorumlu değildir.
          </p>
          <p>Devam ederek bu hususları okuduğunuzu ve kabul ettiğinizi beyan edersiniz.</p>
        </div>
        <div className="sheet-actions">
          <button type="button" className="btn btn-primary" ref={ref} onClick={onAccept}>
            Kabul Ediyorum
          </button>
        </div>
      </div>
    </div>
  );
}

export function ConversionWarning({
  count,
  onConfirm,
  onCancel,
}: {
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    ref.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCancel]);

  return (
    <div className="sheet-backdrop">
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="uyari-baslik">
        <h2 id="uyari-baslik">Dönüştürmeden önce</h2>
        <p style={{ color: "var(--text-secondary)" }}>
          {count > 1
            ? `${count} belge dönüştürülecek. `
            : ""}
          Biçimsel farklılıklar oluşabilir. Belgeyi kullanmadan önce kontrol etmeniz gerekir.
        </p>
        <div className="sheet-actions">
          <button type="button" className="btn" onClick={onCancel}>
            Vazgeç
          </button>
          <button type="button" className="btn btn-primary" ref={ref} onClick={onConfirm}>
            Anladım, Dönüştür
          </button>
        </div>
      </div>
    </div>
  );
}
