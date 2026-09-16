import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { OutputFolder } from "./types";
import { Button, Status } from "../../shared-ui/primitives";
import { logFailure, safeMessage } from "../../shared-ui/failure";

/**
 * Çıktı klasörü satırı — saf sunum.
 *
 * Tercihler penceresinin tek grid'ine oturur: etiket ve mevcut değer solda,
 * kontrol sağ kolonda. Tam yol ipucunda durur; tercihler penceresinde dokuz
 * satıra sarılan bir yol, klasörün adından daha az bilgi taşıyor.
 */
export function OutputFolderRow({
  folder,
  error,
  onChoose,
  onReset,
}: {
  folder: OutputFolder;
  /** Klasör değiştirilemediyse nedeni. Sessiz kalmak yerine söylenir. */
  error?: string;
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
        <Button onClick={onChoose}>Değiştir</Button>
      </div>
      {error ? (
        <p className="prefs-hint">
          <Status tone="error">{error}</Status>
        </p>
      ) : null}
      {!folder.is_default ? (
        <p className="prefs-hint">
          <Button className="btn-sm" variant="quiet" onClick={onReset}>
            Varsayılana Dön
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

  // Klasör DEĞİŞTİRİLEMEDİĞİNDE sessiz kalınmıyordu-kalınıyordu: hata
  // `.catch(() => folder)` ile yutuluyor, satır eski değeri göstermeye devam
  // ediyor ve kullanıcı neden hiçbir şey olmadığını anlamıyordu. Motor sebebi
  // biliyor (yazılamayan klasör, izin yok); o sebep artık ekrana çıkar.
  const [error, setError] = useState("");

  const apply = useCallback(
    async (next: string | null) => {
      setError("");
      try {
        setFolder(await api.setOutputFolder(next));
      } catch (e) {
        logFailure("tavzih output folder", e);
        setError(
          safeMessage(e, "Bu klasör kullanılamadı. Yazma izni olan başka bir klasör seçin."),
        );
      }
    },
    [],
  );

  const choose = useCallback(async () => {
    const picked = await open({ directory: true, multiple: false }).catch(() => null);
    // Kullanıcı vazgeçtiyse bu bir hata değildir; sessizlik doğrudur.
    if (typeof picked !== "string") return;
    await apply(picked);
  }, [apply]);

  const reset = useCallback(async () => {
    await apply(null);
  }, [apply]);

  if (!folder) return null;
  return (
    <OutputFolderRow folder={folder} error={error} onChoose={choose} onReset={reset} />
  );
}
