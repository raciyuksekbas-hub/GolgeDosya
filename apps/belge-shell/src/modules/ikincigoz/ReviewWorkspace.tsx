import { useCallback, useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { AnalysisResult, Finding, Fix, Severity } from "./types";
import { announce } from "../../shared-ui/Announcer";

/**
 * Severity'nin metin karşılığı.
 *
 * Bulgu ciddiyeti ASLA yalnız renkle verilmez: her satır bu etiketi ve kendi
 * şeklini taşır. Renk körlüğü ve yüksek kontrast modunda ayrım kaybolmamalı.
 */
export const SEVERITY: Record<Severity, { label: string; mark: string }> = {
  error: { label: "Kesin hata", mark: "●" },
  warning: { label: "Uyarı", mark: "▲" },
  review: { label: "İncele", mark: "○" },
};

function summary(r: AnalysisResult): string {
  return [
    `${r.errorCount} kesin hata`,
    `${r.warningCount} uyarı`,
    `${r.reviewCount} incele`,
  ].join(" · ");
}

/**
 * Denetle — belge incelemesi ve cerrahi düzeltme.
 *
 * Zincir: belge açıldı → incele → bulgular → konum → açıklama → önerilen
 * düzeltme → uygula. Düzeltmeler **kopyaya** yazılır; kaynak belge hiçbir
 * zaman yazmak için açılmaz.
 *
 * Yeni UX icat edilmedi; standalone İkinciGöz'ün akışı kabuğun görsel diline
 * uyarlandı. Parser ve migration ayrıntıları kullanıcıya gösterilmez.
 */
export function ReviewWorkspace({ path }: { path: string }) {
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [active, setActive] = useState<string | null>(null);
  const [written, setWritten] = useState<string | null>(null);

  const key = (f: Finding) => `${f.rule_id}:${f.block_id}:${f.location.charStart ?? -1}`;

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setResult(null);
    setFailure(null);
    setSelected(new Set());
    setWritten(null);
    api
      .analyzeDocument(path)
      .then((r) => {
        if (cancelled) return;
        setResult(r);
        setActive(r.findings.length > 0 ? key(r.findings[0]) : null);
        announce(
          r.findings.length === 0
            ? "İnceleme tamamlandı. Bulgu yok."
            : `İnceleme tamamlandı. ${summary(r)}.`,
        );
      })
      .catch((e) => !cancelled && setFailure(String(e)))
      .finally(() => !cancelled && setBusy(false));
    return () => {
      cancelled = true;
    };
  }, [path]);

  const blockText = useMemo(() => {
    const m = new Map<string, string>();
    result?.blocks.forEach((b) => m.set(b.id, b.text));
    return m;
  }, [result]);

  const fixable = useMemo(
    () => (result?.findings ?? []).filter((f) => f.fix !== null),
    [result],
  );

  const toggle = useCallback((f: Finding) => {
    setSelected((prev) => {
      const next = new Set(prev);
      const k = `${f.rule_id}:${f.block_id}:${f.location.charStart ?? -1}`;
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }, []);

  const chosen: Fix[] = useMemo(
    () => fixable.filter((f) => selected.has(key(f))).map((f) => f.fix as Fix),
    [fixable, selected],
  );

  const applyChosen = useCallback(async () => {
    if (chosen.length === 0) return;
    const target = await save({
      defaultPath: undefined,
      title: "Düzeltilmiş kopyayı kaydet",
    });
    if (!target) return;
    const dir = target.slice(0, target.lastIndexOf("/"));
    try {
      const r = await api.applyFixes(path, chosen, dir || undefined);
      setWritten(`${r.fileName} · ${r.applied} düzeltme uygulandı`);
      announce(`${r.applied} düzeltme uygulandı. Kopya kaydedildi.`);
      // Kaynak değişmediği için bulgular geçerliliğini korur; seçim sıfırlanır.
      setSelected(new Set());
    } catch (e) {
      setFailure(String(e));
    }
  }, [chosen, path]);

  if (busy) {
    return (
      <div className="doc-surface">
        <p className="notice" role="status">Belge inceleniyor…</p>
      </div>
    );
  }

  if (failure) {
    return (
      <div className="doc-surface">
        <p className="notice" data-tone="error" role="alert">{failure}</p>
      </div>
    );
  }

  if (!result) return null;

  return (
    <div className="doc-surface">
      <p className="notice" role="status">
        <strong style={{ fontWeight: 600 }}>{result.document.fileName}</strong>
        {" · "}
        {result.document.format} · {result.document.blockCount} paragraf ·{" "}
        {result.document.wordCount} kelime
        <br />
        {summary(result)}
        {result.profileName ? ` · ${result.profileName} profiline göre` : ""}
      </p>

      {result.findings.length === 0 ? (
        <p className="notice">Bu belgede bulgu yok.</p>
      ) : (
        <section aria-labelledby="bulgu-baslik">
          <h2
            id="bulgu-baslik"
            style={{ fontSize: "0.86em", textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--text-tertiary)", margin: "0 0 6px" }}
          >
            Bulgular
          </h2>
          <ul className="findings" role="list">
            {result.findings.map((f) => {
              const k = key(f);
              const sev = SEVERITY[f.severity];
              const isActive = active === k;
              const text = blockText.get(f.block_id) ?? "";
              return (
                <li key={k} className="finding" data-severity={f.severity} data-active={isActive}>
                  <button
                    type="button"
                    className="finding-head"
                    aria-expanded={isActive}
                    onClick={() => setActive(isActive ? null : k)}
                  >
                    <span className="finding-mark" aria-hidden="true">{sev.mark}</span>
                    <span className="finding-sev">{sev.label}</span>
                    <span className="finding-title">{f.title}</span>
                    <span className="finding-loc">{f.block_id.replace(/^p/, "")}. paragraf</span>
                  </button>
                  {isActive ? (
                    <div className="finding-body">
                      <p className="finding-message">{f.message}</p>
                      {/* Belge konumu: bulgu, metnin neresinde olduğunu göstermeli. */}
                      {f.location.charStart !== null && f.location.charEnd !== null && text ? (
                        <p className="finding-excerpt selectable">
                          {[...text].slice(Math.max(0, f.location.charStart - 40), f.location.charStart).join("")}
                          <mark>{[...text].slice(f.location.charStart, f.location.charEnd).join("")}</mark>
                          {[...text].slice(f.location.charEnd, f.location.charEnd + 40).join("")}
                        </p>
                      ) : null}
                      <p className="finding-why">{f.explanation}</p>
                      {f.fix ? (
                        <label className="finding-fix">
                          <input
                            type="checkbox"
                            checked={selected.has(k)}
                            onChange={() => toggle(f)}
                          />
                          <span>
                            Önerilen düzeltme: <code>{f.fix.original || "∅"}</code> →{" "}
                            <code>{f.fix.replacement || "∅"}</code>
                            <span className="finding-fixnote"> ({f.fix.description})</span>
                          </span>
                        </label>
                      ) : (
                        <p className="finding-nofix">Bu bulgu için otomatik düzeltme önerilmiyor.</p>
                      )}
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {result.truncatedRules.length > 0 ? (
        <p className="notice">
          {result.truncatedRules
            .map((t) => `${t.ruleId}: ${t.total} bulgunun ${t.shown} tanesi gösteriliyor`)
            .join(" · ")}
        </p>
      ) : null}

      {fixable.length > 0 ? (
        <section className="notice">
          <div>
            {selected.size} / {fixable.length} düzeltme seçildi.
          </div>
          <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
            <button
              type="button"
              className="btn"
              onClick={() => setSelected(new Set(fixable.map(key)))}
              disabled={selected.size === fixable.length}
            >
              Tümünü seç
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => setSelected(new Set())}
              disabled={selected.size === 0}
            >
              Seçimi kaldır
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={applyChosen}
              disabled={selected.size === 0}
            >
              Kopyaya uygula…
            </button>
          </div>
          <p style={{ margin: "8px 0 0", color: "var(--text-tertiary)" }}>
            Düzeltmeler yeni bir kopyaya yazılır. Kaynak belgeniz değiştirilmez.
          </p>
        </section>
      ) : null}

      {written ? (
        <p className="notice" role="status">{written}</p>
      ) : null}
    </div>
  );
}
