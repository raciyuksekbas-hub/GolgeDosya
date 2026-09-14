import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { OutputFolder } from "./types";
import { Button } from "../../shared-ui/primitives";

/**
 * Çıktı klasörü satırı — saf sunum.
 *
 * Tercihler penceresinin tek grid'ine oturur: etiket ve mevcut değer solda,
 * kontrol sağ kolonda. Tam yol ipucunda durur; tercihler penceresinde dokuz
 * satıra sarılan bir yol, klasörün adından daha az bilgi taşıyor.
 */
export function OutputFolderRow({
  folder,
  onChoose,
  onReset,
}: {
  folder: OutputFolder;
  onChoose: () => void;
  onReset: () => void;
}) {
  const name = folder.path.split("/").filter(Boolean).pop() ?? folder.path;
  return (
    <>
      <div className="field">
        <div className="field-label">
          Çıktı klasörü
          <span title={folder.path}>
            {name}
            {folder.is_default ? " · varsayılan" : ""}
          </span>
        </div>
        <Button onClick={onChoose}>Değiştir…</Button>
      </div>
      {!folder.is_default ? (
        <p className="prefs-hint">
          <Button className="btn-sm" variant="quiet" onClick={onReset}>
            Varsayılana dön
          </Button>
        </p>
      ) : null}
    </>
  );
}

/**
 * Çıktı klasörü tercihi.
 *
 * Tercihler penceresinin `Genel` bölümünde durur ama komut bu modülün
 * kendisine aittir: kabuk `tavzih_output_folder`'ı tanımaz, yalnız bu
 * bileşeni yerleştirir. Komut adı, anlamı ve varsayılanı DEĞİŞMEDİ.
 *
 * Motor bu derlemeye linklenmemişse çağrı reddedilir ve satır hiç çizilmez:
 * uydurma bir tercih göstermektense hiç göstermemek doğrudur.
 */
export function OutputFolderField() {
  const [folder, setFolder] = useState<OutputFolder | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .outputFolder()
      .then((f) => !cancelled && setFolder(f))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const choose = useCallback(async () => {
    const picked = await open({ directory: true, multiple: false }).catch(() => null);
    if (typeof picked !== "string") return;
    setFolder(await api.setOutputFolder(picked).catch(() => folder));
  }, [folder]);

  const reset = useCallback(async () => {
    setFolder(await api.setOutputFolder(null).catch(() => folder));
  }, [folder]);

  if (!folder) return null;
  return <OutputFolderRow folder={folder} onChoose={choose} onReset={reset} />;
}
