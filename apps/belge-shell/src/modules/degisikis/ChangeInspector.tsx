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

function ChangeListRow({ change, active, onClick }: { change: ComparisonChange; active: boolean; onClick: () => void }) {
  return (
    // Seçili satır yalnız renkle belliydi. Ray (`rail-node`) bunu zaten
    // `aria-current` ile söylüyordu; liste söylemiyordu. Aynı bilgi, aynı yol.
    <button
      className={`change-row ${change.kind} ${active ? "is-active" : ""}`}
      aria-current={active || undefined}
      onClick={onClick}
    >
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
        <span>Eski</span>
        <p>{change.leftText ?? "—"}</p>
      </div>
      <div className="detail-block added">
        <span>Yeni</span>
        <p>{change.rightText ?? "—"}</p>
      </div>
      {/* Özet satırları burada TEKRARLANMAZ: aynı üç satır hemen aşağıdaki
          listede de yazılıyordu ve gerçek pencerede iki kez okunuyordu. Burada
          sürümlerin tam metni var — özetten fazlası. */}
    </section>
  );
}

/**
 * Fark paneli — kabuğun sağ panelinin İÇERİĞİ.
 *
 * Kendi `<aside>`'ı, başlığı ve kapatma düğmesi yoktur: panelin çerçevesini,
 * başlığını ve göster/gizle düğmesini kabuk sahiplenir (`shell/chrome.tsx`,
 * yardımcı bardaki "Ayrıntılar"). İki kapatma düğmesi ve iki başlık, aynı işi
 * iki kez çizmekti.
 *
 * Oran halkası kaldırıldı: 300 px'lik bir panelde üç sayı ve üç yüzde, bir
 * grafikten daha hızlı okunur ve ürünün geri kalanıyla aynı dili konuşur.
 * Sayılar view model'den gelir; burada yeniden hesaplanmaz.
 */
export function ChangeInspector({ summary, filter, onFilterChange, filteredChanges, selectedChange, onSelect }: {
  summary?: ComparisonSummary;
  filter: ChangeFilter;
  onFilterChange: (filter: ChangeFilter) => void;
  filteredChanges: ComparisonChange[];
  selectedChange?: ComparisonChange;
  onSelect: (change: ComparisonChange) => void;
}) {
  // Fark yoksa özet, filtre ve liste yazacak bir şey bulamaz: tek satır kalır.
  if (!summary || summary.total === 0) {
    return <p className="inspector-total">Fark bulunmadı</p>;
  }

  const rows: Array<{ kind: ComparisonChange["kind"]; count: number }> = [
    { kind: "added", count: summary.added },
    { kind: "removed", count: summary.removed },
    { kind: "modified", count: summary.modified },
  ];

  return (
    <>
      {/* Toplam ayrıca yazılmaz: üç sayının kendisi zaten toplamı veriyor ve
          rayın altındaki konum göstergesi ("2/4") toplamı da taşıyor. */}
      <ul className="summary-legend">
        {rows.map((row) => (
          <li key={row.kind} className={row.kind}>
            <i aria-hidden="true" />
            <span className="legend-label">{KIND_LABEL[row.kind]}</span>
            <span className="legend-count">{row.count}</span>
          </li>
        ))}
      </ul>

      <div className="inspector-section-head">
        <h3 className="inspector-label">Seçili fark</h3>
        {filter !== "all" && <span className="filter-note">{filteredChanges.length} / {summary.total} gösteriliyor</span>}
      </div>

      {selectedChange ? <SelectedDetail change={selectedChange} /> : null}

      <div className="filters" role="group" aria-label="Değişiklik filtresi">
        {/* `aria-pressed`: yürürlükteki filtre yalnız renkle belliydi; ekran
            okuyucu dört özdeş düğme okuyor, hangisinin açık olduğunu
            söylemiyordu. */}
        {(Object.keys(FILTER_LABELS) as ChangeFilter[]).map((value) => (
          <button key={value} className={filter === value ? "is-active" : ""} aria-pressed={filter === value} onClick={() => onFilterChange(value)}>
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

    </>
  );
}
