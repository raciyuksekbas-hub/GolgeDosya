import { useRef } from "react";
import type { FilledRef, NullableRef } from "./reactCompat";
import type { ComparisonRow, DiffFragment, LocalDocument } from "./core/types";
import { missingBlockLabel } from "./uiLabels";
import type { ComparisonChange } from "./viewModels/comparisonViewModel";
import { DOCUMENT_FILE_ACCEPT, DOCUMENT_FILE_TYPES_LABEL } from "./fileSupport";
import { Icon } from "./Icon";

export type Side = "base" | "revised";

export const SIDE_LABEL: Record<Side, string> = { base: "Temel Sürüm", revised: "Değişik Sürüm" };
// Türkçe büyük harf dönüşümü tarayıcıya bırakılmaz; mikro etiketler sabit yazılır.
export const SIDE_CAPS: Record<Side, string> = { base: "TEMEL SÜRÜM", revised: "DEĞİŞİK SÜRÜM" };

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UploadZone({ side, loading, error, dropTarget, onFiles }: {
  side: Side;
  loading: boolean;
  error?: string;
  /** Sürükleme sırasında bu panelin hedef olduğunu gösterir. */
  dropTarget: boolean;
  onFiles: (files: File[]) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <div className={`upload-zone ${dropTarget ? "is-dragging" : ""} ${error ? "has-error" : ""}`}>
      {/* Tek seferde iki belge seçilebilir; ilki Temel, ikincisi Değişik olur. */}
      <input
        ref={inputRef}
        type="file"
        accept={DOCUMENT_FILE_ACCEPT}
        multiple
        hidden
        onChange={(event) => {
          const files = Array.from(event.target.files ?? []);
          event.target.value = "";
          if (files.length) onFiles(files);
        }}
      />
      <div className="upload-icon"><Icon name="upload" size={22} /></div>
      <h2>{loading ? "Belge hazırlanıyor…" : dropTarget ? `${SIDE_LABEL[side]} olarak bırak` : `${SIDE_LABEL[side]} dosyasını seçin`}</h2>
      <p>{loading ? "Metin yerel olarak çıkarılıyor" : "veya buraya bırakın"}</p>
      <button className="button primary" onClick={() => inputRef.current?.click()} disabled={loading}>Dosya Seç</button>
      <span className="file-types">{DOCUMENT_FILE_TYPES_LABEL}</span>
      <span className="upload-hint">İki belgeyi birlikte seçebilir veya sürükleyebilirsiniz.</span>
      {error && <div className="inline-error" role="alert">{error}</div>}
    </div>
  );
}

export function PaneHeader({ doc, side, onReplace }: {
  doc?: LocalDocument;
  side: Side;
  onReplace: (file: File) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <header className="pane-header">
      <span className="pane-side">{SIDE_CAPS[side]}</span>
      {doc ? (
        <>
          <span className="pane-file"><Icon name="file" size={13} /><strong title={doc.name}>{doc.name}</strong></span>
          <span className="pane-meta">{doc.extension.toLocaleUpperCase("tr-TR")} · {formatSize(doc.size)} · {doc.blocks.length} bölüm</span>
          <input ref={inputRef} type="file" accept={DOCUMENT_FILE_ACCEPT} hidden onChange={(event) => event.target.files?.[0] && onReplace(event.target.files[0])} />
          <button className="text-button" onClick={() => inputRef.current?.click()}>Değiştir</button>
        </>
      ) : (
        <span className="pane-meta">Belge seçilmedi</span>
      )}
    </header>
  );
}

/**
 * Eklenen ve çıkarılan sözcükler ANLAMLI etiketlerle çizilir.
 *
 * Görünüş değişmez: sınıflar aynı kaldığı için `.fragment.added` /
 * `.fragment.removed` kuralları olduğu gibi yürür. Değişen, etiketin ekran
 * okuyucuya ne söylediğidir — `<span>` "bu bir sözcük" der, `<ins>` ve
 * `<del>` "bu sözcük eklendi / çıkarıldı" der. Görsel ipuçları da renge
 * bağlı değil: çıkarılan üstü çizili, eklenenin altında çizgi var.
 */
function FragmentText({ fragments }: { fragments: DiffFragment[] }) {
  return (
    <>
      {fragments.map((fragment, index) => {
        const Tag = fragment.kind === "added" ? "ins" : fragment.kind === "removed" ? "del" : "span";
        return (
          <Tag key={`${fragment.kind}-${index}`} className={`fragment ${fragment.kind}`}>
            {fragment.text}{index < fragments.length - 1 ? " " : ""}
          </Tag>
        );
      })}
    </>
  );
}

// `doc` sözleşmenin parçası ama bu gövdede okunmuyor; `_doc` takma adı
// TypeScript'in "bilerek kullanılmıyor" konvansiyonu. Prop adı değişmedi.
export function DocumentPane({ side, doc: _doc, rows, changes, selectedRows, paneRef, rowRefs, onScroll }: {
  side: Side;
  doc: LocalDocument;
  rows: ComparisonRow[];
  changes: ComparisonChange[];
  selectedRows: number[];
  paneRef: NullableRef<HTMLDivElement>;
  rowRefs: FilledRef<Map<number, HTMLDivElement>>;
  onScroll: () => void;
}) {
  const selectedRowSet = new Set(selectedRows);
  const groupByRow = new Map<number, ComparisonChange>();
  changes.forEach((change) => change.rowIndices.forEach((rowIndex) => groupByRow.set(rowIndex, change)));
  return (
    <div className="document-scroll" ref={paneRef} onScroll={onScroll}>
      <article className="paper">
        {rows.map((row, index) => {
          const block = side === "base" ? row.base : row.revised;
          const fragments = side === "base" ? row.baseFragments : row.revisedFragments;
          const missingAdded = side === "base" && row.kind === "added";
          const missingRemoved = side === "revised" && row.kind === "removed";
          const selected = selectedRowSet.has(index);
          const group = groupByRow.get(index);
          const groupRows = group?.rowIndices ?? [];
          const groupedContinuation = (missingAdded || missingRemoved) && groupRows.length > 1 && groupRows[0] !== index;
          return (
            <div
              key={`${side}-${row.id}`}
              ref={(node) => { if (node) rowRefs.current.set(index, node); else rowRefs.current.delete(index); }}
              className={`document-row kind-${row.kind} ${selected ? "is-selected" : ""} ${groupedContinuation ? "grouped-placeholder-continuation" : ""} block-${block?.kind ?? "placeholder"}`}
              data-row={index}
            >
              <div className="row-gutter">{block?.label ? "§" : index + 1}</div>
              <div className="row-content">
                {/* Satır yüksekliği senkronizasyonunun ölçüm hedefi; bu öğeye
                    hiçbir zaman yükseklik yazılmaz. */}
                <div className="row-body">
                  {(missingAdded || missingRemoved) && !groupedContinuation && (
                    <span className="placeholder-copy">{missingBlockLabel(row, side)}</span>
                  )}
                  {block && <FragmentText fragments={fragments} />}
                </div>
              </div>
            </div>
          );
        })}
      </article>
    </div>
  );
}
