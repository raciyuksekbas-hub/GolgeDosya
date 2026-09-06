import { useCallback, useEffect, useState } from "react";
import { Layout } from "./shell/Layout";
import { DEFAULT_ROUTE, featureForRoute, resolveRoute } from "./shell/routes";
import type { AppInfo, FeatureState, MigrationReport, Settings } from "./shell/types";
import * as api from "./shell/api";
import { applyPreferences } from "./shared-ui/theme";
import { Home } from "./features/Home";
import { Pending } from "./features/Pending";

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [features, setFeatures] = useState<FeatureState[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [migration, setMigration] = useState<MigrationReport | null>(null);
  const [route, setRoute] = useState(DEFAULT_ROUTE);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        // Migration açılışta bir kez, ayarlar okunmadan ÖNCE. Yeniden
        // çalıştırmak güvenli: hiçbir değer ezilmez.
        const report = await api.migrateLegacySettings().catch(() => null);
        const [i, f, s] = await Promise.all([
          api.appInfo(),
          api.enabledFeatures(),
          api.getSettings(),
        ]);
        if (cancelled) return;
        setMigration(report);
        setInfo(i);
        setFeatures(f);
        setSettings(s);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!settings) return;
    return applyPreferences(settings);
  }, [settings]);

  const navigate = useCallback(
    (next: string) => setRoute(resolveRoute(next, features)),
    [features],
  );

  // Bir modül çalışma zamanında kapatılırsa kullanıcı o rotada kalmamalı.
  useEffect(() => {
    setRoute((current) => resolveRoute(current, features));
  }, [features]);

  const active = featureForRoute(route, features);

  return (
    <Layout info={info} features={features} current={route} onNavigate={navigate}>
      {error ? (
        <section className="placeholder" role="alert">
          <h1>Uygulama başlatılamadı</h1>
          <p>{error}</p>
        </section>
      ) : active ? (
        <Pending feature={active} />
      ) : (
        <Home info={info} features={features} migration={migration} />
      )}
    </Layout>
  );
}
