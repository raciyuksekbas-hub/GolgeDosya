import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { BatchResult, ConversionResult, InspectOutcome, OutputFolder } from "./types";
import { ConversionWarning, FirstUseAcceptance } from "./Consent";
import { announce } from "../../shared-ui/Announcer";
import { Button, Status } from "../../shared-ui/primitives";
import { ToolbarStatus } from "../../shell/chrome";
import { logFailure, safeMessage } from "../../shared-ui/failure";

type Phase = "idle" | "confirm" | "running" | "done";

const SEVERITY_LABEL: Record<string, string> = {
  INFO: "Bilgi",
  APPROXIMATION: "Yaklaşık aktarıldı",
  LOSS: "Kayıp",
};

function results(outcome: BatchResult | ConversionResult | null): ConversionResult[] {
  if (!outcome) return [];
  return "items" in outcome ? outcome.items : [outcome];
}

/**
 * Motorun biçim etiketini iki parçaya ayırır: `Word (.docx)` → `Word` + `.docx`.
 *
 * Saf sunum. Motor sözleşmesi tek bir dize veriyor; akışın iki durağı bu
 * dizeyi ad ve uzantı olarak iki satıra yazınca okunuyor. Parantez yoksa
 * etiket olduğu gibi kullanılır — uydurma yapılmaz.
 */
export function formatParts(label: string | null | undefined): { name: string; ext: string } {
  if (!label) return { name: "—", ext: "" };
  const m = label.match(/^(.*?)\s*\((\.[A-Za-z0-9]+)\)\s*$/);
  return m ? { name: m[1], ext: m[2] } : { name: label, ext: "" };
}

/**
 * Akışın iki durağı — kaynak ve hedef.
 *
 * Hazır ve tamamlandı durumları bunu PAYLAŞIR: iş bitince kullanıcı "az önce
 * ne dönüştürdüm" sorusunun cevabını hâlâ ekranda görmeli. Eskiden sonuç ayrı
 * bir ekran gibi açılıyor ve model kayboluyordu.
 */
export function FlowPair({ fromName, fromKind, toName, toKind }: {
  fromName: string;
  fromKind: string;
  toName: string;
  toKind: string;
}) {
  return (
    <div className="flow-pair">
      <div className="flow-end">
        <p className="flow-label">Kaynak</p>
        <p className="flow-name" title={fromName}>{fromName}</p>
        <p className="flow-kind">{fromKind}</p>
      </div>
      <span className="flow-arrow" aria-hidden="true">→</span>
      <div className="flow-end">
        <p className="flow-label">Hedef</p>
        <p className="flow-name" title={toName}>{toName}</p>
        <p className="flow-kind">{toKind}</p>
      </div>
    </div>
  );
}

/** Çıktının yeri: adı önde, tam yol ipucunda. */
export function FlowDestination({ folder, onChoose, onReset }: {
  folder: OutputFolder;
  onChoose?: () => void;
  onReset?: () => void;
}) {
  const name = folder.path.split("/").filter(Boolean).pop() ?? folder.path;
  return (
    <div className="flow-dest">
      <p className="flow-label">Çıktı klasörü</p>
      <p className="flow-dest-row">
        <span title={folder.path}>{name}</span>
        {onChoose ? (
          <Button className="btn-sm" variant="quiet" onClick={onChoose}>
            Değiştir
          </Button>
        ) : null}
        {onReset && !folder.is_default ? (
          <Button className="btn-sm" variant="quiet" onClick={onReset}>
            Varsayılana Dön
          </Button>
        ) : null}
      </p>
    </div>
  );
}

/**
 * Dönüşüm akışı — KAYNAK, yön, HEDEF, çıktı konumu ve tek eylem.
 *
 * Form değil, kart yığını da değil: iki durak aynı taban çizgisinde, aralarında
 * bir ok. Hiyerarşi hizalama ve boşlukla kurulur.
 *
 * Birincil eylem akışın İÇİNDEDİR. Yardımcı bara taşındığında ekranın konusu
 * (dönüşüm) ile eylemi birbirinden 500 piksel uzaklaşıyor ve bar aynı işi ikinci
 * kez ilan ediyordu. Bar artık yönü taşır, düğmeyi değil (§10, §22).
 */
export function ConvertFlow({
  selected,
  target,
  folder,
  busy,
  onConvert,
  onChooseFolder,
  onResetFolder,
}: {
  selected: InspectOutcome[];
  target: string | null;
  folder: OutputFolder | null;
  busy?: boolean;
  onConvert: () => void;
  onChooseFolder: () => void;
  onResetFolder: () => void;
}) {
  const source = selected[0];
  const sourceName = source?.info?.name ?? source?.path.split("/").pop() ?? "—";
  const to = formatParts(target);
  const usable = selected.filter((s) => s.info).length;

  return (
    <div className="flow">
      <FlowPair
        fromName={sourceName}
        fromKind={source?.info?.source_format ?? "—"}
        toName={to.name}
        toKind={to.ext || "—"}
      />

      {selected.length > 1 ? (
        <p className="flow-note">{selected.length} belge dönüştürülecek.</p>
      ) : null}
      {source && !source.info ? (
        <p className="flow-note" data-tone="error">
          {source.error?.message ?? "Bu belge okunamadı."}
        </p>
      ) : null}

      {folder ? (
        <FlowDestination folder={folder} onChoose={onChooseFolder} onReset={onResetFolder} />
      ) : null}

      <div className="flow-action">
        <Button variant="primary" disabled={busy || usable === 0} onClick={onConvert}>
          {busy ? "Dönüştürülüyor…" : "Dönüştür"}
        </Button>
      </div>
    </div>
  );
}

/**
 * Tamamlandı — ne üretildi, nerede, sırada ne var.
 *
 * Akış KAYBOLMAZ: iki durak aynı yerde durur, yalnız hedef artık bir biçim
 * adı değil üretilen dosyadır. Kullanıcının "az önce ne dönüştürdüm"
 * sorusunun cevabı ekranda kalır; sonuç ayrı bir ekran gibi açılmaz.
 *
 * Kaynağın değişmediği her sonuçta açıkça yazılır; motor bunu hash'le kanıtlar.
 */
export function ConvertDone({ items, from, to, folder, archiveName, onReveal, onAgain }: {
  items: ConversionResult[];
  /** Kaynağın biçim etiketi — akışın sol durağı için. */
  from: string | null;
  /** Hedefin biçim etiketi — akışın sağ durağı için. */
  to: string | null;
  folder: OutputFolder | null;
  /** Toplu iş tek bir arşiv ürettiyse onun adı. Motor arşivi yazdıktan sonra
   *  tekil çıktıları SİLER; bu ad gösterilmezse kullanıcı diskte olmayan
   *  dosyaları arar. */
  archiveName?: string | null;
  onReveal: () => void;
  onAgain: () => void;
}) {
  const failed = items.filter((r) => r.status === "failure").length;
  const single = items.length === 1 ? items[0] : null;
  const unchanged = items.every((r) => r.source_unchanged);
  // Başlık gerçeği söylemeli:
  //  · HİÇBİRİ dönüşmediyse "tamamlandı" yanlıştır — hiçbir şey üretilmedi.
  //  · İçerik KAYBI olduysa yalın yeşil ✓ kaybı gizler; hukuki belgede bu
  //    fark önemlidir. Uyarılar zaten aşağıda listeleniyor, başlık da bunu
  //    yansıtır.
  const allFailed = items.length > 0 && failed === items.length;
  const lossy = items.some((r) => r.warnings.some((w) => w.severity === "LOSS"));
  const mark = allFailed || failed > 0 ? "✕" : lossy ? "!" : "✓";
  const tone = failed > 0 ? "error" : lossy ? "warn" : undefined;
  const headline = allFailed
    ? `Dönüştürme başarısız: ${failed} belge dönüştürülemedi`
    : failed > 0
      ? `${items.length - failed} belge dönüştürüldü, ${failed} belge başarısız`
      : lossy
        ? items.length > 1
          ? `${items.length} belge dönüştürüldü — içerik kaybı var`
          : "Dönüştürme tamamlandı — içerik kaybı var"
        : items.length > 1
          ? `${items.length} belge dönüştürüldü`
          : "Dönüştürme tamamlandı";

  return (
    <section className="convert-done" aria-labelledby="sonuc-baslik">
      {single ? (
        <FlowPair
          fromName={single.source_name}
          fromKind={from ?? "—"}
          toName={single.output_name ?? "—"}
          toKind={to ?? "—"}
        />
      ) : null}

      <p className="flow-result" id="sonuc-baslik" data-tone={tone}>
        <span className="flow-result-mark" aria-hidden="true">{mark}</span>
        {headline}
      </p>

      {archiveName ? (
        <p className="flow-note">
          Dönüştürülen belgeler tek arşivde toplandı: <strong>{archiveName}</strong>
        </p>
      ) : null}

      {items.length > 1
        ? items.map((r) => (
            <p key={r.source} className="flow-note" data-tone={r.status === "failure" ? "error" : undefined}>
              {r.source_name} → {r.output_name ?? "—"}
            </p>
          ))
        : null}

      {items.flatMap((r) =>
        r.warnings.map((w, i) => (
          <p className="flow-warn" key={`${r.source}-${w.code}-${i}`}>
            <span className="flow-warn-mark" aria-hidden="true">▲</span>
            <span>
              {SEVERITY_LABEL[w.severity] ?? w.severity}: {w.title}
              {w.location ? ` (${w.location})` : ""}
            </span>
          </p>
        )),
      )}

      {items.map((r) =>
        r.error ? (
          <p className="flow-note" data-tone="error" key={`${r.source}-hata`}>
            {r.error.message}
          </p>
        ) : null,
      )}

      {unchanged ? <p className="flow-note">Kaynak belge değiştirilmedi.</p> : null}

      {folder ? <FlowDestination folder={folder} /> : null}

      <div className="flow-action">
        {folder ? (
          <Button variant="primary" onClick={onReveal}>
            Finder&apos;da Göster
          </Button>
        ) : null}
        <Button variant="quiet" onClick={onAgain}>
          Yeniden Dönüştür
        </Button>
      </div>
    </section>
  );
}

/**
 * Dönüştür — DOCX ↔ UDF.
 *
 * Motor `document-core`'un `convert` modülüdür ve hiç değiştirilmemiştir.
 * Akış da aynı: onay → seçim → uyarı → dönüştürme → sonuç.
 *
 * Yüzey bir form değil, tek bir dönüşüm akışıdır: KAYNAK → HEDEF, altında
 * çıktı konumu, altında tek birincil eylem. Yardımcı bar yönü taşır.
 *
 * Durumlar birbirini dışlar: belge inceleniyor → akış → sonuç. Kip kendi sağ
 * panelini açmaz: bir klasör adı üçüncü bir kolonu hak etmiyordu.
 */
export function ConvertWorkspace({ paths }: { paths: string[] }) {
  const [accepted, setAccepted] = useState<boolean | null>(null);
  const [acceptError, setAcceptError] = useState("");
  const [selected, setSelected] = useState<InspectOutcome[]>([]);
  const [folder, setFolder] = useState<OutputFolder | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [outcome, setOutcome] = useState<BatchResult | ConversionResult | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    api.termsAccepted().then(setAccepted).catch(() => setAccepted(false));
    api.outputFolder().then(setFolder).catch(() => setFolder(null));
  }, []);

  useEffect(() => {
    if (paths.length === 0) return;
    setOutcome(null);
    setFailure(null);
    setPhase("idle");
    api
      .inspectFiles(paths)
      .then(setSelected)
      // `String(e)` ham JS/Rust hatasını kullanıcıya sızdırıyordu; kategoriye
      // indirilmiş cümle gösterilir, ayrıntı konsolda kalır.
      .catch((e) => {
        logFailure("tavzih inspect failure", e);
        setFailure(safeMessage(e, "Belgeler incelenemedi. Dosyaları kontrol edip yeniden deneyin."));
      });
  }, [paths]);

  const usable = selected.filter((s) => s.info);

  const run = useCallback(async () => {
    setPhase("running");
    setFailure(null);
    try {
      const list = usable.map((s) => s.path);
      const result =
        list.length === 1
          ? await api.convertFile(list[0])
          : await api.convertBatch(list, api.localStamp());
      setOutcome(result);
      setPhase("done");
      const total = "items" in result ? result.total : 1;
      const failed = "items" in result ? result.failed : result.status === "failure" ? 1 : 0;
      // Ekran okuyucu kullanıcısı sonucu yalnız renkten/ikondan çıkaramaz;
      // duyuru da başlıkla aynı gerçeği söylemeli.
      announce(
        failed > 0 && failed === total
          ? `Dönüştürme başarısız. ${failed} belge dönüştürülemedi.`
          : failed > 0
            ? `${total - failed} belge dönüştürüldü, ${failed} belge başarısız.`
            : "Dönüştürme tamamlandı.",
      );
    } catch (e) {
      logFailure("tavzih convert failure", e);
      setFailure(safeMessage((e as { message?: string })?.message ?? e, "Dönüştürme tamamlanamadı. Yeniden deneyin."));
      setPhase("idle");
    }
  }, [usable]);

  const chooseFolder = useCallback(async () => {
    // `open()` try'ın DIŞINDAYDI: seçici hiç açılamazsa (izin, panel çökmesi)
    // söz reddi yakalanmıyor, kullanıcı düğmeye basıyor ve hiçbir şey
    // olmuyordu. VAZGEÇMEK ile BAŞARISIZLIK ayrı şeylerdir: ilki sessizdir,
    // ikincisi değil.
    try {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      setFolder(await api.setOutputFolder(picked));
    } catch (e) {
      logFailure("tavzih output folder failure", e);
      const message = safeMessage(
        (e as { message?: string })?.message ?? e,
        "Klasör seçilemedi. Başka bir klasör deneyin.",
      );
      setFailure(message);
      announce(message);
    }
  }, []);

  /**
   * Çıktı klasörünü varsayılana döndür.
   *
   * Akıştaki kopyası satır içi ve KORUMASIZDI
   * (`async () => setFolder(await api.setOutputFolder(null))`): yazma
   * düşerse klasör varsayılan dışı kalıyor, düğme hiçbir şey yapmamış
   * gibi görünüyordu. Tercihler penceresindeki kardeşi zaten korunuyordu.
   */
  const resetFolder = useCallback(async () => {
    try {
      setFolder(await api.setOutputFolder(null));
      announce("Çıktı klasörü varsayılana döndürüldü.");
    } catch (e) {
      logFailure("tavzih reset output folder", e);
      const message = safeMessage(e, "Varsayılana dönülemedi. Klasör olduğu gibi kaldı.");
      setFailure(message);
      announce(message);
    }
  }, []);

  if (accepted === false) {
    return (
      <FirstUseAcceptance
        error={acceptError}
        onAccept={async () => {
          // Onay yazması düşerse `setAccepted(true)` hiç çalışmıyordu:
          // kullanıcı "Kabul Ediyorum" düğmesine basıyor, ekran olduğu gibi
          // duruyor ve hiçbir açıklama gelmiyordu. Ürün, sebebi
          // söylenmeden kullanılamaz hâle geliyordu.
          setAcceptError("");
          try {
            await api.acceptTerms();
            setAccepted(true);
          } catch (e) {
            logFailure("tavzih accept terms", e);
            const message = safeMessage(
              e,
              "Onayınız kaydedilemedi. Yeniden deneyin; sorun sürerse diskte yer olduğundan emin olun.",
            );
            setAcceptError(message);
            announce(message);
          }
        }}
      />
    );
  }

  const items = results(outcome);

  const target = usable[0]?.info?.target_format ?? null;
  const done = phase === "done" && items.length > 0;

  return (
    <>
      {/* Barın ortası: bu belgenin hangi yöne dönüşeceği. Düğme burada
          tekrarlanmaz — eylem akışın içinde, işin yapıldığı yerde. */}
      {usable.length > 0 ? (
        <ToolbarStatus>
          {usable[0].info?.source_format} → {target ?? "—"}
        </ToolbarStatus>
      ) : null}

      <div className="surface convert">
        <div className="convert-body">
          {failure ? <Status tone="error">{failure}</Status> : null}

          {done ? (
            <ConvertDone
              items={items}
              archiveName={outcome && "items" in outcome && outcome.is_zip ? outcome.output_name : null}
              from={usable[0]?.info?.source_format ?? null}
              to={target}
              folder={folder}
              onReveal={async () => {
                // Yüzen bir sözdü: klasör silinmiş ya da taşınmışsa Finder
                // açılmıyor, kullanıcıya hiçbir şey söylenmiyordu.
                try {
                  await api.revealOutputFolder();
                } catch (e) {
                  logFailure("tavzih reveal output folder", e);
                  const message = safeMessage(
                    e,
                    "Çıktı klasörü açılamadı. Klasör taşınmış veya silinmiş olabilir.",
                  );
                  setFailure(message);
                  announce(message);
                }
              }}
              onAgain={() => setPhase("idle")}
            />
          ) : selected.length > 0 ? (
            <ConvertFlow
              selected={selected}
              target={target}
              folder={folder}
              busy={phase === "running"}
              onConvert={() => setPhase("confirm")}
              onChooseFolder={chooseFolder}
              onResetFolder={resetFolder}
            />
          ) : (
            <Status tone="busy">Belge inceleniyor…</Status>
          )}
        </div>

        {phase === "confirm" ? (
          <ConversionWarning
            count={usable.length}
            onCancel={() => setPhase("idle")}
            onConfirm={() => {
              setPhase("idle");
              void run();
            }}
          />
        ) : null}
      </div>
    </>
  );
}
