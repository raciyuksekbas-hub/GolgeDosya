import { useCallback, useEffect, useMemo, useState } from "react";
import { Layout } from "./shell/Layout";
import { SettingsSheet } from "./shell/Settings";
import { featureForRoute, firstAvailableRoute, resolveRoute } from "./shell/routes";
import type { FeatureState, Settings } from "./shell/types";
import * as api from "./shell/api";
import { applyPreferences } from "./shared-ui/theme";
import { DocumentSurface } from "./features/DocumentSurface";
import { ConvertWorkspace } from "./modules/tavzih/ConvertWorkspace";
import { ReviewWorkspace } from "./modules/ikincigoz/ReviewWorkspace";
import { CompareWorkspace } from "./modules/degisikis/CompareWorkspace";
import { PdfWorkspace } from "./modules/duzenek/PdfWorkspace";
import { MODES, carryContext, fileNameOf, type ContextOutcome } from "./shell/modes";
import { Toolbar, ToolbarTitle, ToolbarSpacer, Button, Status } from "./shared-ui/primitives";
import { announce } from "./shared-ui/Announcer";
import { useShortcuts } from "./shared-ui/useShortcuts";

export function App() {
  const [features, setFeatures] = useState<FeatureState[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [route, setRoute] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  /**
   * Açık belgeler — kipe değil ORTAMA aittir.
   *
   * Ürünün en önemli davranışı: belge bir kez açılır, kipler arasında gezerken
   * yeniden seçilmez. Eskiden her gezinmede temizleniyordu.
   */
  const [documents, setDocuments] = useState<string[]>([]);
  const [context, setContext] = useState<ContextOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** ⌘O sayacı — belge yüzeyine seçiciyi açması için verilen işaret. */
  const [openRequest, setOpenRequest] = useState(0);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        // Eski ayar taşıması açılışta bir kez. Sonucu KULLANICIYA GÖSTERİLMEZ.
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

  const active = featureForRoute(route, features);

  const navigate = useCallback(
    (next: string) =>
      setRoute((current) => {
        const resolved = resolveRoute(next, features) || current;
        if (resolved === current) return current;
        const target = features.find((f) => f.route === resolved);
        if (target) {
          // Belge taşınır; yalnız tür uyuşmuyorsa açıkça anlatılır.
          setContext(carryContext(target.key, documents));
        }
        return resolved;
      }),
    [features, documents],
  );

  const saveSettings = useCallback(async (next: Settings) => {
    setSettings(next);
    setSettings(await api.saveSettings(next));
  }, []);

  const openDocuments = useCallback(
    async (paths: string[]) => {
      setDocuments(paths);
      if (active) setContext(carryContext(active.key, paths));
      setSettings(await api.rememberDocuments(paths));
    },
    [active],
  );

  const closeDocuments = useCallback(() => {
    setDocuments([]);
    setContext(null);
    announce("Belge kapatıldı.");
  }, []);

  const forgetDocuments = useCallback(async () => {
    setSettings(await api.forgetDocuments());
  }, []);

  // macOS'un öğrettiği iki kısayol. Sheet açıkken kapalı: odak tuzağının
  // içinden arka plandaki eylemi tetiklemek odak modelini bozar.
  useShortcuts(
    useMemo(
      () => [
        { key: "o", run: () => setOpenRequest((n) => n + 1), enabled: !showSettings },
        { key: ",", run: () => setShowSettings(true) },
      ],
      [showSettings],
    ),
  );

  const mode = active ? MODES[active.key] : null;

  /** Bu kip, açık belgelerle şu an çalışabiliyor mu? */
  const usable = useMemo(() => {
    if (!active || documents.length === 0) return false;
    const outcome = context ?? carryContext(active.key, documents);
    return outcome.kind === "keep";
  }, [active, documents, context]);

  const subtitle = useMemo(() => {
    if (!mode) return undefined;
    if (documents.length === 0) return mode.purpose;
    if (documents.length === 1) return fileNameOf(documents[0]);
    return documents.map(fileNameOf).join("  ·  ");
  }, [mode, documents]);

  if (error) {
    return (
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={() => setShowSettings(true)}
        openDocuments={[]}
        toolbar={
          <Toolbar>
            <ToolbarTitle title="Yüksekbaş Belge" />
          </Toolbar>
        }
      >
        <div className="surface">
          <Status tone="error">Uygulama başlatılamadı. Lütfen yeniden açmayı deneyin.</Status>
        </div>
      </Layout>
    );
  }

  const outcome = active && documents.length > 0 ? context ?? carryContext(active.key, documents) : null;

  return (
    <>
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={() => setShowSettings(true)}
        openDocuments={documents}
        toolbar={
          <Toolbar>
            <ToolbarTitle title={mode?.label ?? "Yüksekbaş Belge"} subtitle={subtitle} />
            <ToolbarSpacer />
            {documents.length > 0 ? (
              <Button variant="quiet" onClick={closeDocuments}>
                Kapat
              </Button>
            ) : null}
          </Toolbar>
        }
      >
        {active && settings ? (
          usable && active.key === "tavzih" ? (
            <ConvertWorkspace paths={documents} />
          ) : usable && active.key === "ikincigoz" ? (
            <ReviewWorkspace path={documents[0]} />
          ) : usable && active.key === "degisikis" ? (
            <CompareWorkspace paths={documents} />
          ) : usable && active.key === "duzenek" ? (
            <PdfWorkspace paths={documents} />
          ) : (
            <DocumentSurface
              feature={active}
              recents={settings.recentDocuments}
              outcome={outcome}
              openRequest={openRequest}
              onDocuments={openDocuments}
              onForget={forgetDocuments}
            />
          )
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
