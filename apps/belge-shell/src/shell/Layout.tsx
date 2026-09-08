import type { ReactNode } from "react";
import type { FeatureState } from "./types";
import { Sidebar } from "./Sidebar";
import { Announcer } from "../shared-ui/Announcer";

interface Props {
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
  onOpenSettings: () => void;
  /** Açık belgeler — kenar çubuğunda bağlamı görünür tutar. */
  openDocuments: string[];
  toolbar: ReactNode;
  children: ReactNode;
}

/**
 * Pencere iskeleti.
 *
 * Ayrı bir "sayfa başlığı" bloğu YOK. Başlık toolbar'ın içindedir: dikey alan
 * belgeye aittir, kabuğa değil. Bu, envanterde bulunan ilk gereksiz katmandı.
 */
export function Layout({
  features,
  current,
  onNavigate,
  onOpenSettings,
  openDocuments,
  toolbar,
  children,
}: Props) {
  return (
    <div className="shell">
      <a className="skip-link" href="#icerik">İçeriğe geç</a>
      <Sidebar
        features={features}
        current={current}
        onNavigate={onNavigate}
        onOpenSettings={onOpenSettings}
        openDocuments={openDocuments}
      />
      <section className="content">
        {toolbar}
        <main id="icerik" className="content-body" tabIndex={-1}>
          {children}
        </main>
      </section>
      <Announcer />
    </div>
  );
}
