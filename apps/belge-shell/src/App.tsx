import { useCallback, useEffect, useMemo, useState } from "react";
import { Layout } from "./shell/Layout";
import { SettingsSheet } from "./shell/Settings";
import { featureForRoute, firstAvailableRoute, resolveRoute } from "./shell/routes";
import type { FeatureState, Settings } from "./shell/types";
import * as api from "./shell/api";
import { applyPreferences } from "./shared-ui/theme";
import { DocumentSurface } from "./features/DocumentSurface";

/** Bölüm başlıkları. Kullanıcı eylem adını görür, ürün adını değil. */
const HEADINGS: Record<FeatureState["key"], { title: string; subtitle: string }> = {
  duzenek: { title: "Düzenle", subtitle: "Dilekçe eklerini hazırlayın" },
  tavzih: { title: "Dönüştür", subtitle: "Word ve UYAP belgeleri arasında" },
  degisikis: { title: "Karşılaştır", subtitle: "İki belge arasındaki değişiklikler" },
  ikincigoz: { title: "Denetle", subtitle: "Göndermeden önce" },
};

export function App() {
  const [features, setFeatures] = useState<FeatureState[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [route, setRoute] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        // Eski ayar taşıması açılışta bir kez. Sonucu KULLANICIYA GÖSTERİLMEZ:
        // hangi eski uygulamadan ne okunduğu bir geliştirme ayrıntısıdır.
        // Yeniden çalıştırmak güvenlidir; hiçbir değer ezilmez.
        const report = await api.migrateLegacySettings().catch(() => null);
        const [f, s, info] = await Promise.all([
          api.enabledFeatures(),
          api.getSettings(),
          api.appInfo(),
        ]);
        if (cancelled) return;
        console.debug("[belge] app", info);
        console.debug("[belge] features", f);
        if (report) console.debug("[belge] legacy settings migration", report);
        setFeatures(f);
        setSettings(s);
        setRoute(firstAvailableRoute(f));
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
    (next: string) => setRoute((current) => resolveRoute(next, features) || current),
    [features],
  );

  const saveSettings = useCallback(async (next: Settings) => {
    setSettings(next);
    setSettings(await api.saveSettings(next));
  }, []);

  const openDocuments = useCallback(async (paths: string[]) => {
    // Phase 2: kabuk belgeyi henüz bir motora vermiyor; yalnız hatırlıyor.
    // Motor bağlantısı ilgili modül taşınırken eklenir.
    const next = await api.rememberDocuments(paths);
    setSettings(next);
    console.debug("[belge] documents opened", paths);
  }, []);

  const forgetDocuments = useCallback(async () => {
    setSettings(await api.forgetDocuments());
  }, []);

  const active = featureForRoute(route, features);
  const heading = useMemo(
    () => (active ? HEADINGS[active.key] : null),
    [active],
  );

  if (error) {
    return (
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={() => setShowSettings(true)}
        title="Yüksekbaş Belge"
      >
        <p className="notice" data-tone="error" role="alert">
          Uygulama başlatılamadı. Lütfen yeniden açmayı deneyin.
        </p>
      </Layout>
    );
  }

  return (
    <>
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={() => setShowSettings(true)}
        title={heading?.title ?? "Yüksekbaş Belge"}
        subtitle={heading?.subtitle}
      >
        {active && settings ? (
          <DocumentSurface
            feature={active}
            recents={settings.recentDocuments}
            onDocuments={openDocuments}
            onForget={forgetDocuments}
          />
        ) : null}
      </Layout>
      {showSettings && settings ? (
        <SettingsSheet
          settings={settings}
          onChange={saveSettings}
          onClose={() => setShowSettings(false)}
        />
      ) : null}
    </>
  );
}
