import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { BatchResult, ConversionResult, InspectOutcome, OutputFolder } from "./types";
import { ConversionWarning, FirstUseAcceptance } from "./Consent";
import { announce } from "../../shared-ui/Announcer";
import { Button, Section, Status } from "../../shared-ui/primitives";

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
 * Dönüştür — DOCX ↔ UDF.
 *
 * Motor `document-core`'un `convert` modülüdür ve hiç değiştirilmemiştir.
 * Bu bileşen yalnız bağımsız Tavzih'teki akışı yeni kabuğun görsel diliyle
 * yeniden çizer: onay → seçim → uyarı → dönüştürme → sonuç.
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
      const err = e as { message?: string };
      setFailure(err?.message ?? String(e));
      setPhase("idle");
    }
  }, [usable]);

  const chooseFolder = useCallback(async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    try {
      setFolder(await api.setOutputFolder(picked));
    } catch (e) {
      const err = e as { message?: string };
      setFailure(err?.message ?? String(e));
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

  return (
    <div className="surface">
      {failure ? <Status tone="error">{failure}</Status> : null}

      {usable.length > 0 ? (
        <Section title="Dönüştürülecek" id="secili">
          <ul className="file-list">
            {selected.map((s) => (
              <li key={s.path}>
                <div className="file-row">
                  <span className="file-name">{s.info?.name ?? s.path.split("/").pop()}</span>
                  {s.info ? (
                    <span className="file-kind">
                      {s.info.source_format} → {s.info.target_format}
                    </span>
                  ) : (
                    <span className="file-kind" data-tone="error">
                      {s.error?.message ?? "okunamadı"}
                    </span>
                  )}
                  {s.info ? <span className="file-time">{s.info.size_label}</span> : null}
                </div>
              </li>
            ))}
          </ul>
          <div className="row-end">
            <Button
              variant="primary"
              disabled={phase === "running"}
              onClick={() => setPhase("confirm")}
            >
              {phase === "running" ? "Dönüştürülüyor…" : "Dönüştür"}
            </Button>
          </div>
        </Section>
      ) : null}

      {items.length > 0 ? (
        <Section title="Sonuç" id="sonuc">
          {items.map((r) => (
            <div key={r.source} className="result" data-tone={r.status === "failure" ? "error" : undefined}>
              <div className="result-line">
                {r.source_name} → {r.output_name ?? "—"}
              </div>
              {r.error ? <div>{r.error.message}</div> : null}
              {/* Kaynak dosyanın değişmediği her sonuçta açıkça gösterilir:
                  motor kaynağı asla değiştirmez ve bunu hash'le kanıtlar. */}
              {r.source_unchanged ? <div>Kaynak belge değiştirilmedi.</div> : null}
              {r.warnings.length > 0 ? (
                <ul className="warn-list">
                  {r.warnings.map((w, i) => (
                    <li key={`${w.code}-${i}`}>
                      <strong>{SEVERITY_LABEL[w.severity] ?? w.severity}:</strong>{" "}
                      {w.title}
                      {w.location ? ` (${w.location})` : ""}
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ))}
        </Section>
      ) : null}

      {folder ? (
        <Section title="Çıktı klasörü" id="klasor">
          <p className="folder-path selectable">{folder.path}</p>
          <div className="row">
            <Button onClick={chooseFolder}>Değiştir…</Button>
            <Button onClick={() => api.revealOutputFolder()}>Klasörde Göster</Button>
            {!folder.is_default ? (
              <Button variant="quiet" onClick={async () => setFolder(await api.setOutputFolder(null))}>
                Varsayılana dön
              </Button>
            ) : null}
          </div>
        </Section>
      ) : null}

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
  );
}
