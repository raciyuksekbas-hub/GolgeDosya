import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { BatchResult, ConversionResult, InspectOutcome, OutputFolder } from "./types";
import { ConversionWarning, FirstUseAcceptance } from "./Consent";
import { announce } from "../../shared-ui/Announcer";

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
    <div className="doc-surface">
      {failure ? (
        <p className="notice" data-tone="error" role="alert">
          {failure}
        </p>
      ) : null}

      {usable.length > 0 ? (
        <section aria-labelledby="secili-baslik">
          <h2 id="secili-baslik" style={{ fontSize: "0.86em", textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--text-tertiary)", margin: "0 0 6px" }}>
            Dönüştürülecek
          </h2>
          <ul className="recents-list">
            {selected.map((s) => (
              <li key={s.path}>
                <div className="recent-item" style={{ cursor: "default" }}>
                  <span className="recent-name">{s.info?.name ?? s.path.split("/").pop()}</span>
                  {s.info ? (
                    <span className="recent-kind">
                      {s.info.source_format} → {s.info.target_format} · {s.info.size_label}
                    </span>
                  ) : (
                    <span className="recent-kind" style={{ color: "var(--danger)" }}>
                      {s.error?.message ?? "okunamadı"}
                    </span>
                  )}
                </div>
              </li>
            ))}
          </ul>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 12 }}>
            <button
              type="button"
              className="btn btn-primary"
              disabled={phase === "running"}
              onClick={() => setPhase("confirm")}
            >
              {phase === "running" ? "Dönüştürülüyor…" : "Dönüştür"}
            </button>
          </div>
        </section>
      ) : null}

      {items.length > 0 ? (
        <section aria-labelledby="sonuc-baslik">
          <h2 id="sonuc-baslik" style={{ fontSize: "0.86em", textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--text-tertiary)", margin: "0 0 6px" }}>
            Sonuç
          </h2>
          {items.map((r) => (
            <div key={r.source} className="notice" style={{ marginBottom: 8 }} data-tone={r.status === "failure" ? "error" : undefined}>
              <div style={{ color: "var(--text)" }}>
                {r.source_name} → {r.output_name ?? "—"}
              </div>
              {r.error ? <div>{r.error.message}</div> : null}
              {/* Kaynak dosyanın değişmediği her sonuçta açıkça gösterilir:
                  motor kaynağı asla değiştirmez ve bunu hash'le kanıtlar. */}
              {r.source_unchanged ? <div>Kaynak belge değiştirilmedi.</div> : null}
              {r.warnings.length > 0 ? (
                <ul style={{ margin: "6px 0 0", paddingLeft: 18 }}>
                  {r.warnings.map((w, i) => (
                    <li key={`${w.code}-${i}`}>
                      <strong style={{ fontWeight: 500 }}>{SEVERITY_LABEL[w.severity] ?? w.severity}:</strong>{" "}
                      {w.title}
                      {w.location ? ` (${w.location})` : ""}
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ))}
        </section>
      ) : null}

      {folder ? (
        <section className="notice">
          <div>Çıktı klasörü: <span className="selectable">{folder.path}</span></div>
          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <button type="button" className="btn" onClick={chooseFolder}>
              Değiştir…
            </button>
            <button type="button" className="btn" onClick={() => api.revealOutputFolder()}>
              Klasörde Göster
            </button>
            {!folder.is_default ? (
              <button
                type="button"
                className="btn btn-quiet"
                onClick={async () => setFolder(await api.setOutputFolder(null))}
              >
                Varsayılana dön
              </button>
            ) : null}
          </div>
        </section>
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
