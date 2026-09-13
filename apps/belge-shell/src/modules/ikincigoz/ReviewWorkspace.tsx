import { useCallback, useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { AnalysisResult, Finding, Fix, Severity } from "./types";
import { announce } from "../../shared-ui/Announcer";
import { logFailure, safeMessage } from "../../shared-ui/failure";
import { Button, EmptyState, Status } from "../../shared-ui/primitives";
import { InspectorPanel, InspectorSection, ToolbarActions } from "../../shell/chrome";

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
 * Düzeltmenin görünür yazımı.
 *
 * Boşluk düzeltmelerinde eski ve yeni değer yalnız boşluktan ibaret; HTML
 * boşlukları daralttığı için `<code>` kutuları bomboş çiziliyordu ve kullanıcı
 * "Önerilen düzeltme:  →  " diye bir şey okuyordu (gerçek pencerede görüldü).
 * Boşluklar görünür bir işaretle yazılır; metin değişmez.
 */
export function visibleText(text: string): string {
  if (text.length === 0) return "∅";
  return text.replace(/ /g, "·").replace(/\t/g, "⇥");
}

/**
 * Bulgunun işaretli aralığı.
 *
 * Motor `SourceLocation`'ı snake_case seri hâle getiriyor (`char_start`),
 * arayüz tipi ise camelCase yazıyor. Bu yüzden ofsetler pratikte `undefined`
 * geliyordu ve `slice(0, undefined)` üç kez tüm paragrafı döndürüyordu:
 * paketlenmiş uygulamada alıntı aynı cümleyi üç kez yazıyordu. Burada iki
 * yazım da okunur ve yalnız gerçek sayı çifti işaretlenir; ofset yoksa
 * paragraf işaretsiz gösterilir. Motor sözleşmesine dokunulmadı.
 */
function markedRange(location: Finding["location"]): [number, number] | null {
  const raw = location as unknown as Record<string, unknown>;
  const from = typeof raw.charStart === "number" ? raw.charStart : raw.char_start;
  const to = typeof raw.charEnd === "number" ? raw.charEnd : raw.char_end;
  if (typeof from !== "number" || typeof to !== "number") return null;
  return to > from ? [from, to] : null;
}

/**
 * Barın eylemi ve sağdaki denetim özeti.
 *
 * Yalnız bulgu varken vardır. Sıfır bulguda panel üç kez "0" yazan bir sütuna
 * dönüyor, boş-başarı durumunu ikiye bölüyordu; kip o durumda üçüncü kolonu
 * hiç açmaz. Sayı ve ciddiyet anlamları değişmedi — yalnız çizildikleri koşul.
 */
export function ReviewChrome({
  result,
  fixable,
  selected,
  onApply,
  onSelectAll,
  onClearSelection,
}: {
  result: AnalysisResult;
  fixable: Finding[];
  selected: Set<string>;
  onApply: () => void;
  onSelectAll: () => void;
  onClearSelection: () => void;
}) {
  if (result.findings.length === 0) return null;
  const doc = result.document;

  return (
    <>
      {fixable.length > 0 ? (
        <ToolbarActions>
          <Button variant="primary" onClick={onApply} disabled={selected.size === 0}>
            Kopyaya uygula…
          </Button>
        </ToolbarActions>
      ) : null}

      <InspectorPanel title="Denetim özeti">
        <div className="kv">
          <span className="kv-key">{SEVERITY.error.label}</span>
          <span className="kv-value">{result.errorCount}</span>
        </div>
        <div className="kv">
          <span className="kv-key">{SEVERITY.warning.label}</span>
          <span className="kv-value">{result.warningCount}</span>
        </div>
        <div className="kv">
          <span className="kv-key">{SEVERITY.review.label}</span>
          <span className="kv-value">{result.reviewCount}</span>
        </div>
        <p className="tool-hint">
          {doc.blockCount} paragraf · {doc.wordCount} kelime
          {result.profileName ? ` · ${result.profileName} profili` : ""}
        </p>
        {result.truncatedRules.length > 0 ? (
          <p className="tool-hint">
            {result.truncatedRules
              .map((t) => `${t.total} bulgunun ${t.shown} tanesi gösteriliyor`)
              .join(" · ")}
          </p>
        ) : null}

        {fixable.length > 0 ? (
          <InspectorSection title="Düzeltmeler">
            <p className="tool-count">
              {selected.size} / {fixable.length} düzeltme seçildi.
            </p>
            <div className="row">
              <Button className="btn-sm" onClick={onSelectAll} disabled={selected.size === fixable.length}>
                Tümünü seç
              </Button>
              <Button className="btn-sm" onClick={onClearSelection} disabled={selected.size === 0}>
                Seçimi kaldır
              </Button>
            </div>
            <p className="tool-hint">
              Düzeltmeler yeni bir kopyaya yazılır. Kaynak belgeniz değiştirilmez.
            </p>
          </InspectorSection>
        ) : null}
      </InspectorPanel>
    </>
  );
}

/**
 * Temiz belge: gösteri değil, sakin bir kapanış.
 *
 * Kutu, kart, ikon ya da başarı rengi yok. İncelemenin gerçekten yapıldığını
 * gösteren belge künyesi tek muted satır olarak burada durur; yalnız bu iki
 * sayı için sağda bir panel açmak yersizdi.
 */
export function ReviewClear({ doc }: { doc: AnalysisResult["document"] }) {
  return (
    <EmptyState
      title="Bulgu bulunmadı"
      note="Bu belge tanımlı kuralların hiçbirine takılmadı."
      meta={`${doc.blockCount} paragraf · ${doc.wordCount} kelime incelendi`}
    />
  );
}

/** Bulgunun kimliği — akordeon ve düzeltme seçimi bunu paylaşır. */
export const findingKey = (f: Finding) =>
  `${f.rule_id}:${f.block_id}:${f.location.charStart ?? -1}`;

/**
 * Bulgu listesi — satır içi akordeon.
 *
 * Kanıt bulgunun yanında durur: paragrafın tamamı, işaretli aralık vurgulu.
 * Saf sunum; durumu ve seçimi çağıran tutar.
 */
export function FindingList({
  findings,
  blockText,
  activeKey,
  selectedKeys,
  onSetActive,
  onToggleFix,
}: {
  findings: Finding[];
  blockText: Map<string, string>;
  activeKey: string | null;
  selectedKeys: Set<string>;
  onSetActive: (key: string | null) => void;
  onToggleFix: (finding: Finding) => void;
}) {
  return (
    <>
      <h2 className="section-head" id="bulgu-baslik">
        Bulgular
      </h2>
      <ul className="findings" role="list" aria-labelledby="bulgu-baslik">
        {findings.map((f) => {
          const k = findingKey(f);
          const sev = SEVERITY[f.severity];
          const isActive = activeKey === k;
          const text = blockText.get(f.block_id) ?? "";
          const range = markedRange(f.location);
          return (
            <li key={k} className="finding" data-severity={f.severity} data-active={isActive}>
              <button
                type="button"
                className="finding-head"
                aria-expanded={isActive}
                onClick={() => onSetActive(isActive ? null : k)}
              >
                <span className="finding-mark" aria-hidden="true">{sev.mark}</span>
                <span className="finding-sev">{sev.label}</span>
                <span className="finding-title">{f.title}</span>
                <span className="finding-loc">{f.block_id.replace(/^p/, "")}. paragraf</span>
              </button>
              {isActive ? (
                <div className="finding-body">
                  <p className="finding-message">{f.message}</p>
                  {/* Belgenin kendisi: bulgunun geçtiği paragrafın TAMAMI,
                      işaretli aralık vurgulu. Kırpılmış ±40 karakterlik
                      pencere, bulguyu bağlamından koparıyordu. */}
                  {text ? (
                    <p className="finding-excerpt selectable">
                      {range ? (
                        <>
                          {[...text].slice(0, range[0]).join("")}
                          <mark>{[...text].slice(range[0], range[1]).join("")}</mark>
                          {[...text].slice(range[1]).join("")}
                        </>
                      ) : (
                        text
                      )}
                    </p>
                  ) : null}
                  <p className="finding-why">{f.explanation}</p>
                  {f.fix ? (
                    <label className="finding-fix">
                      <input
                        type="checkbox"
                        checked={selectedKeys.has(k)}
                        onChange={() => onToggleFix(f)}
                      />
                      <span>
                        Önerilen düzeltme: <code>{visibleText(f.fix.original)}</code> →{" "}
                        <code>{visibleText(f.fix.replacement)}</code>
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
    </>
  );
}

/**
 * Denetle — belge incelemesi ve cerrahi düzeltme.
 *
 * Zincir: belge açıldı → incele → bulgular → konum → açıklama → önerilen
 * düzeltme → uygula. Düzeltmeler **kopyaya** yazılır; kaynak belge hiçbir
 * zaman yazmak için açılmaz.
 *
 * Bulgular satır içi akordeondur ve öyle kalır: kanıt — belgeden alınan
 * paragraf — bulgunun yanında durmalı, yan panele taşınırsa ikisi ayrılır.
 * Sağ panel yalnız özeti ve düzeltme seçimini taşır; birincil eylem barda.
 * Bulgu yoksa panel de yoktur: gerçek bağlam olmadan üçüncü kolon açılmaz.
 *
 * Parser ve migration ayrıntıları kullanıcıya gösterilmez.
 */
export function ReviewWorkspace({ path }: { path: string }) {
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [active, setActive] = useState<string | null>(null);
  const [written, setWritten] = useState<string | null>(null);

  const key = findingKey;

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
      .catch((e) => {
        if (cancelled) return;
        // Motor kullanıcı cümlesini kendisi üretiyor; teknik bir şey kalırsa
        // kullanıcıya yedek cümle gider, ham metin log'da durur.
        logFailure("ikincigoz analyze failure", e);
        setFailure(safeMessage(e, "Belge incelenemedi. Farklı bir dosya seçip yeniden deneyin."));
      })
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
      const k = findingKey(f);
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
      logFailure("ikincigoz writeback failure", e);
      setFailure(safeMessage(e, "Düzeltmeler uygulanamadı. Yeniden deneyin."));
    }
  }, [chosen, path]);

  if (busy) {
    return (
      <div className="surface">
        <Status tone="busy">Belge inceleniyor…</Status>
      </div>
    );
  }

  if (failure) {
    return (
      <div className="surface">
        <Status tone="error">{failure}</Status>
      </div>
    );
  }

  if (!result) return null;

  return (
    <>
      <ReviewChrome
        result={result}
        fixable={fixable}
        selected={selected}
        onApply={applyChosen}
        onSelectAll={() => setSelected(new Set(fixable.map(key)))}
        onClearSelection={() => setSelected(new Set())}
      />

      <div className="surface review">
        {written ? <Status tone="success">{written}</Status> : null}

        {result.findings.length === 0 ? (
          <ReviewClear doc={result.document} />
        ) : (
          <FindingList
            findings={result.findings}
            blockText={blockText}
            activeKey={active}
            selectedKeys={selected}
            onSetActive={setActive}
            onToggleFix={toggle}
          />
        )}
      </div>
    </>
  );
}
