import type { FeatureState } from "./types";
import { announce } from "../shared-ui/Announcer";

interface Props {
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
}

/**
 * Ana navigasyon. Kullanıcıya eylem adları gösterilir; eski ürün adları
 * yalnız `data-feature` özniteliğinde (log/debug için) kalır.
 *
 * Taşınmamış bir modül gizlenmez, **devre dışı gösterilir**: kullanıcı ürünün
 * neyi kapsadığını görür, neyin henüz gelmediğini de.
 */
export function Nav({ features, current, onNavigate }: Props) {
  return (
    <nav className="shell-nav" aria-label="Ana bölümler">
      {features.map((f) => {
        const active = current === f.route;
        return (
          <button
            key={f.key}
            type="button"
            data-feature={f.key}
            aria-current={active ? "page" : undefined}
            disabled={!f.enabled}
            title={f.enabled ? undefined : `${f.label} bu sürümde henüz kullanılamıyor.`}
            onClick={() => {
              onNavigate(f.route);
              announce(`${f.label} bölümü açıldı.`);
            }}
          >
            {f.label}
          </button>
        );
      })}
    </nav>
  );
}
