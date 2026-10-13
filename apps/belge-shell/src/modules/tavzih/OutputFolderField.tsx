import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { OutputFolder } from "./types";
import { Button, Status } from "../../shared-ui/primitives";
import { logFailure, safeMessage } from "../../shared-ui/failure";
import { fileNameOf } from "../../shell/modes";

/**
 * Dönüştür'ün çıktı klasörü satırı — saf sunum.
 *
 * Satır eskiden yalnız "Çıktı klasörü" diyordu ve Ayarlar > Genel'de bütün
 * uygulamanın tercihi gibi duruyordu; ama onu yalnız Dönüştür okur. Kullanıcı
 * klasörü değiştirip Düzenle'de kaydedince dosya başka yere gidiyordu ("kayıt
 * klasörleri arasında birlik yok"). Etiket artık neyin buraya yazıldığını,
 * ipucu da öteki araçların ne yaptığını söyler. Tercihin anlamı DEĞİŞMEDİ.
 *
 * Tercihler penceresinin tek grid'ine oturur: etiket ve mevcut değer solda,
 * kontrol sağ kolonda. Tam yol ipucunda durur; tercihler penceresinde dokuz
 * satıra sarılan bir yol, klasörün adından daha az bilgi taşıyor.
 */
/**
 * Satırın kapsamı: yalnız Dönüştür buraya yazar. Öteki araçlar adlarıyla
 * sayılır, çünkü hepsi aynı şeyi yapmaz: Düzenle, Ekler ve Denetle kaydetme
 * penceresini belgenin klasöründe açar (`startFolder`); Karşılaştır raporunu
 * pencere açmadan İndirilenler'e yazar (`degisikis.rs` report_directory).
 * "Öteki araçlar belgenin klasörünü önerir" Karşılaştır için yanlıştı.
 */
export const OUTPUT_FOLDER_HINT =
  "Yalnız Dönüştür buraya kaydeder. Düzenle, Ekler ve Denetle kaydederken belgenin klasörünü önerir. Karşılaştır'ın raporu doğrudan İndirilenler klasörüne yazılır.";

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
  const name = fileNameOf(folder.path);
  return (
    <>
      <div className="field">
        <div className="field-label">
          Dönüştürülen belgeler
          <span title={folder.path}>
            {name}
            {folder.is_default ? " · varsayılan" : ""}
          </span>
        </div>
        <Button onClick={onChoose}>Değiştir</Button>
      </div>
      <p className="prefs-hint">{OUTPUT_FOLDER_HINT}</p>
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
 * Word → UDF dilekçe aralığı tercihi — saf sunum.
 *
 * UYAP'a gidecek dilekçenin olağan biçimi satır aralığı 1,5 ve paragraf
 * öncesi/sonrası boşluksuzdur; dönüştürme bunu varsayılan olarak uygular.
 * Kapalıyken Word belgesindeki aralıklar korunur (sayfa akışı korunması gereken
 * sözleşme gibi belgeler için).
 */
export function PetitionSpacingRow({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <label className="field prefs-check">
      <span className="field-label">
        Word'den UDF'e satır aralığı 1,5
        <span>Paragraf öncesi ve sonrası boşluk bırakılmaz. Kapalıyken Word belgesindeki aralıklar korunur.</span>
      </span>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
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
export function OutputFolderField({
  petitionSpacing,
  onPetitionSpacing,
}: {
  /** Dönüştür'ün ikinci tercihi; motor yoksa onunla birlikte gizlenir. */
  petitionSpacing?: boolean;
  onPetitionSpacing?: (next: boolean) => void;
} = {}) {
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
    <>
      <OutputFolderRow folder={folder} error={error} onChoose={choose} onReset={reset} />
      {onPetitionSpacing ? (
        <PetitionSpacingRow checked={petitionSpacing !== false} onChange={onPetitionSpacing} />
      ) : null}
    </>
  );
}
