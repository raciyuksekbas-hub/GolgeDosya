import type { ReactNode } from "react";
import type { FeatureState } from "./types";
import { Sidebar } from "./Sidebar";
import { Announcer } from "../shared-ui/Announcer";

interface Props {
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
  onOpenSettings: () => void;
  title: string;
  subtitle?: string;
  children: ReactNode;
}

export function Layout({
  features,
  current,
  onNavigate,
  onOpenSettings,
  title,
  subtitle,
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
      />
      <section className="content">
        <header className="content-header">
          <h1>{title}</h1>
          {subtitle ? <p>{subtitle}</p> : null}
        </header>
        <main id="icerik" className="content-body" tabIndex={-1}>
          {children}
        </main>
      </section>
      <Announcer />
    </div>
  );
}
