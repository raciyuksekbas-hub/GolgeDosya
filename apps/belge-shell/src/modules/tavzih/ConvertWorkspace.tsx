import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { BatchResult, ConversionResult, InspectOutcome, OutputFolder } from "./types";
import { ConversionWarning, FirstUseAcceptance } from "./Consent";
import { announce } from "../../shared-ui/Announcer";
import { Button, Pill, Section, Status } from "../../shared-ui/primitives";
import { InspectorPanel, InspectorSection, ToolbarActions } from "../../shell/chrome";

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
 * Akış da aynı: onay → seçim → uyarı → dönüştürme → sonuç.
 *
 * Yüzey bir form değil, sakin bir dönüşüm akışıdır: KAYNAK, aşağı ok, HEDEF.
 * Birincil eylem yardımcı barda; çıktı klasörü sağ panelde, işin yapıldığı
 * yerin yanında. Sonuç tek yerde — workspace'te — durur.
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

  const target = usable[0]?.info?.target_format ?? null;

  return (
    <>
      {usable.length > 0 ? (
        <ToolbarActions>
          <Button
            variant="primary"
            disabled={phase === "running"}
            onClick={() => setPhase("confirm")}
          >
            {phase === "running" ? "Dönüştürülüyor…" : "Dönüştür"}
          </Button>
        </ToolbarActions>
      ) : null}

      {folder ? (
        <InspectorPanel title="Çıktı">
          <p className="folder-path selectable" title={folder.path}>
            {folder.path}
          </p>
          <div className="row">
            <Button onClick={() => api.revealOutputFolder()}>Finder'da Göster</Button>
            <Button onClick={chooseFolder}>Değiştir…</Button>
          </div>
          {!folder.is_default ? (
            <Button variant="quiet" onClick={async () => setFolder(await api.setOutputFolder(null))}>
              Varsayılana dön
            </Button>
          ) : null}
          {items.length > 0 ? (
            <InspectorSection title="Son işlem">
              {items.map((r) => (
                <p key={r.source} className="tool-hint">
                  {r.output_name ?? r.source_name}
                </p>
              ))}
            </InspectorSection>
          ) : null}
        </InspectorPanel>
      ) : null}

      <div className="surface convert">
        {failure ? <Status tone="error">{failure}</Status> : null}

        {selected.length > 0 ? (
          <div className="flow">
            <div className="flow-step">
              <h2 className="section-head">Kaynak</h2>
              <ul className="file-list">
                {selected.map((s) => (
                  <li key={s.path}>
                    <div className="file-row">
                      <span className="file-name">{s.info?.name ?? s.path.split("/").pop()}</span>
                      {s.info ? (
                        <>
                          <Pill>{s.info.source_format}</Pill>
                          <span className="file-time">{s.info.size_label}</span>
                        </>
                      ) : (
                        <span className="file-kind" data-tone="error">
                          {s.error?.message ?? "okunamadı"}
                        </span>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            </div>

            {target ? (
              <>
                <div className="flow-arrow" aria-hidden="true">
                  ↓
                </div>
                <div className="flow-step">
                  <h2 className="section-head">Hedef</h2>
                  <p className="flow-target">{target}</p>
                </div>
              </>
            ) : null}
          </div>
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
                        <strong>{SEVERITY_LABEL[w.severity] ?? w.severity}:</strong> {w.title}
                        {w.location ? ` (${w.location})` : ""}
                      </li>
                    ))}
                  </ul>
                ) : null}
              </div>
            ))}
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
    </>
  );
}
