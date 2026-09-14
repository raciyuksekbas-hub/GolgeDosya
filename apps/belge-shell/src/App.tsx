import { useCallback, useEffect, useMemo, useState } from "react";
import { Layout } from "./shell/Layout";
import { PreferencesSheet } from "./shell/Settings";
import { featureForRoute, firstAvailableRoute, resolveRoute } from "./shell/routes";
import type { FeatureState, PrefTab, Settings } from "./shell/types";
import * as api from "./shell/api";
import { applyPreferences } from "./shared-ui/theme";
import { DocumentSurface } from "./features/DocumentSurface";
import { ConvertWorkspace } from "./modules/tavzih/ConvertWorkspace";
import { ReviewWorkspace } from "./modules/ikincigoz/ReviewWorkspace";
import { CompareWorkspace } from "./modules/degisikis/CompareWorkspace";
import { PdfWorkspace } from "./modules/duzenek/PdfWorkspace";
import { MODES, carryContext, fileNameOf, type ContextOutcome } from "./shell/modes";
import { Button, Status } from "./shared-ui/primitives";
import { announce } from "./shared-ui/Announcer";
import { useShortcuts } from "./shared-ui/useShortcuts";

export function App() {
  const [features, setFeatures] = useState<FeatureState[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [route, setRoute] = useState("");
  /** Tercihler penceresi — hangi sekmede açıldığı kenar çubuğundan gelir. */
  const [prefsTab, setPrefsTab] = useState<PrefTab | null>(null);
  /** Sürüm — yalnız Hakkında bölümünde. Yapılandırma dizini GÖSTERİLMEZ. */
  const [version, setVersion] = useState<string | null>(null);
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
        setVersion(info.version);
        // Bu üçü yapılandırma dizinini, özellik matrisini ve eski kurulumların
        // yollarını taşır. Konsol da bir yüzeydir: yalnız geliştirmede yazılır.
        if (import.meta.env.DEV) {
          console.debug("[belge] app", info);
          console.debug("[belge] features", f);
          if (report) console.debug("[belge] legacy settings migration", report);
        }
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
        { key: "o", run: () => setOpenRequest((n) => n + 1), enabled: !prefsTab },
        { key: ",", run: () => setPrefsTab("genel") },
      ],
      [prefsTab],
    ),
  );

  /** Bu kip, açık belgelerle şu an çalışabiliyor mu? */
  const usable = useMemo(() => {
    if (!active || documents.length === 0) return false;
    const outcome = context ?? carryContext(active.key, documents);
    return outcome.kind === "keep";
  }, [active, documents, context]);

  /**
   * Yardımcı barın sol ucu: açık belgenin adı; belge yokken kipin adı.
   */
  const barContext = useMemo(() => {
    // Belge yokken bar boş bir bant değildir: kipin adını taşır.
    if (documents.length === 0) {
      return active ? <span className="toolbar-mode">{MODES[active.key].label}</span> : null;
    }
    // Yalnız bu kipin GERÇEKTEN kullandığı belgeler. Seçici çoklu seçime izin
    // veriyor; yedi dosya seçilince bar yedi kırpılmış çipe dönüşüyordu.
    // Fazlalıklar atılmaz — kullanıcı geri döndüğünde yine oradalar.
    const shown = documents.slice(0, active ? MODES[active.key].needs : 1);
    return shown.map((p, i) => (
      <span key={p} className="doc-chip" title={fileNameOf(p)}>
        {i > 0 ? (
          <span className="doc-chip-sep" aria-hidden="true">
            ↔
          </span>
        ) : null}
        <span className="doc-chip-name">{fileNameOf(p)}</span>
      </span>
    ));
  }, [documents, active]);

  if (error) {
    return (
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={setPrefsTab}
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
        onOpenSettings={setPrefsTab}
        context={barContext}
        actions={
          // Home'da bar boştur: tek birincil eylem boş durumdadır (§6, §22).
          documents.length > 0 ? (
            <Button variant="quiet" onClick={closeDocuments}>
              Kapat
            </Button>
          ) : null
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
            // Açılamayan belgeden kurtulma yolu kabuktan geçer: bardaki ad, son
            // kullanılanlar ve çalışma alanı aynı belgeyi göstersin.
            <PdfWorkspace paths={documents} onOpenDocument={openDocuments} />
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
      {prefsTab && settings ? (
        <PreferencesSheet
          settings={settings}
          version={version}
          tab={prefsTab}
          onTab={setPrefsTab}
          onChange={saveSettings}
          onForget={forgetDocuments}
          onClose={() => setPrefsTab(null)}
        />
      ) : null}
    </>
  );
}
