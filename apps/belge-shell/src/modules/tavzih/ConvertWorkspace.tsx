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
  const folderName = folder ? folder.path.split("/").filter(Boolean).pop() ?? folder.path : "";
  const source = selected[0];
  const sourceName = source?.info?.name ?? source?.path.split("/").pop() ?? "—";
  const to = formatParts(target);
  const usable = selected.filter((s) => s.info).length;

  return (
    <div className="flow">
      <div className="flow-pair">
        <div className="flow-end">
          <p className="flow-label">Kaynak</p>
          <p className="flow-name" title={sourceName}>
            {sourceName}
          </p>
          <p className="flow-kind">{source?.info?.source_format ?? "—"}</p>
        </div>
        <span className="flow-arrow" aria-hidden="true">
          →
        </span>
        <div className="flow-end">
          <p className="flow-label">Hedef</p>
          <p className="flow-name">{to.name}</p>
          <p className="flow-kind">{to.ext || " "}</p>
        </div>
      </div>

      {selected.length > 1 ? (
        <p className="flow-note">{selected.length} belge dönüştürülecek.</p>
      ) : null}
      {source && !source.info ? (
        <p className="flow-note" data-tone="error">
          {source.error?.message ?? "Bu belge okunamadı."}
        </p>
      ) : null}

      {folder ? (
        <div className="flow-dest">
          <p className="flow-label">Çıktı konumu</p>
          <p className="flow-dest-row">
            <span title={folder.path}>{folderName}</span>
            <Button className="btn-sm" variant="quiet" onClick={onChooseFolder}>
              Değiştir…
            </Button>
            {!folder.is_default ? (
              <Button className="btn-sm" variant="quiet" onClick={onResetFolder}>
                Varsayılana dön
              </Button>
            ) : null}
          </p>
        </div>
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
 * Akışın yerini alır, üstüne binmez: iş bitince kullanıcının sorusu artık
 * "neye dönüşecek" değil "ne çıktı ve nerede" sorusudur. Kaynağın
 * değişmediği her sonuçta açıkça yazılır; motor bunu hash'le kanıtlar.
 */
export function ConvertDone({ items, canReveal, onReveal, onAgain }: {
  items: ConversionResult[];
  canReveal: boolean;
  onReveal: () => void;
  onAgain: () => void;
}) {
  const failed = items.filter((r) => r.status === "failure").length;
  return (
    <section className="convert-done" aria-labelledby="sonuc-baslik">
      <h1 id="sonuc-baslik">
        {failed > 0 ? "Dönüştürme tamamlandı, hatalar var" : "Dönüştürme tamamlandı"}
      </h1>
      {items.map((r) => (
        <div key={r.source} className="result" data-tone={r.status === "failure" ? "error" : undefined}>
          <p className="result-done">
            <span className="result-line">
              {r.source_name} → {r.output_name ?? "—"}
            </span>
            <span className="result-ok">
              {r.status === "failure" ? "✕ Başarısız" : "✓ Tamamlandı"}
            </span>
          </p>
          {r.error ? <p className="flow-note" data-tone="error">{r.error.message}</p> : null}
          {r.warnings.length > 0 ? (
            <ul className="warn-list">
              {r.warnings.map((w, i) => (
                <li key={`${w.code}-${i}`}>
                  <strong>{SEVERITY_LABEL[w.severity] ?? w.severity}:</strong> {w.title}
                  {w.location ? ` (${w.location})` : ""}
                </li>
              ))}
            </ul>
          ) : null}
          {r.source_unchanged ? <p className="flow-note">Kaynak belge değiştirilmedi.</p> : null}
        </div>
      ))}
      <div className="convert-action">
        {canReveal ? (
          <Button variant="primary" onClick={onReveal}>
            Finder'da Göster
          </Button>
        ) : null}
        <Button variant="quiet" onClick={onAgain}>
          Yeniden dönüştür
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
    api.inspectFiles(paths).then(setSelected).catch((e) => setFailure(String(e)));
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
      const failed = "items" in result ? result.failed : result.status === "failure" ? 1 : 0;
      announce(failed > 0 ? "Dönüştürme tamamlandı, hatalar var." : "Dönüştürme tamamlandı.");
    } catch (e) {
      logFailure("tavzih convert failure", e);
      setFailure(safeMessage((e as { message?: string })?.message ?? e, "Dönüştürme tamamlanamadı. Yeniden deneyin."));
      setPhase("idle");
    }
  }, [usable]);

  const chooseFolder = useCallback(async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    try {
      setFolder(await api.setOutputFolder(picked));
    } catch (e) {
      logFailure("tavzih output folder failure", e);
      setFailure(safeMessage((e as { message?: string })?.message ?? e, "Klasör seçilemedi. Başka bir klasör deneyin."));
    }
  }, []);

  if (accepted === false) {
    return (
      <FirstUseAcceptance
        onAccept={async () => {
          await api.acceptTerms();
          setAccepted(true);
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
              canReveal={folder !== null}
              onReveal={() => api.revealOutputFolder()}
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
              onResetFolder={async () => setFolder(await api.setOutputFolder(null))}
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
