import type { ReactNode } from "react";
import type { AppInfo, FeatureState } from "./types";
import { Nav } from "./Nav";
import { Announcer } from "../shared-ui/Announcer";

interface Props {
  info: AppInfo | null;
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
  children: ReactNode;
}

export function Layout({ info, features, current, onNavigate, children }: Props) {
  const ready = features.filter((f) => f.enabled).length;
  return (
    <div className="shell">
      <a className="skip-link" href="#icerik">İçeriğe geç</a>
      <Nav features={features} current={current} onNavigate={onNavigate} />
      <main id="icerik" className="shell-main" tabIndex={-1}>
        {children}
      </main>
      <footer className="shell-footer">
        <span>
          {info ? `${info.name} ${info.version}` : "Yükleniyor…"}
          {info?.nameIsProvisional ? " · geçici ad" : ""}
        </span>
        <span>{ready} / {features.length} bölüm kullanılabilir</span>
      </footer>
      <Announcer />
    </div>
  );
}
