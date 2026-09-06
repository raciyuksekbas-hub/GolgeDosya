import type { AppInfo, FeatureState, MigrationReport } from "../shell/types";

interface Props {
  info: AppInfo | null;
  features: FeatureState[];
  migration: MigrationReport | null;
}

function statusText(status: MigrationReport["sources"][number]["status"]): string {
  if (status === "notFound") return "bu makinede bulunamadı";
  if (status === "nothingToDo") return "aktarılacak yeni bir şey yok";
  if ("migrated" in status) return `aktarıldı: ${status.migrated.fields.join(", ")}`;
  if ("unreadable" in status) return `okunamadı — ${status.unreadable.detail}`;
  return status.needsManualStep.detail;
}

export function Home({ info, features, migration }: Props) {
  return (
    <section className="placeholder" aria-labelledby="ev-baslik">
      <h1 id="ev-baslik">Yüksekbaş Belge</h1>
      <p>
        Düzenleme, dönüştürme, karşılaştırma ve denetim işlerini tek yerde toplayan
        yerel belge çalışma ortamı. Tüm işlemler bu bilgisayarda yapılır.
      </p>

      <h2 style={{ fontSize: "1.05em" }}>Bölümler</h2>
      <dl>
        {features.map((f) => (
          <div key={f.key}>
            <dt>{f.label}</dt>
            <dd>{f.enabled ? "kullanılabilir" : "henüz taşınmadı"}</dd>
          </div>
        ))}
      </dl>

      {migration && (
        <>
          <h2 style={{ fontSize: "1.05em" }}>Önceki uygulamalardan ayarlar</h2>
          <dl>
            {migration.sources.map((s) => (
              <div key={s.identifier}>
                <dt>{s.app}</dt>
                <dd>{statusText(s.status)}</dd>
              </div>
            ))}
          </dl>
          <p style={{ color: "var(--text-muted)" }}>
            Önceki uygulamaların ayar klasörleri olduğu gibi bırakıldı; hiçbir dosya
            silinmedi veya taşınmadı.
          </p>
        </>
      )}

      {info && (
        <p style={{ color: "var(--text-muted)" }}>
          Ayarlar: <code>{info.configDir}</code>
        </p>
      )}
    </section>
  );
}
