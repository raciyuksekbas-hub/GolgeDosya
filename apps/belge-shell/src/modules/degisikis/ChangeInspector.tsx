import { Icon } from "./Icon";
import type { ChangeFilter, ComparisonChange, ComparisonSummary } from "./viewModels/comparisonViewModel";

export const FILTER_LABELS: Record<ChangeFilter, string> = {
  all: "Tümü",
  added: "Eklenen",
  removed: "Silinen",
  modified: "Değiştirilen",
};

const KIND_LABEL: Record<ComparisonChange["kind"], string> = {
  added: "Eklenen",
  removed: "Silinen",
  modified: "Değiştirilen",
};

/**
 * Üç dilimli oran halkası. Ek bağımlılık kullanmaz: tek bir SVG çemberi
 * üzerinde stroke-dasharray ile dilimlenir. Yüzdeler view model'den gelir,
 * burada yeniden hesaplanmaz.
 */
function Donut({ summary }: { summary: ComparisonSummary }) {
  const radius = 30;
  const circumference = 2 * Math.PI * radius;
  const segments: Array<{ kind: ComparisonChange["kind"]; pct: number }> = [
    { kind: "added", pct: summary.addedPct },
    { kind: "removed", pct: summary.removedPct },
    { kind: "modified", pct: summary.modifiedPct },
  ];
  let consumed = 0;
  return (
    <svg className="donut" viewBox="0 0 80 80" role="img" aria-label={`Toplam ${summary.total} fark`}>
      <circle className="donut-track" cx="40" cy="40" r={radius} />
      {segments.map((segment) => {
        const length = (segment.pct / 100) * circumference;
        const offset = -(consumed / 100) * circumference;
        consumed += segment.pct;
        if (length <= 0) return null;
        return (
          <circle
            key={segment.kind}
            className={`donut-segment ${segment.kind}`}
            cx="40" cy="40" r={radius}
            strokeDasharray={`${length.toFixed(2)} ${(circumference - length).toFixed(2)}`}
            strokeDashoffset={offset.toFixed(2)}
          />
        );
      })}
      <text className="donut-value" x="40" y="39">{summary.total}</text>
      <text className="donut-label" x="40" y="51">TOPLAM</text>
    </svg>
  );
}

function ChangeListRow({ change, active, onClick }: { change: ComparisonChange; active: boolean; onClick: () => void }) {
  return (
    <button className={`change-row ${change.kind} ${active ? "is-active" : ""}`} onClick={onClick}>
      <span className="change-row-top">
        <span className="change-no">{change.displayIndex}</span>
        <span className="change-type"><i />{KIND_LABEL[change.kind]}</span>
        <span className="change-location">{change.sectionLabel}</span>
      </span>
      <span className="change-summary">
        {change.summary.map((line, index) => (
          <span className={/^\+ \d+ değişiklik daha$/u.test(line) ? "change-summary-more" : undefined} key={index}>{line}</span>
        ))}
      </span>
    </button>
  );
}

function SelectedDetail({ change }: { change: ComparisonChange }) {
  return (
    <section className={`inspector-detail ${change.kind}`}>
      <header>
        <span className="change-no">{change.displayIndex}</span>
        <span className="change-type"><i />{KIND_LABEL[change.kind]}</span>
        <span className="change-location">{change.sectionLabel}</span>
      </header>
      {change.structural && (
        <p className="detail-note">
          {change.structural.articleLabel} bütünüyle {change.kind === "added" ? "eklendi" : "silindi"}
          {change.structural.contentCount > 0 ? ` · ${change.structural.contentCount} bölüm` : ""}.
        </p>
      )}
      <div className="detail-block removed">
        <span>TEMEL SÜRÜM</span>
        <p>{change.leftText ?? "—"}</p>
      </div>
      <div className="detail-block added">
        <span>DEĞİŞİK SÜRÜM</span>
        <p>{change.rightText ?? "—"}</p>
      </div>
      {change.summary.length > 0 && (
        <div className="detail-summary">
          {change.summary.map((line, index) => <span key={index}>{line}</span>)}
        </div>
      )}
    </section>
  );
}

export function ChangeInspector({ summary, changes, filter, onFilterChange, filteredChanges, selectedChange, onSelect, onClose }: {
  summary?: ComparisonSummary;
  changes: ComparisonChange[];
  filter: ChangeFilter;
  onFilterChange: (filter: ChangeFilter) => void;
  filteredChanges: ComparisonChange[];
  selectedChange?: ComparisonChange;
  onSelect: (change: ComparisonChange) => void;
  onClose: () => void;
}) {
  const closeButton = (
    <button className="inspector-close" onClick={onClose} aria-label="Özet panelini kapat" title="Özet panelini kapat">
      <Icon name="x" size={14} />
    </button>
  );
  if (!summary) {
    return (
      <aside className="inspector">
        <header className="inspector-head"><h2>ÖZET</h2>{closeButton}</header>
        <div className="inspector-empty">
          <div className="empty-lines" aria-hidden="true"><span /><span /><span /></div>
          <strong>Değişiklikler burada listelenir</strong>
          <p>Karşılaştırmak için iki belge ekleyin.</p>
        </div>
      </aside>
    );
  }

  const rows: Array<{ kind: ComparisonChange["kind"]; count: number; pct: number }> = [
    { kind: "added", count: summary.added, pct: summary.addedPct },
    { kind: "removed", count: summary.removed, pct: summary.removedPct },
    { kind: "modified", count: summary.modified, pct: summary.modifiedPct },
  ];

  return (
    <aside className="inspector">
      <header className="inspector-head">
        <h2>ÖZET</h2>
        <span className="inspector-total"><strong>{summary.total}</strong> toplam fark</span>
        {closeButton}
      </header>

      <div className="inspector-summary">
        <Donut summary={summary} />
        <ul className="summary-legend">
          {rows.map((row) => (
            <li key={row.kind} className={row.kind}>
              <i aria-hidden="true" />
              <span className="legend-label">{KIND_LABEL[row.kind]}</span>
              <span className="legend-count">{row.count}</span>
              <span className="legend-pct">%{row.pct}</span>
            </li>
          ))}
        </ul>
      </div>

      <div className="inspector-section-head">
        <h3>SEÇİLİ FARK</h3>
        {filter !== "all" && <span className="filter-note">{filteredChanges.length} / {summary.total} gösteriliyor</span>}
      </div>

      {selectedChange
        ? <SelectedDetail change={selectedChange} />
        : <p className="detail-idle">Rayda ya da listede bir fark seçin.</p>}

      <div className="filters" role="group" aria-label="Değişiklik filtresi">
        {(Object.keys(FILTER_LABELS) as ChangeFilter[]).map((value) => (
          <button key={value} className={filter === value ? "is-active" : ""} onClick={() => onFilterChange(value)}>
            {FILTER_LABELS[value]}
          </button>
        ))}
      </div>

      <div className="change-list">
        {filteredChanges.length
          ? filteredChanges.map((change) => (
            <ChangeListRow key={change.id} change={change} active={selectedChange?.id === change.id} onClick={() => onSelect(change)} />
          ))
          : <div className="filter-empty">Bu filtrede değişiklik yok.</div>}
      </div>

      <footer className="inspector-hint">
        <kbd>↑</kbd><kbd>↓</kbd><span>{changes.length} fark arasında gezin</span>
      </footer>
    </aside>
  );
}
